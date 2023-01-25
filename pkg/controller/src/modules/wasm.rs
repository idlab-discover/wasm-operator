use crate::runtime::controller_ctx::ControllerCtx;
use crate::runtime::Environment;
use futures::future::BoxFuture;
use futures::FutureExt;
use std::fmt;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::OwnedSemaphorePermit as AsyncOwnedSemaphorePermit;
use tokio::sync::Semaphore as AsyncSemaphore;
use tracing::Instrument;
use wasmtime::{Instance, Module, Store};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

const WASM_PAGE_SIZE: u64 = 0x10000;

pub struct Snapshot {
    pub globals: Vec<(String, wasmtime::Val)>,
    pub memory_min: usize,
}

enum MaybeInst {
    Locked,
    NotInst(ControllerCtx),// used if not initialised i.e at the beginning
    UnsInst(ControllerCtx, Snapshot), // used if  the wasm module is cached because not used, so on disk
    GotInst(    // used when the  wasm module is still in memmory
        Store<ControllerCtx>,
        AsyncOwnedSemaphorePermit,
        Instance,
    ),
}

impl MaybeInst {
    pub(crate) fn take_not(&mut self) -> Self {
        match self {
            Self::NotInst(_) => self.take(),
            _ => Self::Locked,
        }
    }

    pub(crate) fn take_uns(&mut self) -> Self {
        match self {
            Self::UnsInst(_, _) => self.take(),
            _ => Self::Locked,
        }
    }

    pub(crate) fn take_got(&mut self) -> Self {
        match self {
            Self::GotInst(_, _, _) => self.take(),
            _ => Self::Locked,
        }
    }

    pub(crate) fn set(&mut self, mut val: Self) -> Self {
        std::mem::swap(self, &mut val);
        val
    }

    fn take(&mut self) -> Self {
        self.set(Self::Locked)
    }
}

impl fmt::Debug for MaybeInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Locked => f.debug_struct("Locked"),
            Self::NotInst(_) => f.debug_struct("NotInst(ctx)"),
            Self::UnsInst(_, _) => f.debug_struct("UnsInst(ctx, snapshot)"),
            Self::GotInst(_, _, _) => f.debug_struct("GotInst(store, permit, instance)"),
        }
        .finish()
    }
}

pub struct WasmRuntime {
    inner: Arc<AsyncMutex<MaybeInst>>,

    pub wasm_work: Option<BoxFuture<'static, anyhow::Result<()>>>,
    uninstantiating: bool,

    wasm_path: std::path::PathBuf,
    swap_path: std::path::PathBuf,
    environment: Environment,

    async_active_client_counter: Arc<AsyncSemaphore>,
}

impl fmt::Debug for WasmRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WasmRuntime")
            .field("uninstantiating", &self.uninstantiating)
            .field("swap_path", &self.swap_path)
            .field(
                "wasm_work",
                if self.wasm_work.is_none() {
                    &"None"
                } else {
                    &"Some(...)"
                },
            )
            .finish()
    }
}

impl WasmRuntime {
    pub(crate) fn new(
        controller_ctx: ControllerCtx,
        wasm_path: std::path::PathBuf,
        swap_path: std::path::PathBuf,
        environment: Environment,
        async_active_client_counter: Arc<AsyncSemaphore>,
    ) -> Self {
        Self {
            inner: Arc::new(AsyncMutex::new(MaybeInst::NotInst(controller_ctx))),

            wasm_work: None,
            uninstantiating: true,

            wasm_path,
            swap_path,
            environment,

            async_active_client_counter,
        }
    }

    pub(crate) fn is_uninstantiating(&self) -> bool {
        self.uninstantiating
    }

    pub(crate) fn uninstantiate(&mut self) {
        assert!(self.wasm_work.is_none());

        let arc = self.inner.clone();
        let swap_path = self.swap_path.clone();

        let fut = async move {
            let mut lock = arc.lock().await;
            // check if wasm module is in memmory,  if so  cache it (should always be the case?)
            if let MaybeInst::GotInst(mut store, permit, instance) = lock.take_got() { 
                let mem = instance.get_memory(&mut store, "memory").unwrap();
                tokio::fs::write(&swap_path, mem.data(&mut store)).await?;

                let mut globals: Vec<(String, wasmtime::Global)> = instance
                    .exports(&mut store)
                    .filter_map(|exp| {
                        let glob = exp.clone().into_global();

                        glob.map(|x| (exp.name().to_string(), x))
                    })
                    .collect();

                globals = globals
                    .into_iter()
                    .filter(|(_, glob)| {
                        glob.ty(&mut store).mutability() == wasmtime::Mutability::Var
                    })
                    .collect();

                let global_vals = globals
                    .into_iter()
                    .map(|(name, glob)| (name, glob.get(&mut store)))
                    .collect();

                let snapshot = Snapshot {
                    globals: global_vals,
                    memory_min: mem.data_size(&mut store),
                };

                lock.set(MaybeInst::UnsInst(store.into_data(), snapshot));

                drop(permit);
            }

            Ok(())
        }
        .boxed();

        self.set_wasm_work(fut, "uninstantiate");
        self.uninstantiating = true;
    }

    pub(crate) fn start_controller(&mut self) -> anyhow::Result<()> {
        assert!(self.wasm_work.is_none());
        let arc = self.inner.clone();
        let environment = self.environment.clone();
        let wasm_path = self.wasm_path.clone();
        let async_active_client_counter_clone = self.async_active_client_counter.clone();

        let fut = async move {
            let mut lock = arc.lock().await;
            // check if module is never initialised, should always  be the case since  this funcion only once get called when  first  starting
            if let MaybeInst::NotInst(context) = lock.take_not() {
                let permit = async_active_client_counter_clone.acquire_owned().await?;

                let mut store = Store::new(&environment.engine, context);

                let module = unsafe { Module::deserialize_file(&environment.engine, &wasm_path)? };

                let pre_instance = environment.linker.instantiate_pre(&mut store, &module)?;

                drop(module);

                let instance = pre_instance.instantiate(&mut store)?;

                lock.set(MaybeInst::GotInst(
                    store,
                    permit,
                    instance,
                ));
            }

            let (store, instance) = match &mut *lock {
                MaybeInst::GotInst(store, _, instance) => (store, instance),
                _ => unreachable!(),
            };

            crate::abi::start_controller(store, instance).await?;

            Ok(())
        }
        .boxed();

        self.set_wasm_work(fut, "start_controller");
        self.uninstantiating = false;

        Ok(())
    }

    pub(crate) fn wakeup(
        &mut self,
        async_request_id: u64,
        value: Option<bytes::Bytes>,
        finished: bool,
    ) -> anyhow::Result<()> {
        assert!(self.wasm_work.is_none());
        let arc = self.inner.clone();
        let environment = self.environment.clone();
        let swap_path = self.swap_path.clone();
        let wasm_path = self.wasm_path.clone();
        let async_active_client_counter_clone = self.async_active_client_counter.clone();

        let fut = async move {
            let mut lock = arc.lock().await;
            // check if wasm is uninitialised, i.e. loaded to  disk, in that case load it back to memory
            if let MaybeInst::UnsInst(context, snapshot) = lock.take_uns() {
                let permit = async_active_client_counter_clone.acquire_owned().await?;

                let module = unsafe { Module::deserialize_file(&environment.engine, &wasm_path)? };

                let mut store = Store::new(&environment.engine, context);
                let pre_instance = environment.linker.instantiate_pre(&mut store, &module)?;
                
                drop(module);

                let instance = pre_instance.instantiate(&mut store)?;
                let mem = instance.get_memory(&mut store, "memory").unwrap();

                let mut f = File::open(&swap_path).await?;
                let mem_size = mem.data_size(&mut store);

                if snapshot.memory_min > mem_size {
                    let memory_diff = (snapshot.memory_min - mem_size) as u64;

                    let mut n_pages = memory_diff / WASM_PAGE_SIZE;
                    if (memory_diff % WASM_PAGE_SIZE) > 0 {
                        n_pages += 1;
                    }

                    mem.grow(&mut store, n_pages)?;
                }

                let read = f.read_exact(mem.data_mut(&mut store)).await?;
                assert_eq!(read, snapshot.memory_min);

                for (name, global) in snapshot.globals.iter() {
                    instance
                        .get_global(&mut store, name)
                        .unwrap()
                        .set(&mut store, global.clone())?;
                }

                lock.set(MaybeInst::GotInst(
                    store,
                    permit,
                    instance,
                ));
            }

            let (store, instance) = match &mut *lock {
                MaybeInst::GotInst(store, _, instance) => (store, instance),
                _ => unreachable!(),
            };

            crate::abi::wakeup(store, instance, async_request_id, value, finished).await?;

            Ok(())
        }
        .boxed();

        self.set_wasm_work(fut, "wakeup");
        self.uninstantiating = false;

        Ok(())
    }

    // load wasm  back to memory, like wakeup but without needing  a request to be  finished
    pub(crate) fn load_to_mem(
        &mut self,
    )  {
        assert!(self.wasm_work.is_none());
        let arc = self.inner.clone();
        let environment = self.environment.clone();
        let swap_path = self.swap_path.clone();
        let wasm_path = self.wasm_path.clone();
        let async_active_client_counter_clone = self.async_active_client_counter.clone();

        let fut = async move {
            let mut lock = arc.lock().await;
            // check if wasm is uninitialised, i.e. loaded to  disk, in that case load it back to memory
            if let MaybeInst::UnsInst(context, snapshot) = lock.take_uns() {
                let permit = async_active_client_counter_clone.acquire_owned().await?;

                let module = unsafe { Module::deserialize_file(&environment.engine, &wasm_path)? };

                let mut store = Store::new(&environment.engine, context);
                let pre_instance = environment.linker.instantiate_pre(&mut store, &module)?;
                
                drop(module);

                let instance = pre_instance.instantiate(&mut store)?;
                let mem = instance.get_memory(&mut store, "memory").unwrap();

                let mut f = File::open(&swap_path).await?;
                let mem_size = mem.data_size(&mut store);

                if snapshot.memory_min > mem_size {
                    let memory_diff = (snapshot.memory_min - mem_size) as u64;

                    let mut n_pages = memory_diff / WASM_PAGE_SIZE;
                    if (memory_diff % WASM_PAGE_SIZE) > 0 {
                        n_pages += 1;
                    }

                    mem.grow(&mut store, n_pages)?;
                }

                let read = f.read_exact(mem.data_mut(&mut store)).await?;
                assert_eq!(read, snapshot.memory_min);

                for (name, global) in snapshot.globals.iter() {
                    instance
                        .get_global(&mut store, name)
                        .unwrap()
                        .set(&mut store, global.clone())?;
                }

                lock.set(MaybeInst::GotInst(
                    store,
                    permit,
                    instance,
                ));
            }

            let (store, instance) = match &mut *lock {
                MaybeInst::GotInst(store, _, instance) => (store, instance),
                _ => unreachable!(),
            };

            Ok(())
        }
        .boxed();

        self.set_wasm_work(fut, "load_to_mem");
        self.uninstantiating = false;

        
    }


    fn set_wasm_work(&mut self, fut: BoxFuture<'static, anyhow::Result<()>>, name: &'static str) {
        assert!(self.wasm_work.is_none());

        self.wasm_work = Some(Box::pin(
            fut.instrument(tracing::debug_span!("wasm_work", name = name)),
        ));
    }

    pub(crate) fn poll_unpin(&mut self, cx: &mut Context) -> anyhow::Result<Poll<()>> {
        if self.wasm_work.is_none() {
            return Ok(Poll::Ready(()));
        }

        let res = self.wasm_work.as_mut().unwrap().poll_unpin(cx);

        match res {
            Poll::Pending => Ok(Poll::Pending),
            Poll::Ready(Ok(())) => {
                self.wasm_work = None;
                Ok(Poll::Ready(()))
            }
            Poll::Ready(Err(e)) => Err(e),
        }
    }
}
