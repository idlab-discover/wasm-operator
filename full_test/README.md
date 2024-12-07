## Dependencies run_wasm.sh
> These are managed by [`devel/tool.sh`](../devel/tool.sh) and often installed when not present

- [Kind](https://kind.sigs.k8s.io/)
- [Kubectl](https://kubernetes.io/docs/reference/kubectl/)
- [Docker](https://www.docker.com/)
- [Rust](https://www.rust-lang.org/)
- [Go](shttps://go.dev/)
- [wasm-opt](https://github.com/WebAssembly/binaryen)
- [Cross](https://crates.io/crates/cross) (never checked, but required to run operator)

Tools mentioned in [`devel/tool.sh`](../devel/tool.sh), but not used:
- [sccache](https://github.com/mozilla/sccache) (installed when executing tool.sh though)
- [Python3 + pip3](https://www.python.org/)
- [Helm](https://helm.sh/)
- [kube-apiserver + etcd](https://github.com/kubernetes-sigs/kubebuilder)

## All steps run_wasm.sh
### 1. Set environment variables
Original files: [`full_test/run_wasm.sh`](run_wasm.sh) + [`devel/tool.sh`](../devel/tool.sh)

| Name | Value | Command |
| ---- | ----- | ------- |
| COMPILE_WITH_UNINSTANTIATE | "TRUE" | `export COMPILE_WITH_UNINSTANTIATE="TRUE"` |
| HEAP_MEM_SIZE | 90000000 | `export HEAP_MEM_SIZE=90000000` |
| RUST_BACKTRACE | 1 | `export RUST_BACKTRACE=1` |
| nrworkers | 1 | `export nrworkers=1` |
| KIND_CLUSTER_NAME | "kind" | `export KIND_CLUSTER_NAME="kind"`|


### 2. Create kind cluster
> Original file: [`devel/create_cluster.sh`](../devel/create_cluster.sh)

```sh
export KIND_CLUSTER_NAME="kind"
kind delete clusters "${KIND_CLUSTER_NAME}"
kind create cluster \
  --name "${KIND_CLUSTER_NAME}" \
  --config "./devel/kind-config.yaml"
# sed -i 's|127.0.0.1|localhost|g' ~/.kube/config
```

### 3. Run Flask server for predictions
> Original file: [`devel/setup_flask_server.sh`](../devel/setup_flask_server.sh)

The Flask server is in order to enable prediction and can be found in the `./prediction/webserver` directory.

```sh
docker build -f ./prediction/webserver/dockerFile -t prediction_webserver:webserver ./prediction/webserver
kind load docker-image --name "${KIND_CLUSTER_NAME}" "prediction_webserver:webserver"
```

#### Optional: test Flask deployment
```sh
kubectl apply -f ./tests/yaml/deploymentFlask.yaml
kubectl port-forward service/flask-service 5000:5000
```

### 4. Build the WASM binary & parent operator
> Original file: [`devel/setup_wasm_rust_simple.sh`](../devel/setup_wasm_rust_simple.sh) / [`devel/setup_wasm_rust.sh`](../devel/setup_wasm_rust.sh)

```sh
cd ./pkg/controller
cross build --release --target=x86_64-unknown-linux-musl
```

### 5. Run a child operator
Currently supported:
- [controllers/simple-rust-controller](../controllers/simple-rust-controller/) via [`setup_wasm_rust_simple.sh`](../devel/setup_wasm_rust_simple.sh)
- [controllers/ring-rust-controller](../controllers/ring-rust-controller/) via [`setup_wasm_rust.sh`](../devel/setup_wasm_rust.sh)

The current setup is not developer friendly. It requires applying a lot of files and generating yaml's.
TODO: this process should be improved.
The part below is the only piece that seems to be "extractable", but we currently recommend to use the [`HEAP_MEM_SIZE=90000000 setup_wasm_rust_simple.sh <NR_CONTROLLERS>`](../devel/setup_wasm_rust_simple.sh) or [`HEAP_MEM_SIZE=90000000 setup_wasm_rust.sh <NR_CONTROLLERS>`](../devel/setup_wasm_rust.sh) script until a better approach is implemented.

```sh
# Set NONCE value allowing us to generate unique versions of the operator
export COMPILE_NONCE="REPLACE_MEREPLACE_ME"
# Build the WASM for the operator
cargo wasi build --release --features client-wasi
# Optimize the WASM
wasm-opt -Os ./target/wasm32-wasi/release/<NAME>.wasi.wasm -o ./target/wasm32-wasi/release/<NAME>-optimized.wasi.wasm
```

### 6. Run tests (optional)
> Original file: [`devel/test.sh`](../devel/test.sh) (with args: `<NR_WORKERS> <NR_CYCLES> <TYPE> <OUT_FILE>`)  
> With TYPE = "wasm-rust", "native-rust" or "native-golang"

```sh
export run="run0"
export namespace_prefix="wasm-rust"
export nr_controllers=1
export nonce=0

kubectl delete TestResource --all-namespaces --all

# Create resource for testing the operator
kubectl apply -f - <<EOF
apiVersion: amurant.io/v1
kind: TestResource
metadata:
    name: ${run}
    namespace: ${namespace_prefix}
spec:
    nonce: ${nonce}
EOF

echo "${run} - ${nonce}: waiting for ${namespace_prefix}"
while true
do
    kubectl wait -n ${namespace_prefix}${last_index} TestResource ${run} --for=jsonpath='{.spec.nonce}'=$nonce || {
        echo "${run} - ${nonce}: FAILED waiting for ${namespace_prefix}${last_index}; retrying"

        sleep 5
        continue
    }

    break
done
echo "${run} - ${nonce}: FINISHED waiting for ${namespace_prefix}"

kubectl delete TestResource --all-namespaces --all
```

> [!NOTE]
> The original script does this for multiple cycles (using a for loop)


### 7. Profile solution (optional)
> Original file: [`profile/profile.sh`](../profile/profile.sh) (with args: `wasm ./test_results_run$run/out_wasm_${nrworkers}_uninst.csv &`)

```sh
sudo "${SCRIPT_ROOT}/profile.py <TYPE> <OUTPUT_FILE>"
```

With type = "rust", "comb", "golang" or "wasm"
