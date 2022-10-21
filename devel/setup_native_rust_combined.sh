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

cd "${SCRIPT_ROOT}/.."

NR_CONTROLLERS=$1


CONTROLLER_NAMES=()


echo ">> Build the controller native rust combined"
COMPILE_NONCE="REPLACE_MEREPLACE_ME" cross build --manifest-path controllers/comb-rust-controller/Cargo.toml --release --features client --target=x86_64-unknown-linux-musl

pushd controllers/comb-rust-controller
    mkdir -p bin/

    # Compile the comb controller once with "REPLACE_MEREPLACE_ME" as nonce
    echo ">> move the controller native rust combined"
    # COMPILE_NONCE="REPLACE_MEREPLACE_ME" cargo build --release --features client --target=x86_64-unknown-linux-gnu
    cp ./target/x86_64-unknown-linux-musl/release/ring-pod-example ./bin/comb-rust-controller.REPLACE_ME
    #TODO why is there not a for loop here ? only 1 container will be added...
    # Create unique versions of the controller by replacing the "REPLACE_MEREPLACE_ME" nonce value
    echo ">> Create variants"
    CONTROLLER_NAME="controller"
    NONCE_VALUE=$(echo $CONTROLLER_NAME | md5sum | head -c 20)
    sed -e "s|REPLACE_MEREPLACE_ME|$NONCE_VALUE|" ./bin/comb-rust-controller.REPLACE_ME > ./bin/comb-rust-controller.$CONTROLLER_NAME
    chmod +x ./bin/comb-rust-controller.$CONTROLLER_NAME
    CONTROLLER_NAMES+=($CONTROLLER_NAME)
popd


pushd tests/native_rust_comb
    # Build the docker images
    echo ">> Build the docker image"
    for CONTROLLER_NAME in "${CONTROLLER_NAMES[@]}"; do
        rm -rf ./temp/ && mkdir -p ./temp/

        cp ../../controllers/comb-rust-controller/bin/comb-rust-controller.$CONTROLLER_NAME ./temp/comb-rust-controller

        #if [[ "$(docker images -q "github.com/amurant/native_rust_comb:$CONTROLLER_NAME" 2> /dev/null)" == "" ]]; then
        docker build -f Dockerfile -t "github.com/amurant/native_rust_comb:$CONTROLLER_NAME" ./temp/
        #fi
    done

    # Load the docker images
    echo ">> Load the docker images"
    for CONTROLLER_NAME in "${CONTROLLER_NAMES[@]}"; do
        kind load docker-image --name "$KIND_CLUSTER_NAME" "github.com/amurant/native_rust_comb:$CONTROLLER_NAME"
    done

    # Generate the yaml files
    echo ">> Generate the yaml files"
    mkdir -p ./temp/deploy/
    generate_namespace_yaml_file $NR_CONTROLLERS "native-rust-comb" > temp/deploy/01_namespaces.yaml
    generate_pod_yaml_file_combined $NR_CONTROLLERS "native-rust-comb" "github.com/amurant/native_rust_comb:" > temp/deploy/02_pod.yaml
popd

echo ">> Deploy manifests"

# Setup CRDs, Namespaces, RBAC rules
kubectl apply -f ./tests/yaml/
echo ">> kubectl apply -f ./tests/yaml/"
# Setup CRDs, Namespaces, RBAC rules
kubectl apply -f ./tests/native_rust_comb/temp/deploy/
echo ">> kubectl apply -f ./tests/native_rust_comb/temp/deploy/"
# Wait for pods to become ready
kubectl -n native-rust-comb wait --for=condition=ready pod --all --timeout=3000s
echo ">> setup  native  rust combined  done"