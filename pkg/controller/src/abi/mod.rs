use crate::runtime::controller_ctx::ControllerCtx;
use std::sync::atomic::Ordering;
use std::time::Duration;
use wasmtime::{AsContextMut, Caller, Instance, Linker};

pub mod abicommand;
pub mod opcall;

use crate::runtime::http_engine::HttpRequest;
pub use abicommand::AsyncRequestValue;

pub fn register_imports(linker: &mut Linker<ControllerCtx>) -> anyhow::Result<()> {
    linker.func_wrap("http-proxy-abi", "request", abi_request)?;
    linker.func_wrap("http-proxy-abi", "request_stream", abi_request)?;
    linker.func_wrap("delay-abi", "delay", abi_delay)?;

    Ok(())
}

pub(crate) async fn start_controller<S>(mut store: S, instance: &Instance) -> anyhow::Result<()>
where
    S: AsContextMut,
    S::Data: Send,
{
    instance
        .get_typed_func::<(), (), _>(&mut store, "_start")?
        .call(&mut store, ())?;

    Ok(())
}

pub(crate) async fn allocate<S>(
    mut store: S,
    instance: &Instance,
    allocation_size: u32,
) -> anyhow::Result<u32, wasmtime::Trap>
where
    S: AsContextMut,
    S::Data: Send,
{
    instance
        .get_typed_func::<u32, u32, _>(&mut store, "allocate")?
        .call(&mut store, allocation_size)
}

// TODO maybe make abi for memory loading??

pub(crate) async fn wakeup<S>(
    mut store: S,
    instance: &Instance,
    async_request_id: u64,
    value: Option<bytes::Bytes>,
    finished: bool,
) -> anyhow::Result<()>
where
    S: AsContextMut,
    S::Data: Send,
{
    // allocates the memory of a request
    let (memory_location_ptr, memory_location_size) = match value {
        None => (std::ptr::null::<*const u32>() as u32, 0),
        Some(event) => {
            let memory_location_size = event.len();
            let memory_location_ptr =
                allocate(&mut store, instance, memory_location_size as u32).await?;
            let memory = instance
                .get_memory(&mut store, "memory")
                .expect("memory not found");

            memory.write(&mut store, memory_location_ptr as usize, &event)?;

            (memory_location_ptr, memory_location_size)
        }
    };

    let wakeup_fn = instance.get_typed_func::<(u64, u32, u32, u32), (), _>(&mut store, "wakeup")?;

    wakeup_fn.call(
        &mut store,
        (
            async_request_id,
            if finished { 1 } else { 0 },
            memory_location_ptr,
            memory_location_size as u32,
        ),
    )?;

    Ok(())
}

fn abi_request(mut caller: Caller<'_, ControllerCtx>, ptr: u32, size: u32, stream: u32) -> u64 {
    let inner_request: HttpRequest<Vec<u8>> = {
        let memory = caller
            .get_export("memory")
            .expect("no memory found")
            .into_memory()
            .expect("no memory found");

        let inner_req_bytes =
            &memory.data_mut(caller.as_context_mut())[(ptr as usize)..((ptr + size) as usize)];

        bincode::deserialize(inner_req_bytes).expect("deserialize failed")
    };

    let controller_ctx = caller.data_mut();

    let async_request_id = controller_ctx
        .async_request_id_counter
        .fetch_add(1, Ordering::SeqCst);

    controller_ctx.ops_runner.lock().unwrap().handle_request(
        async_request_id,
        (if stream == 0 {
            AsyncRequestValue::Http
        } else {
            AsyncRequestValue::HttpStream
        })(inner_request.into()),
    );

    async_request_id
}

fn abi_delay(mut caller: Caller<'_, ControllerCtx>, millis: u64) -> u64 {
    let controller_ctx = caller.data_mut();

    let async_request_id = controller_ctx
        .async_request_id_counter
        .fetch_add(1, Ordering::SeqCst);

    controller_ctx.ops_runner.lock().unwrap().handle_request(
        async_request_id,
        AsyncRequestValue::Delay(Duration::from_millis(millis)),
    );

    async_request_id
}
