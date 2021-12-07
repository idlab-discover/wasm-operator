use crate::abi::AsyncRequest;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use wasmtime_wasi::WasiCtx;

pub struct ControllerCtx {
    pub wasi_ctx: WasiCtx,
    pub async_client_id: u64,
    pub async_request_id_counter: Arc<AtomicU64>,
    pub async_request_sender: UnboundedSender<AsyncRequest>,
}

impl ControllerCtx {
    pub fn new(
        wasi_ctx: WasiCtx,
        async_client_id: u64,
        async_request_sender: UnboundedSender<AsyncRequest>,
    ) -> Self {
        let async_request_id_counter = Arc::new(AtomicU64::new(0));
        ControllerCtx {
            wasi_ctx,
            async_client_id,
            async_request_id_counter,
            async_request_sender,
        }
    }
}
