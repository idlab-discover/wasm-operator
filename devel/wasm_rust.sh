#!/usr/bin/env bash
set -o errexit
set -o nounset
set -o pipefail

SCRIPT_ROOT=$(realpath $(dirname "${BASH_SOURCE}"))
source "${SCRIPT_ROOT}/lib.sh"
source "${SCRIPT_ROOT}/lib_functions.sh"

check_tool rust
check_tool docker
check_tool kind

cd "${SCRIPT_ROOT}/.."

NR_CONTROLLERS=10

# Build the WASM binary & parent controller
pushd pkg/controller
cargo build --release
popd

pushd controllers/ring-rust-controller
cargo wasi build --release --features client-wasi
popd

kubectl apply -f ./tests/yaml/

# Build the docker image
pushd tests/wasm_rust
rm -rf ./temp/ && mkdir -p ./temp/deploy/

cp ../../pkg/controller/target/release/controller ./temp/
cp ../../controllers/ring-rust-controller/target/wasm32-wasi/release/ring-pod-example.wasi.wasm ./temp/
generate_wasm_yaml_file $NR_CONTROLLERS "wasm-rust" > ./temp/wasm_config.yaml

local_tag="local"

docker build -f Dockerfile -t "github.com/amurant/wasm_rust:${local_tag}" ./temp/

kind load docker-image --name "${KIND_CLUSTER_NAME}" "github.com/amurant/wasm_rust:${local_tag}"

# Apply the yaml manifests
generate_namespace_yaml_file $NR_CONTROLLERS "wasm-rust" > temp/deploy/01_namespaces.yaml
generate_pod_yaml_file 1 "wasm-rust" "github.com/amurant/wasm_rust:${local_tag}" > temp/deploy/02_pod.yaml
cat << EOF > temp/deploy/03_resource.yaml
apiVersion: amurant.io/v1
kind: TestResource
metadata:
    name: run001
    namespace: wasm-rust0
spec:
    nonce: 0
EOF

# kubectl delete -f ./temp/deploy/
kubectl apply -f ./temp/deploy/
popd
