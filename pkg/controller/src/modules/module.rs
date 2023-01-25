use super::OpsRunner;
use super::WasmRuntime;
use crate::runtime::COMPILE_WITH_UNINSTANCIATE;
use chrono::Duration;
use chrono::Utc;
use futures::executor::block_on;
use futures::future::poll_fn;
use futures::StreamExt;
use futures_task::Waker;
use chrono::DateTime;
use crossbeam_channel::{unbounded, Receiver, Sender};
use reqwest::blocking::Client;
use std::borrow::BorrowMut;
use std::collections::VecDeque;
use std::env;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Context;
use std::task::Poll;
use tokio::task::JoinHandle;
use tokio::time::Duration as Durationtk;
use tracing::debug;
use serde::{Deserialize, Serialize};
use serde_json::json;

const BUFFERLENGTH: usize = 10; // how long history of events to be saved
const WAKEUPINTERVAL: u64 = 1000; // ever x seconds wake up polling
const SHUTDOWNINACTIVEINTERVALMS: i64 = 500; // if inactive  for  x ms,  shut down
const TIMEBEFOREPREDICTEDMs : i64  = 1200; // load back in to memory when predicted time is close

#[derive(Deserialize,Debug)]
struct ServerResp{
    prediction:DateTime<Utc>
}


pub struct ControllerModule {
    wasm: WasmRuntime,
    ops_runner: Arc<Mutex<OpsRunner>>,
    thread_spawned: bool,
    tx: Sender<String>,
    rx: Receiver<String>,
    threadhandle: Option<JoinHandle<()>>,
    last_events: VecDeque<DateTime<Utc>>,
    apiserver: String,
    http_client: Client,
    predicted_wakeup: ServerResp
}




// How this  works: variables: Last event  (when  the last event  was i.e last async reques), SHUTDOWNINACTIVEINTERVALMS is  time of inactivity from last event when we want to shutdown, TIMEBEFOREPREDICTEDMs is the time before the predicted  next  wakeup
//  Last event                        shutdown       load back mem                       predicted                                      shutdown if no  event was and prediction  was wrong
//    |_____SHUTDOWNINACTIVEINTERVALMS__|                 | ____TIMEBEFOREPREDICTEDMs________|_________TIMEBEFOREPREDICTEDMs(grace period)________|
//                                                                       we  hope predicted is  right and an event is made here                                                                                

// How polling works https://fasterthanli.me/articles/pin-and-suffering
impl ControllerModule {
    pub(crate) fn new(wasm: WasmRuntime, ops_runner: Arc<Mutex<OpsRunner>>) -> Self {
        debug!("doing new");

        let (tx, rx): (Sender<String>, Receiver<String>) = unbounded();
        let thread_spawned = false;
        let threadhandle = None;
        let mut last_events = VecDeque::with_capacity(BUFFERLENGTH);
        last_events.push_back(Utc::now() );
        let mut apiserver = env::var("PREDICTION_SERVER").unwrap_or("none".to_string());
        apiserver.push_str(&"prediction");

        let http_client = reqwest::blocking::Client::new();
        let predicted_wakeup = ServerResp { prediction: Utc::now() + Duration::days(999) };
        Self {
            wasm,
            ops_runner,
            thread_spawned,
            tx,
            rx,
            threadhandle,
            last_events,
            apiserver,
            http_client,
            predicted_wakeup
        }
    }

    pub async fn start(&mut self) -> anyhow::Result<()> {
        debug!("doing start");

        self.wasm.start_controller()?;

        self.run_event_loop().await?;

        Ok(())
    }

    pub async fn run_event_loop(&mut self) -> anyhow::Result<()> {
        debug!("doing run event loop");
        debug!(self.apiserver);
        poll_fn(|cx| self.poll_event_loop(cx)).await
    }

    pub fn poll_event_loop(&mut self, cx: &mut Context) -> Poll<anyhow::Result<()>> {
        // resolve async ops until wasm is busy or no ops can be resolved
        debug!("doing poll event loop start");

        // spawn a thread that polls this poll function every x seconds, not really super efficient but  no  other solution, we  can't do another  poller inside this  function and call wake_by_ref() as this would just slow this whole function down
        if !self.thread_spawned {
            self.threadhandle = spawn_qeueu_thread(cx.waker(), &self.rx);
            self.thread_spawned = true;
        }

        while self.wasm.poll_unpin(cx)?.is_ready() && self.resolve_async_ops(cx)? {}

        if self.wasm.poll_unpin(cx)?.is_pending() {
            return Poll::Pending; // wasm is running, check again later
        }

        // WASM is not running, so the lock will not delay a new op from being added using 'handle_request'
        let runner = self.ops_runner.lock().unwrap();

        let has_pending_ops = !runner.pending_ops.is_empty();
        // will be be xecuted when all instructions are over, but for operators  this is probably never
        if !has_pending_ops {
            debug!("doing poll ready");
            self.tx
                .send("end thread".to_string())
                .expect("can't send message to  thread to shutdown");
            // wait till  thread is done
            match self.threadhandle.borrow_mut() {
                Some(handle) => {
                    block_on(handle).expect("error waiting on  thread to shutdown");
                }
                None => (),
            }
            return Poll::Ready(Ok(()));
        }

        if runner.have_unpolled_ops {
            //debug!("doing wake by ref");
            cx.waker().wake_by_ref();
        }

        let current_time = Utc::now();

        if self.wasm.is_uninstantiating() && self.predicted_wakeup.prediction.signed_duration_since(current_time).num_milliseconds() < 0 {
            // something is wrong, we current time is past  predicted time, deadline missed
            debug!("doing predicted time is in past, reset");
            self.predicted_wakeup  = ServerResp { prediction :   current_time + Duration::days(999) };
        }

        debug!("doing isinst {:?} current {:?} predicted {:?} since last {:?}", self.wasm.is_uninstantiating(),current_time, self.predicted_wakeup.prediction, *self.last_events.back().unwrap()
    );

        // check predicted time if available and  if predicted time  is close to current time, reload from  disk if it was unloaded
        if self.wasm.is_uninstantiating() && 
        self.predicted_wakeup.prediction.signed_duration_since(current_time).num_milliseconds() < TIMEBEFOREPREDICTEDMs {
            debug!("doing load in memory");
            self.wasm.load_to_mem();
            

        }

        if runner.nr_web_calls == 0
            && !self.wasm.is_uninstantiating()
            && *COMPILE_WITH_UNINSTANCIATE 
            // only shutdown not direct but after x milliseconds of inactive
            && current_time.signed_duration_since(*self.last_events.back().unwrap()).num_milliseconds() > SHUTDOWNINACTIVEINTERVALMS 
             // do not shut down when we see  in the future predicted is coming
            && !(self.predicted_wakeup.prediction.signed_duration_since(current_time).num_milliseconds().abs() < TIMEBEFOREPREDICTEDMs)

        {
            debug!("doing uninstatniate");
            self.wasm.uninstantiate();
            //debug!("doing uninstatniate done ");
            // call a API here given event history and wake it up  in time before  message comes in
            cx.waker().wake_by_ref();

            let body = json!({ "history": self.last_events });

            match self.http_client.post(self.apiserver.clone()).json(&body)
            .send() {
                Ok(resp) => {
                    
                    self.predicted_wakeup = resp.json().unwrap();
                    debug!("doing predicted time is {:?}", self.predicted_wakeup  );
                }
                Err(e) => {
                    debug!("doing error {:?}", e)
                }
            }
        }

        debug!("doing pend end event");
        Poll::Pending
    }

    fn resolve_async_ops(&mut self, cx: &mut Context) -> anyhow::Result<bool> {
        let maybe_result = {
            // WASM is not running, so the lock will not delay a new op from being added using 'handle_request'
            let mut runner = self.ops_runner.lock().unwrap();
            //debug!("doing resolve async nr calls : {:?}", runner.nr_web_calls);

            runner.have_unpolled_ops = false;

            // Check if any async requests errored
            loop {
                let poll_result = runner.pending_ops.poll_next_unpin(cx);

                if let Poll::Ready(Some(Ok(val))) = poll_result {
                    if val {
                        runner.nr_web_calls -= 1;
                    }
                }

                if let Poll::Ready(Some(Err(err))) = poll_result {
                    debug!("found error {:?}", err);
                    return Err(err);
                }

                if let Poll::Ready(Some(result)) = runner.async_result_rx.poll_recv(cx) {
                    break Some(result);
                }

                if let Poll::Ready(None) | Poll::Pending = poll_result {
                    break None;
                }
            }
        };

        // Retrieve async request results & start wasm again
        if let Some(result) = maybe_result {
            

            debug!("doing wakeup request unin is : {:?}", self.wasm.is_uninstantiating() );
            let now_timestamp = Utc::now();
            self.add_event_time(now_timestamp);
            // wakeup doesn't always  "wake up from disk"
            self.wasm
                .wakeup(result.async_request_id, result.value, result.finished)?;

            Ok(true)
        } else {
            //debug!("doing not wakeup");
            Ok(false)
        }
    }

    fn add_event_time(&mut self, time: DateTime<Utc>) {
        if self.last_events.len() >= BUFFERLENGTH {
            self.last_events.pop_front();
        }
        self.last_events.push_back(time);
    }
}

// TODO maybe not  do  this but poll  within? https://fasterthanli.me/articles/pin-and-suffering
fn spawn_qeueu_thread(waker: &Waker, rx: &Receiver<String>) -> Option<JoinHandle<()>> {
    debug!("doing spawning thread");
    //self.thread_spawned = true;
    let waker = waker.clone();
    let rx2 = rx.clone();
    let spawn = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Durationtk::from_millis(WAKEUPINTERVAL)).await;
            debug!("doing other thread wakebyref");
            waker.wake_by_ref();
            let try_result = rx2.try_recv();
            match try_result {
                Err(_) => {}
                Ok(_msg) => break,
            }
        }
        debug!("ending spawned thread");
    });
    return Some(spawn);
    //self.threadhandle = Some(spawn);
}
