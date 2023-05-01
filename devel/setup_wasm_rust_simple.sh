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


#export RUST_BACKTRACE=1
export COMPILE_WITH_UNINSTANTIATE="TRUE"
#export COMPILE_WITH_UNINSTANTIATE="FALSE"
export RUSTFLAGS="-g"
#export OPENSSL_DIR="/usr"
#echo $OPENSSL_DIR


# Build the WASM binary & parent controller
echo ">> Build the WASM binary & parent controller"
pushd pkg/controller
   cross build --release --target=x86_64-unknown-linux-musl
popd

CONTROLLER_NAMES=()





pushd controllers/simple-rust-controller
    mkdir -p bin_wasm/

    # Compile the ring controller once with "REPLACE_MEREPLACE_ME" as nonce
    echo ">> Build the controller wasm rust"
    cargo wasi build --release --features client-wasi
    echo ">> optimise wasm"
    # why use wasm opt when it is default already  optimised using  cargo wasi
    #wasm-opt --version
    wasm-opt -Os ./target/wasm32-wasi/release/simple-pod-example.wasi.wasm -o ./target/wasm32-wasi/release/simple-pod-example.wasi.wasm1 
    #wasm-opt -Os ./target/wasm32-wasi/release/ring-pod-example.wasi.wasm -o ./target/wasm32-wasi/release/ring-pod-example.wasi.opt.wasm 
    #cp ./target/wasm32-wasi/release/ring-pod-example.wasi.opt.wasm ./bin_wasm/ring-rust-example.wasi.REPLACE_ME.wasm
    cp ./target/wasm32-wasi/release/simple-pod-example.wasi.wasm1 ./bin_wasm/simple-pod-example.wasm

    # Create unique versions of the controller by replacing the "REPLACE_MEREPLACE_ME" nonce value
    #echo ">> Create variants"
    #for (( i = 0; i < NR_CONTROLLERS; i++ )); do
    #    CONTROLLER_NAME="controller${i}"
     #   NONCE_VALUE=$(echo $CONTROLLER_NAME | md5sum | head -c 20)
     #   sed -e "s|REPLACE_MEREPLACE_ME|$NONCE_VALUE|" ./bin_wasm/ring-rust-example.wasi.REPLACE_ME.wasm > ./bin_wasm/ring-rust-example.wasi.$CONTROLLER_NAME.wasm
     #   CONTROLLER_NAMES+=($CONTROLLER_NAME)
    #done
popd


pushd tests/wasm_rust_simple
    rm -rf ./temp/ && mkdir -p ./temp/deploy/

    cp ../../pkg/controller/target/x86_64-unknown-linux-musl/release/controller ./temp/
    #cp ../../pkg/controller/target/x86_64-unknown-linux-musl/debug/controller ./temp/
    cp ../../controllers/simple-rust-controller/bin_wasm/*.wasm ./temp/
    
    
    
    generate_wasm_yaml_file_simple > ./temp/wasm_config.yaml

    # Build the docker image
    echo ">> Build the docker image"
    local_tag="controller0"
    docker build -f Dockerfile -t "github.com/amurant/wasm_rust_simple:${local_tag}" ./temp/
    
    # Load the docker images
    echo ">> Load the docker images"
    kind load docker-image --name "${KIND_CLUSTER_NAME}" "github.com/amurant/wasm_rust_simple:${local_tag}"

    # Generate the yaml files
    echo ">> Generate the yaml files"
    generate_namespace_yaml_file_simple $NR_CONTROLLERS "wasm-rust-simple" > temp/deploy/01_namespaces.yaml
    
    ## get server ip of prediction server
    SERVER="http://"
    SERVER+=$(kubectl get service/flask-service -o jsonpath='{.spec.clusterIP}')
    SERVER+=":5000/"

    generate_pod_yaml_file_simple_rust $SERVER > temp/deploy/02_pod.yaml
popd



echo ">> Deploy manifests"

# Setup CRDs, Namespaces, RBAC rules
kubectl apply -f ./tests/yaml/metricsServer.yaml
kubectl apply -f ./tests/yaml/crd.yaml
kubectl apply -f ./tests/yaml/namespace.yaml
kubectl apply -f ./tests/yaml/rbac.yaml


echo ">> Deploy first"

# Setup CRDs, Namespaces, RBAC rules
kubectl apply -f ./tests/wasm_rust_simple/temp/deploy/

echo ">> Deploy second"

# Wait for pods to become ready
kubectl -n wasm-rust-simple wait --for=condition=ready pod --all --timeout=3000s
