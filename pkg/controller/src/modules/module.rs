use super::OpsRunner;
use super::WasmRuntime;
use crate::runtime::COMPILE_WITH_UNINSTANCIATE;
use futures::future::poll_fn;
use futures::StreamExt;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Context;
use std::task::Poll;
use tracing::debug;

pub struct ControllerModule {
    wasm: WasmRuntime,
    ops_runner: Arc<Mutex<OpsRunner>>,
}

impl ControllerModule {
    pub(crate) fn new(wasm: WasmRuntime, ops_runner: Arc<Mutex<OpsRunner>>) -> Self {
        Self { wasm, ops_runner }
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
        while self.wasm.poll_unpin(cx)?.is_ready() && self.resolve_async_ops(cx)? {}

        if self.wasm.poll_unpin(cx)?.is_pending() {
            return Poll::Pending; // wasm is running, check again later
        }

        // WASM is not running, so the lock will not delay a new op from being added using 'handle_request'
        let runner = self.ops_runner.lock().unwrap();

        let has_pending_ops = !runner.pending_ops.is_empty();
        if !has_pending_ops {
            return Poll::Ready(Ok(()));
        }

        if runner.have_unpolled_ops {
            cx.waker().wake_by_ref();
        }

        if runner.nr_web_calls == 0
            && !self.wasm.is_uninstantiating()
            && *COMPILE_WITH_UNINSTANCIATE
        {
            self.wasm.uninstantiate();
            cx.waker().wake_by_ref();
        }

        Poll::Pending
    }

    fn resolve_async_ops(&mut self, cx: &mut Context) -> anyhow::Result<bool> {
        let maybe_result = {
            // WASM is not running, so the lock will not delay a new op from being added using 'handle_request'
            let mut runner = self.ops_runner.lock().unwrap();

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
            self.wasm
                .wakeup(result.async_request_id, result.value, result.finished)?;

            Ok(true)
        } else {
            Ok(false)
        }
    }
}
