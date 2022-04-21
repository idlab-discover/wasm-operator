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
check_tool heaptrack

cd "${SCRIPT_ROOT}/.."

NR_CONTROLLERS=$1

export COMPILE_WITH_UNINSTANTIATE="FALSE"

# Build the WASM binary & parent controller
echo ">> Build the WASM binary & parent controller"
pushd pkg/controller
    cargo build --release # --target=x86_64-unknown-linux-musl
popd

CONTROLLER_NAMES=()

pushd controllers/ring-rust-controller
    mkdir -p bin_wasm/

    # Compile the ring controller once with "REPLACE_MEREPLACE_ME" as nonce
    echo ">> Build the controller"
    COMPILE_NONCE="REPLACE_MEREPLACE_ME" cargo wasi build --release --features client-wasi
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

    # cp ../../pkg/controller/target/x86_64-unknown-linux-musl/release/controller ./temp/
    cp ../../pkg/controller/target/release/controller ./temp/
    cp ../../controllers/ring-rust-controller/bin_wasm/*.wasm ./temp/
    generate_wasm_yaml_file $NR_CONTROLLERS "wasm-rust" > ./temp/wasm_config.yaml
    
    # Generate the yaml files
    echo ">> Generate the yaml files"
    generate_namespace_yaml_file $NR_CONTROLLERS "wasm-rust" > temp/deploy/01_namespaces.yaml
popd

echo ">> Deploy manifests"

# Setup CRDs, Namespaces, RBAC rules
kubectl apply -f ./tests/yaml/

# Setup CRDs, Namespaces, RBAC rules
kubectl apply -f ./tests/wasm_rust/temp/deploy/

pushd tests/wasm_rust/temp
    ./controller .
    # sudo env "PATH=$PATH" heaptrack ./controller .
    # sudo valgrind --tool=memcheck ./controller .
    # sudo perf mem record -a -g -F 999 -o ../../../perf.data.raw ./controller .
popd
# -o ~/perf.data.raw
# perf report -i perf.data
