mod abi;

use wasmer_runtime::{error, func, imports, Func, compile};
use std::cell::RefCell;

// Make sure that the compiled wasm-sample-app is accessible at this path.
static WASM: &'static [u8] =
    include_bytes!("../http.wasm");

macro_rules! execution_time {
    ($code:block) => {
        {
            let start = std::time::Instant::now();
            let res = $code;
            let execution_time = start.elapsed();
            (res, execution_time)
        }
    };
}

fn main() -> error::Result<()> {
    let client = reqwest::Client::builder().build().unwrap();
    let rt = RefCell::new(tokio::runtime::Runtime::new().unwrap());

    let (module, duration) = execution_time!({
        compile(WASM).expect("wasm compilation")
    });
    println!("Compilation time duration: {} ms", duration.as_millis());

    // get the version of the WASI module in a non-strict way, meaning we're
    // allowed to have extra imports
    let wasi_version = wasmer_wasi::get_wasi_version(&module, false)
        .expect("WASI version detected from Wasm module");

    // WASI imports
    let mut base_imports =
        wasmer_wasi::generate_import_object_for_version(wasi_version, vec![], vec![], vec![], vec![]);

    // add execute_request to the ABI
    let custom_import = imports! {
        "http-proxy-abi" => {
            // the func! macro autodetects the signature
            "request" => func!(abi::request_fn(rt, client)),
        },
    };
    base_imports.extend(custom_import);

    // Compile our webassembly into an `Instance`.
    let instance = module.instantiate(&base_imports)
        .expect("Failed to instantiate wasm module");

    // Call our start function!
    let run_fn: Func<(), ()> = instance.exports.get("run").unwrap();
    run_fn.call()
        .expect("Something went wrong while invoking run");

    Ok(())
}