# Extending Kubernetes API in-process

Project structure:

* `ext-simple-pod`: Wasm module that implements a simple controller to spawn pods
* `ext-memcached`: Wasm module that implements the operator-sdk [Memcached sample](https://sdk.operatorframework.io/docs/golang/quickstart/)
* `kube-rs`: Hacked https://github.com/clux/kube-rs to run inside the module
* `kube-rs-host`: Hacked https://github.com/clux/kube-rs to use inside the host
* `rust-host`: The host running wasm modules

## Build

To build the memcached example controller:

```shell script
cd ext-memcached
cargo build --target wasm32-wasi --release
```

Assuming you have a Kubernetes cluster up and running and you have an admin access to it configured in your local environment, deploy the CRD:

```shell script
kubectl apply -f ext-memcached/crd.yaml
```

Now, copy in a directory (eg `rust-host/compiled_mods`) the compiled module and the manifest the host needs to identify the abi to use:

```shell script
mkdir rust-host/compiled_mods
cp ext-memcached/memcached.yaml rust-host/compiled_mods
cp ext-memcached/wasm32-wasi/release/memcached.wasm rust-host/compiled_mods
```

To compile and run the host:

```shell script
cd rust-host
RUST_LOG=rust_host=debug,cranelift=warn,kube=debug cargo +nightly run compiled_mods
```

Now you can create the `Memcached` CR with:

```shell script
kubectl apply -f ext-memcached/cr.yaml
```
