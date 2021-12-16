#!/usr/bin/env bash
set -o errexit
set -o nounset
set -o pipefail

SCRIPT_ROOT=$(realpath $(dirname "${BASH_SOURCE}"))
source "${SCRIPT_ROOT}/lib.sh"
source "${SCRIPT_ROOT}/lib_functions.sh"

check_tool go
check_tool docker
check_tool kind
check_tool kubectl

cd "${SCRIPT_ROOT}/.."

NR_CONTROLLERS=100

echo "Build the controller"
CONTROLLER_NAMES=()

pushd controllers/ring-go-controller
go build -ldflags "-s -w -X main.CompileNonce=REPLACE_MEREPLACE_ME" -o ./bin/ring-go-controller.REPLACE_ME

for (( i = 0; i < NR_CONTROLLERS; i++ )); do
    CONTROLLER_NAME="controller${i}"
    RANDOM_VALUE=$(echo $RANDOM | md5sum | head -c 20)
    sed -e "s|REPLACE_MEREPLACE_ME|$RANDOM_VALUE|" ./bin/ring-go-controller.REPLACE_ME > ./bin/ring-go-controller.$CONTROLLER_NAME
    chmod +x ./bin/ring-go-controller.$CONTROLLER_NAME
    CONTROLLER_NAMES+=($CONTROLLER_NAME)
done
popd

kubectl apply -f ./tests/yaml/

echo "Build the docker image"
pushd tests/native_golang

for CONTROLLER_NAME in "${CONTROLLER_NAMES[@]}"; do
    rm -rf ./temp/ && mkdir -p ./temp/

    cp ../../controllers/ring-go-controller/bin/ring-go-controller.$CONTROLLER_NAME ./temp/ring-go-controller

    if [[ "$(docker images -q "github.com/amurant/native_golang:$CONTROLLER_NAME" 2> /dev/null)" == "" ]]; then
        docker build -f Dockerfile -t "github.com/amurant/native_golang:$CONTROLLER_NAME" ./temp/
    fi
done

echo "Loading the docker images"
# for CONTROLLER_NAME in "${CONTROLLER_NAMES[@]}"; do
#     kind load docker-image --name "$KIND_CLUSTER_NAME" "github.com/amurant/native_golang:$CONTROLLER_NAME"
# done

echo "Applying the manifests"
mkdir -p ./temp/deploy/

# Apply the yaml manifests
generate_namespace_yaml_file $NR_CONTROLLERS "native-golang" > temp/deploy/01_namespaces.yaml
generate_pod_yaml_file $NR_CONTROLLERS "native-golang" "github.com/amurant/native_golang:" > temp/deploy/02_pod.yaml
cat << EOF > temp/deploy/03_resource.yaml
apiVersion: amurant.io/v1
kind: TestResource
metadata:
    name: run001
    namespace: native-golang0
spec:
    nonce: 0
EOF

kubectl apply -f ./temp/deploy/
popd
