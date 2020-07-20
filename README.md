# Extending Kubernetes API in-process

* `ext`: WASM module using the http proxy
* `rust-host`: The host running wasm module

## Build

To build the sample:

```shell script
cd ext
cargo build --target wasm32-wasi --release
cd ..
cp ext/target/wasm32-wasi/release/http.wasm rust-host
cd rust-host
cargo +nightly run
```

