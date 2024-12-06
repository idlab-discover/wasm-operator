use super::OpsRunner;
use super::WasmRuntime;
use crate::runtime::COMPILE_WITH_UNINSTANCIATE;
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use futures::future::poll_fn;
use futures::FutureExt;
use futures::StreamExt;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;
use std::collections::VecDeque;
use std::env;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Context;
use std::task::Poll;
use tokio::time::Duration as Durationtk;
use tokio::time::Sleep;
use tracing::debug;

const BUFFERLENGTH: usize = 50; // how long history of events to be saved
const SHUTDOWNINACTIVEINTERVALMS: i64 = 1000; // if inactive  for  x ms,  shut down
const TIMEBEFOREPREDICTEDMS: i64 = 1000; // load back in to memory when predicted time is close
const GRACEPERIODMS: i64 = 1000; // keep in  memory period after prediction time

#[derive(Deserialize, Debug)]
struct ServerResp {
    prediction: DateTime<Utc>,
}

pub struct ControllerModule {
    wasm: WasmRuntime,
    ops_runner: Arc<Mutex<OpsRunner>>,
    last_event_time: DateTime<Utc>,
    last_events: VecDeque<DateTime<Utc>>,
    apiserver: String,
    http_client: Client,
    predicted_wakeup: ServerResp,
    sleepvec: Vec<Pin<Box<Sleep>>>,
    first_event_after_shutdown: bool,
}

// How this  works: variables: Last event  (when  the last event  was i.e last async reques), SHUTDOWNINACTIVEINTERVALMS is  time of inactivity from last event when we want to shutdown, TIMEBEFOREPREDICTEDMs is the time before the predicted  next  wakeup
//  Last event                        shutdown       load back mem                       predicted                     shutdown if no  event was and prediction  was wrong
//    |_____SHUTDOWNINACTIVEINTERVALMS__|                 | ____TIMEBEFOREPREDICTEDMs________|_________GRACEPERIODMs________|
//                                                                       we  hope predicted is  right and an event is made here

// How polling works https://fasterthanli.me/articles/pin-and-suffering
// we do cx.waker().wake_by_ref();  to wake up  the poll, and  importantly wake  up wasm  work when we  set it, always  do  a wake after it!

impl ControllerModule {
    pub(crate) fn new(wasm: WasmRuntime, ops_runner: Arc<Mutex<OpsRunner>>) -> Self {
        debug!("doing new");

        let mut last_events = VecDeque::with_capacity(BUFFERLENGTH);
        let last_event_time = Utc::now();
        last_events.push_back(Utc::now());
        let mut apiserver = env::var("PREDICTION_SERVER").unwrap_or("none".to_string());
        apiserver.push_str("prediction");

        let http_client = reqwest::blocking::Client::new();
        let predicted_wakeup = ServerResp {
            prediction: Utc::now() + Duration::days(999),
        };
        let sleepvec = vec![];
        let first_event_after_shutdown = true;
        Self {
            wasm,
            ops_runner,
            last_events,
            apiserver,
            http_client,
            predicted_wakeup,
            sleepvec,
            first_event_after_shutdown,
            last_event_time,
        }
    }

    pub async fn start(&mut self) -> anyhow::Result<()> {
        self.wasm.start_controller()?;
        self.run_event_loop().await?;

        Ok(())
    }

    pub async fn run_event_loop(&mut self) -> anyhow::Result<()> {
        poll_fn(|cx| self.poll_event_loop(cx)).await
    }

    pub fn poll_event_loop(&mut self, cx: &mut Context) -> Poll<anyhow::Result<()>> {
        // resolve async ops until wasm is busy or no ops can be resolved
        //debug!("doing poll event loop start");

        while self.wasm.poll_unpin(cx)?.is_ready() && self.resolve_async_ops(cx)? {}

        if self.wasm.poll_unpin(cx)?.is_pending() {
            return Poll::Pending; // wasm is running, check again later
        }

        // WASM is not running, so the lock will not delay a new op from being added using 'handle_request'
        let runner = self.ops_runner.lock().unwrap();

        // will be be executed when all instructions are over, but for operators  this is probably never
        if runner.pending_ops.is_empty() {
            // TODO what happends to the wakups still in the sleep list when context is ready
            //cx.waker().wake_by_ref();
            return Poll::Ready(Ok(()));
        }

        if runner.have_unpolled_ops {
            // do we need  this wake up
            cx.waker().wake_by_ref();
        }

        let current_time = Utc::now();

        if current_time
            .signed_duration_since(self.predicted_wakeup.prediction)
            .num_milliseconds()
            > 0
            && !in_time_grace_period(&current_time, &self.predicted_wakeup.prediction)
        {
            // something is wrong, we current time is past  predicted time, deadline missed
            debug!("predicted time is in past, reset");
            self.predicted_wakeup = ServerResp {
                prediction: current_time + Duration::days(999),
            };

            //todo maybe do wakeup
            cx.waker().wake_by_ref();
        }
        //debug!("doing isinst {:?} current {:?} predicted {:?} since last {:?}", self.wasm.is_uninstantiating(),current_time, self.predicted_wakeup.prediction, *self.last_events.back().unwrap());

        // check predicted time if available and  if predicted time  is close to current time, reload from  disk if it was unloaded
        if self.wasm.is_uninstantiating()
            && in_time_before_prediction_period(&current_time, &self.predicted_wakeup.prediction)
        {
            debug!("doing signal  load in memory");
            self.wasm.load_to_mem();
            cx.waker().wake_by_ref();
            //wake up  again  after graceperiod todo better calculation than grace+timebefore for  quicker
            let mut sleep = Box::pin(tokio::time::sleep(Durationtk::from_millis(
                (GRACEPERIODMS + TIMEBEFOREPREDICTEDMS) as u64,
            )));
            sleep.poll_unpin(cx);
            self.sleepvec.push(sleep);
        }

        if runner.nr_web_calls == 0
            && !self.wasm.is_uninstantiating()
            && *COMPILE_WITH_UNINSTANCIATE
            // only shutdown not direct but after x milliseconds of inactive
            && is_inactive_period(&current_time, &self.last_event_time)
             // do not shut down when we see  in the future predicted is coming 
            && ! in_time_before_prediction_period(&current_time, &self.predicted_wakeup.prediction)
            && ! in_time_grace_period(&current_time, &self.predicted_wakeup.prediction)
        {
            debug!("doing signal uninstatniate");
            self.wasm.uninstantiate();

            cx.waker().wake_by_ref();
            // call a API here given event history and wake it up  in time before  message comes in
            //if no async funnc was  called,  then  we  no  the prediction failed and  we  dont do another predition since this  will give same  date...
            if !self.first_event_after_shutdown {
                let body = json!({ "history": self.last_events ,  "function": "SES"});
                match self
                    .http_client
                    .post(self.apiserver.clone())
                    .json(&body)
                    .send()
                {
                    Ok(resp) => {
                        self.predicted_wakeup = resp.json().unwrap();
                        debug!("doing predicted time is {:?}", self.predicted_wakeup);

                        // wakup  before we think predicted is incoming (need min x duration before load is fisinished)
                        // TODO assume date is always in future

                        let mut nexttime = (self.predicted_wakeup.prediction - Utc::now())
                            .num_milliseconds()
                            - TIMEBEFOREPREDICTEDMS
                            + 5;
                        // make it positive but maybe throw  error if neg  instead or do new prediction
                        nexttime = nexttime.abs();

                        let mut sleep =
                            Box::pin(tokio::time::sleep(Durationtk::from_millis(nexttime as u64)));
                        sleep.poll_unpin(cx);
                        self.sleepvec.push(sleep);
                    }
                    Err(e) => {
                        debug!("doing error {:?}", e)
                    }
                }
            } else {
                debug!("don't predict next loop since last one failed");
            }
            self.first_event_after_shutdown = true;
        }

        // remove all old wakeups from vector
        self.sleepvec.retain(|e| !e.is_elapsed());

        //debug!("doing pend end event");

        // todo:  maybe check if it is empty do a wakeup ever x min just to be sure if some  timing goes wrong? or wait till next event and will auto wakeup
        Poll::Pending
    }

    fn resolve_async_ops(&mut self, cx: &mut Context) -> anyhow::Result<bool> {
        let maybe_result = {
            // WASM is not running, so the lock will not delay a new op from being added using 'handle_request'
            let mut runner = self.ops_runner.lock().unwrap();
            //debug!("doing resolve async nr calls : {:?}  and  unpolled {:?} is  active {:?}", runner.nr_web_calls,runner.have_unpolled_ops,!self.wasm.is_uninstantiating());

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
                    //should never happen, currently error handeling is not  well implemented i think, if web connection times out we crash
                    runner.nr_web_calls -= 0;
                    //break None;
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
            // use  wakeup timings instead of requests
            if self.first_event_after_shutdown {
                self.first_event_after_shutdown = false;

                //reset failed  prediction
                let now_timestamp = Utc::now();
                self.add_event_time(now_timestamp);
                // wakeup doesn't always  "wake up from disk"

                // let mut sleep = Box::pin(tokio::time::sleep(Durationtk::from_millis((SHUTDOWNINACTIVEINTERVALMS + 10) as u64)));
                //debugnextwakeup(SHUTDOWNINACTIVEINTERVALMS + 10);
                //sleep.poll_unpin(cx);
                //self.sleepvec.push(sleep);
            }

            // our prediction failed, just set it  far away
            if self.wasm.is_uninstantiating() {
                debug!("prediction failed, we got request when inactive");
                self.predicted_wakeup = ServerResp {
                    prediction: Utc::now() + Duration::days(999),
                };
            }

            // wake  up after unactive interval
            if result.finished {
                self.last_event_time = Utc::now();
                let mut sleep = Box::pin(tokio::time::sleep(Durationtk::from_millis(
                    (SHUTDOWNINACTIVEINTERVALMS + 10) as u64,
                )));
                sleep.poll_unpin(cx);
                self.sleepvec.push(sleep);
            }

            self.wasm
                .wakeup(result.async_request_id, result.value, result.finished)?;

            Ok(true)
        } else {
            //debug!("doing false resolve async");
            Ok(false)
        }
    }

    fn add_event_time(&mut self, time: DateTime<Utc>) {
        if self.last_events.len() >= BUFFERLENGTH {
            self.last_events.pop_front();
        }
        self.last_events.push_back(time);
        debug!("added event time {:?}", time);
    }
}

//        load back mem                       predicted                            CURRENT
//               | ____TIMEBEFOREPREDICTEDMs________|_________GRACEPERIODMs________|
//

fn in_time_before_prediction_period(
    current_time: &DateTime<Utc>,
    predicted_time: &DateTime<Utc>,
) -> bool {
    let difference = predicted_time
        .signed_duration_since(*current_time)
        .num_milliseconds();
    difference > 0 && difference < TIMEBEFOREPREDICTEDMS
}

fn in_time_grace_period(current_time: &DateTime<Utc>, predicted_time: &DateTime<Utc>) -> bool {
    let difference = current_time
        .signed_duration_since(*predicted_time)
        .num_milliseconds();
    difference > 0 && difference < GRACEPERIODMS
}

//  Last event                        shutdown/CURRENTTIME
//    |_____SHUTDOWNINACTIVEINTERVALMS__|
fn is_inactive_period(current_time: &DateTime<Utc>, lastevent: &DateTime<Utc>) -> bool {
    let difference = current_time
        .signed_duration_since(*lastevent)
        .num_milliseconds();
    difference > SHUTDOWNINACTIVEINTERVALMS
}
