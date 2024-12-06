use super::OpsRunner;
use super::WasmRuntime;
use crate::runtime::COMPILE_WITH_UNINSTANTIATE;
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

const BUFFER_LENGTH: usize = 50; // how long history of events to be saved
const SHUTDOWN_INACTIVE_INTERVAL_MS: i64 = 1000; // if inactive for x ms, shut down
const TIME_BEFORE_PREDICTED_MS: i64 = 1000; // load back in to memory when predicted time is close
const GRACE_PERIOD_MS: i64 = 1000; // keep in memory period after prediction time

#[derive(Deserialize, Debug)]
struct ServerResp {
    prediction: DateTime<Utc>,
}

pub struct ControllerModule {
    wasm: WasmRuntime,
    ops_runner: Arc<Mutex<OpsRunner>>,
    last_event_time: DateTime<Utc>,
    last_events: VecDeque<DateTime<Utc>>,
    api_server: String,
    http_client: Client,
    predicted_wakeup: ServerResp,
    sleep_vec: Vec<Pin<Box<Sleep>>>,
    first_event_after_shutdown: bool,
}

// How this works: variables: Last event (when the last event was i.e last async request), SHUTDOWN_INACTIVE_INTERVAL_MS is time of inactivity from last event when we want to shutdown, TIME_BEFORE_PREDICTED_MS is the time before the predicted next wakeup
//  Last event                        shutdown       load back mem                       predicted                     shutdown if no  event was and prediction  was wrong
//    |_____SHUTDOWN_INACTIVE_INTERVAL_MS__|                 | ____TIME_BEFORE_PREDICTED_MS________|_________GRACE_PERIOD_MS________|
//                                                                       we  hope predicted is  right and an event is made here

// How polling works https://fasterthanli.me/articles/pin-and-suffering
// we do cx.waker().wake_by_ref(); to wake up the poll, and importantly wake up wasm work when we set it, always do a wake after it!

impl ControllerModule {
    pub(crate) fn new(wasm: WasmRuntime, ops_runner: Arc<Mutex<OpsRunner>>) -> Self {
        debug!("doing new");

        let mut last_events = VecDeque::with_capacity(BUFFER_LENGTH);
        let last_event_time = Utc::now();
        last_events.push_back(Utc::now());
        let mut api_server = env::var("PREDICTION_SERVER").unwrap_or("none".to_string());
        api_server.push_str("prediction");

        let http_client = reqwest::blocking::Client::new();
        let predicted_wakeup = ServerResp {
            prediction: Utc::now() + Duration::days(999),
        };
        let sleep_vec = vec![];
        let first_event_after_shutdown = true;
        Self {
            wasm,
            ops_runner,
            last_events,
            api_server,
            http_client,
            predicted_wakeup,
            sleep_vec,
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

        // will be be executed when all instructions are over, but for operators this is probably never
        if runner.pending_ops.is_empty() {
            // TODO what happens to the wakeups still in the sleep list when context is ready
            //cx.waker().wake_by_ref();
            return Poll::Ready(Ok(()));
        }

        if runner.have_unpolled_ops {
            // do we need this wake up
            cx.waker().wake_by_ref();
        }

        let current_time = Utc::now();

        if current_time
            .signed_duration_since(self.predicted_wakeup.prediction)
            .num_milliseconds()
            > 0
            && !in_time_grace_period(&current_time, &self.predicted_wakeup.prediction)
        {
            // something is wrong, we current time is past predicted time, deadline missed
            debug!("predicted time is in past, reset");
            self.predicted_wakeup = ServerResp {
                prediction: current_time + Duration::days(999),
            };

            //todo maybe do wakeup
            cx.waker().wake_by_ref();
        }
        //debug!("doing isinst {:?} current {:?} predicted {:?} since last {:?}", self.wasm.is_uninstantiating(),current_time, self.predicted_wakeup.prediction, *self.last_events.back().unwrap());

        // check predicted time if available and if predicted time is close to current time, reload from disk if it was unloaded
        if self.wasm.is_uninstantiating()
            && in_time_before_prediction_period(&current_time, &self.predicted_wakeup.prediction)
        {
            debug!("doing signal  load in memory");
            self.wasm.load_to_mem();
            cx.waker().wake_by_ref();
            //wake up again after graceperiod todo better calculation than grace+timebefore for quicker
            let mut sleep = Box::pin(tokio::time::sleep(Durationtk::from_millis(
                (GRACE_PERIOD_MS + TIME_BEFORE_PREDICTED_MS) as u64,
            )));
            sleep.poll_unpin(cx);
            self.sleep_vec.push(sleep);
        }

        if runner.nr_web_calls == 0
            && !self.wasm.is_uninstantiating()
            && *COMPILE_WITH_UNINSTANTIATE
            // only shutdown not direct but after x milliseconds of inactive
            && is_inactive_period(&current_time, &self.last_event_time)
            // do not shut down when we see in the future predicted is coming
            && ! in_time_before_prediction_period(&current_time, &self.predicted_wakeup.prediction)
            && ! in_time_grace_period(&current_time, &self.predicted_wakeup.prediction)
        {
            debug!("doing signal uninstantiate");
            self.wasm.uninstantiate();

            cx.waker().wake_by_ref();
            // call a API here given event history and wake it up in time before message comes in
            //if no async func was called, then we know the prediction failed and we don't do another prediction since this will give same date...
            if !self.first_event_after_shutdown {
                let body = json!({ "history": self.last_events ,  "function": "SES"});
                match self
                    .http_client
                    .post(self.api_server.clone())
                    .json(&body)
                    .send()
                {
                    Ok(resp) => {
                        self.predicted_wakeup = resp.json().unwrap();
                        debug!("doing predicted time is {:?}", self.predicted_wakeup);

                        // wakup before we think predicted is incoming (need min x duration before load is finished)
                        // TODO assume date is always in future

                        let mut next_time = (self.predicted_wakeup.prediction - Utc::now())
                            .num_milliseconds()
                            - TIME_BEFORE_PREDICTED_MS
                            + 5;
                        // make it positive but maybe throw error if neg instead or do new prediction
                        next_time = next_time.abs();

                        let mut sleep = Box::pin(tokio::time::sleep(Durationtk::from_millis(
                            next_time as u64,
                        )));
                        sleep.poll_unpin(cx);
                        self.sleep_vec.push(sleep);
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
        self.sleep_vec.retain(|e| !e.is_elapsed());

        //debug!("doing pend end event");

        // TODO: maybe check if it is empty do a wakeup ever x min just to be sure if some timing goes wrong? or wait till next event and will auto wakeup
        Poll::Pending
    }

    fn resolve_async_ops(&mut self, cx: &mut Context) -> anyhow::Result<bool> {
        let maybe_result = {
            // WASM is not running, so the lock will not delay a new op from being added using 'handle_request'
            let mut runner = self.ops_runner.lock().unwrap();
            //debug!("doing resolve async nr calls : {:?} and unpolled {:?} is active {:?}", runner.nr_web_calls,runner.have_unpolled_ops,!self.wasm.is_uninstantiating());

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
                    //should never happen, currently error handeling is not well implemented I think, if web connection times out we crash
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
            // use wakeup timings instead of requests
            if self.first_event_after_shutdown {
                self.first_event_after_shutdown = false;

                //reset failed prediction
                let now_timestamp = Utc::now();
                self.add_event_time(now_timestamp);
                // wakeup doesn't always "wake up from disk"

                // let mut sleep = Box::pin(tokio::time::sleep(Durationtk::from_millis((SHUTDOWN_INACTIVE_INTERVAL_MS + 10) as u64)));
                //debugnextwakeup(SHUTDOWN_INACTIVE_INTERVAL_MS + 10);
                //sleep.poll_unpin(cx);
                //self.sleep_vec.push(sleep);
            }

            // our prediction failed, just set it far away
            if self.wasm.is_uninstantiating() {
                debug!("prediction failed, we got request when inactive");
                self.predicted_wakeup = ServerResp {
                    prediction: Utc::now() + Duration::days(999),
                };
            }

            // wake up after unactive interval
            if result.finished {
                self.last_event_time = Utc::now();
                let mut sleep = Box::pin(tokio::time::sleep(Durationtk::from_millis(
                    (SHUTDOWN_INACTIVE_INTERVAL_MS + 10) as u64,
                )));
                sleep.poll_unpin(cx);
                self.sleep_vec.push(sleep);
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
        if self.last_events.len() >= BUFFER_LENGTH {
            self.last_events.pop_front();
        }
        self.last_events.push_back(time);
        debug!("added event time {:?}", time);
    }
}

//        load back mem                       predicted                            CURRENT
//               | ____TIME_BEFORE_PREDICTED_MS________|_________GRACE_PERIOD_MS________|
//

fn in_time_before_prediction_period(
    current_time: &DateTime<Utc>,
    predicted_time: &DateTime<Utc>,
) -> bool {
    let difference = predicted_time
        .signed_duration_since(*current_time)
        .num_milliseconds();
    difference > 0 && difference < TIME_BEFORE_PREDICTED_MS
}

fn in_time_grace_period(current_time: &DateTime<Utc>, predicted_time: &DateTime<Utc>) -> bool {
    let difference = current_time
        .signed_duration_since(*predicted_time)
        .num_milliseconds();
    difference > 0 && difference < GRACE_PERIOD_MS
}

//  Last event                        shutdown/CURRENTTIME
//    |_____SHUTDOWN_INACTIVE_INTERVAL_MS__|
fn is_inactive_period(current_time: &DateTime<Utc>, last_event: &DateTime<Utc>) -> bool {
    let difference = current_time
        .signed_duration_since(*last_event)
        .num_milliseconds();
    difference > SHUTDOWN_INACTIVE_INTERVAL_MS
}
