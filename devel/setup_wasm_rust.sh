#!/usr/bin/env bash
set -o errexit
set -o nounset
set -o pipefail

SCRIPT_ROOT=$(realpath $(dirname "${BASH_SOURCE}"))
source "${SCRIPT_ROOT}/tool.sh"
source "${SCRIPT_ROOT}/lib_functions.sh"

check_tool rust
check_tool docker
check_tool kind
check_tool kubectl
check_tool wasm-opt

cd "${SCRIPT_ROOT}/.."

NR_CONTROLLERS=$1

# Build the WASM binary & parent controller
echo ">> Build the WASM binary & parent controller"
pushd pkg/controller
   cross build --release --target=x86_64-unknown-linux-musl
popd

CONTROLLER_NAMES=()


export RUST_BACKTRACE=1

pushd controllers/ring-rust-controller
    mkdir -p bin_wasm/

    # Compile the ring controller once with "REPLACE_MEREPLACE_ME" as nonce
    echo ">> Build the controller wasm rust"
    COMPILE_NONCE="REPLACE_MEREPLACE_ME" cargo wasi build --release --features client-wasi
    echo ">> optimise wasm"
    # why use wasm opt when it is default already  optimised using  cargo wasi
    wasm-opt --version
    #wasm-opt -Os ./target/wasm32-wasi/release/ring-pod-example.wasi.wasm -o ./target/wasm32-wasi/release/ring-pod-example.wasi.opt.wasm 
    #cp ./target/wasm32-wasi/release/ring-pod-example.wasi.opt.wasm ./bin_wasm/ring-rust-example.wasi.REPLACE_ME.wasm
    cp ./target/wasm32-wasi/release/ring-pod-example.wasi.wasm ./bin_wasm/ring-rust-example.wasi.REPLACE_ME.wasm

    # Create unique versions of the controller by replacing the "REPLACE_MEREPLACE_ME" nonce value
    echo ">> Create variants"
    for (( i = 0; i < NR_CONTROLLERS; i++ )); do
        CONTROLLER_NAME="controller${i}"
        NONCE_VALUE=$(echo $CONTROLLER_NAME | md5sum | head -c 20)
        sed -e "s|REPLACE_MEREPLACE_ME|$NONCE_VALUE|" ./bin_wasm/ring-rust-example.wasi.REPLACE_ME.wasm > ./bin_wasm/ring-rust-example.wasi.$CONTROLLER_NAME.wasm
        CONTROLLER_NAMES+=($CONTROLLER_NAME)
    done
popd


pushd tests/wasm_rust
    rm -rf ./temp/ && mkdir -p ./temp/deploy/

    cp ../../pkg/controller/target/x86_64-unknown-linux-musl/release/controller ./temp/
    #cp ../../pkg/controller/target/x86_64-unknown-linux-musl/debug/controller ./temp/
    cp ../../controllers/ring-rust-controller/bin_wasm/*.wasm ./temp/
    generate_wasm_yaml_file $NR_CONTROLLERS "wasm-rust" > ./temp/wasm_config.yaml

    # Build the docker image
    echo ">> Build the docker image"
    local_tag="controller0"
    docker build -f Dockerfile -t "github.com/amurant/wasm_rust:${local_tag}" ./temp/
    
    # Load the docker images
    echo ">> Load the docker images"
    kind load docker-image --name "${KIND_CLUSTER_NAME}" "github.com/amurant/wasm_rust:${local_tag}"

    # Generate the yaml files
    echo ">> Generate the yaml files"
    generate_namespace_yaml_file $NR_CONTROLLERS "wasm-rust" > temp/deploy/01_namespaces.yaml
    generate_pod_yaml_file 1 "wasm-rust" "github.com/amurant/wasm_rust:" > temp/deploy/02_pod.yaml
popd

echo ">> Deploy manifests"

# Setup CRDs, Namespaces, RBAC rules
kubectl apply -f ./tests/yaml/

# Setup CRDs, Namespaces, RBAC rules
kubectl apply -f ./tests/wasm_rust/temp/deploy/

# Wait for pods to become ready
kubectl -n wasm-rust wait --for=condition=ready pod --all --timeout=3000s
