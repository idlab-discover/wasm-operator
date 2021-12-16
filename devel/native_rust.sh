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

NR_CONTROLLERS=100

echo "Build the controller"
CONTROLLER_NAMES=()

pushd controllers/ring-rust-controller
mkdir -p bin/
COMPILE_NONCE="REPLACE_MEREPLACE_ME" cargo build --release --features client
cp ./target/release/ring-pod-example ./bin/ring-rust-controller.REPLACE_ME

for (( i = 0; i < NR_CONTROLLERS; i++ )); do
    CONTROLLER_NAME="controller${i}"
    RANDOM_VALUE=$(echo $RANDOM | md5sum | head -c 20)
    sed -e "s|REPLACE_MEREPLACE_ME|$RANDOM_VALUE|" ./bin/ring-rust-controller.REPLACE_ME > ./bin/ring-rust-controller.$CONTROLLER_NAME
    chmod +x ./bin/ring-rust-controller.$CONTROLLER_NAME
    CONTROLLER_NAMES+=($CONTROLLER_NAME)
done
popd

kubectl apply -f ./tests/yaml/

echo "Build the docker image"
pushd tests/native_rust

for CONTROLLER_NAME in "${CONTROLLER_NAMES[@]}"; do
    rm -rf ./temp/ && mkdir -p ./temp/

    cp ../../controllers/ring-rust-controller/bin/ring-rust-controller.$CONTROLLER_NAME ./temp/ring-rust-controller

    if [[ "$(docker images -q "github.com/amurant/native_rust:$CONTROLLER_NAME" 2> /dev/null)" == "" ]]; then
        docker build -f Dockerfile -t "github.com/amurant/native_rust:$CONTROLLER_NAME" ./temp/
    fi
done

echo "Loading the docker images"
# for CONTROLLER_NAME in "${CONTROLLER_NAMES[@]}"; do
#     kind load docker-image --name "$KIND_CLUSTER_NAME" "github.com/amurant/native_rust:$CONTROLLER_NAME"
# done

mkdir -p ./temp/deploy/

generate_namespace_yaml_file $NR_CONTROLLERS "native-rust" > temp/deploy/01_namespaces.yaml
generate_pod_yaml_file $NR_CONTROLLERS "native-rust" "github.com/amurant/native_rust:" > temp/deploy/02_pod.yaml
cat << EOF > temp/deploy/03_resource.yaml
apiVersion: amurant.io/v1
kind: TestResource
metadata:
    name: run001
    namespace: native-rust0
spec:
    nonce: 0
EOF

kubectl apply -f ./temp/deploy/
popd
