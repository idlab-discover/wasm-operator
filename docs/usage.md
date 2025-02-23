# Using the WASM-operator prototype

This guide expects that the user has followed the setup guide.
Some assumptions are made about which files would have already been compiled.
If you receive an error on missing files, please refer back to the setup guide.

## What operators are supported

We currently have only tested the operators present in the [controllers](../controllers/) directory.
More specifically, this is about:

- [controllers/simple-rust-controller](../controllers/simple-rust-controller/)
- [controllers/ring-rust-controller](../controllers/ring-rust-controller/)
- [controllers/comb-rust-controller](../controllers/comb-rust-controller/)

> [!NOTE]
> The author of this document has not been able to verify the use of the ring and comb operators. This collection of supported operators is based on documentation from the previous maintainers.
> TODO: please verify each of these operators and check if the instructions below can be made less specific to the simple-rust-controller

## Deploying child operators

As stated in the [setup guide](./setup.md), the parent operator currently does not support loading child operators at runtime and requires them to instead be loaded inside the Docker image.

This is all configurable using the `wasm_config.yaml` file,
which has the following structure

```yaml
name: <NAME-CHILD-OPERATOR>
wasm: <REL_PATH_TO_WASM_FILE>
env:
  - name: <ENV_NAME>
    value: <ENV_VALUE>
```

We provide an example configuration in [tests/wasm_rust_simple/wasm_config.yaml](../tests/wasm_rust_simple/wasm_config.yaml)

### Compiling child operators

```sh
export NAME_OPERATOR="simple-pod-example"
# Build the WASM for the child operator
cargo component build --release --features client-wasi
# Optimize the WASM
wasm-opt -Os ./target/wasm32-wasip1/release/${NAME_OPERATOR}.wasm -o ./target/wasm32-wasip1/release/${NAME_OPERATOR}-optimized.wasm
```

### Building the complete Docker image and loading into Kind

The Dockerfiles included in either [tests/wasm_rust](../tests/wasm_rust/Dockerfile) or [tests/wasm_rust_simple](../tests/wasm_rust_simple/Dockerfile) both make the same assumptions: their environment contains the following files:

- The controller binary
- Collection of .wasm files which contain the child operators
- A wasm_config.yaml giving information on the child operators

Below is an example on how to setup the **simple-rust-controller** example.

```sh
mkdir temp
cp ./pkg/controller/target/x86_64-unknown-linux-musl/release/controller ./temp
cp ./tests/wasm_rust_simple/wasm_config.yaml ./temp
cp ./controllers/target/wasm32-wasip1/release/${NAME_OPERATOR}-optimized.wasi.wasm ./temp
docker build -f ./tests/wasm_rust_simple/Dockerfile -t wasm_rust_simple:controller ./temp
```

The last step is then to load the Docker image into Kind:

```sh
kind load docker-image --name wasm-operator wasm_rust_simple:controller
```

### Creating the Kubernetes resources

First we setup the resources required for the parent operator.
NOTE: this currently contains quite a bit of "bloat" which comes from the research nature of this project.

```sh
kubectl apply -f ./tests/yaml/metricsServer.yaml
kubectl apply -f ./tests/yaml/crd.yaml
kubectl apply -f ./tests/yaml/namespace.yaml
kubectl apply -f ./tests/yaml/rbac.yaml
```

These are the resources specific to the **simple-rust-controller**.
Before use, please update the IP in pod.yaml to the prediction server IP

```sh
SERVER="http://"
SERVER+=$(kubectl get service/flask-service -o jsonpath='{.spec.clusterIP}')
SERVER+=":5000/"
echo ${SERVER}
```

```sh
kubectl apply -f ./tests/wasm_rust_simple/manifests/namespace.yaml
kubectl apply -f ./tests/wasm_rust_simple/manifests/pod.yaml
```

You can check that everything went smoothly by ordering a wait on the pods:

```sh
kubectl -n wasm-rust-simple wait --for=condition=ready pod --all --timeout=3000s
```
