use crate::modules::OpsRunner;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::sync::Mutex;
use wasmtime_wasi::WasiCtx;

pub struct ControllerCtx {
    pub wasi_ctx: WasiCtx,
    pub async_client_id: u64,
    pub async_request_id_counter: Arc<AtomicU64>,
    pub ops_runner: Arc<Mutex<OpsRunner>>,
}

impl ControllerCtx {
    pub fn new(wasi_ctx: WasiCtx, async_client_id: u64, ops_runner: Arc<Mutex<OpsRunner>>) -> Self {
        let async_request_id_counter = Arc::new(AtomicU64::new(0));
        ControllerCtx {
            wasi_ctx,
            async_client_id,
            async_request_id_counter,
            ops_runner,
        }
    }
}
