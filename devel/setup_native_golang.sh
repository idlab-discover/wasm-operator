#!/usr/bin/env bash
set -o errexit
set -o nounset
set -o pipefail

SCRIPT_ROOT=$(realpath $(dirname "${BASH_SOURCE}"))
source "${SCRIPT_ROOT}/tool.sh"
source "${SCRIPT_ROOT}/lib_functions.sh"

check_tool go
check_tool docker
check_tool kind
check_tool kubectl

cd "${SCRIPT_ROOT}/.."

NR_CONTROLLERS=$1

CONTROLLER_NAMES=()

pushd controllers/ring-go-controller
    mkdir -p bin/

    # Compile the ring controller once with "REPLACE_MEREPLACE_ME" as nonce
    echo ">> Build the controller native go"
    GOGC=100 go build -ldflags "-s -w -X main.CompileNonce=REPLACE_MEREPLACE_ME" -o ./bin/ring-go-controller.REPLACE_ME

    # Create unique versions of the controller by replacing the "REPLACE_MEREPLACE_ME" nonce value
    echo ">> Create variants"
    for (( i = 0; i < NR_CONTROLLERS; i++ )); do
        CONTROLLER_NAME="controller${i}"
        NONCE_VALUE=$(echo $CONTROLLER_NAME | md5sum | head -c 20)
        sed -e "s|REPLACE_MEREPLACE_ME|$NONCE_VALUE|" ./bin/ring-go-controller.REPLACE_ME > ./bin/ring-go-controller.$CONTROLLER_NAME
        chmod +x ./bin/ring-go-controller.$CONTROLLER_NAME
        CONTROLLER_NAMES+=($CONTROLLER_NAME)
    done
popd


pushd tests/native_golang
    # Build the docker images
    echo ">> Build the docker image"
    for CONTROLLER_NAME in "${CONTROLLER_NAMES[@]}"; do
        rm -rf ./temp/ && mkdir -p ./temp/

        cp ../../controllers/ring-go-controller/bin/ring-go-controller.$CONTROLLER_NAME ./temp/ring-go-controller

        # if [[ "$(docker images -q "github.com/amurant/native_golang:$CONTROLLER_NAME" 2> /dev/null)" == "" ]]; then
        docker build -f Dockerfile -t "github.com/amurant/native_golang:$CONTROLLER_NAME" ./temp/
        # fi
    done

    # Load the docker images
    echo ">> Load the docker images"
    for CONTROLLER_NAME in "${CONTROLLER_NAMES[@]}"; do
        kind load docker-image --name "$KIND_CLUSTER_NAME" "github.com/amurant/native_golang:$CONTROLLER_NAME"
    done

    # Generate the yaml files
    echo ">> Generate the yaml files"
    mkdir -p ./temp/deploy/
    generate_namespace_yaml_file $NR_CONTROLLERS "native-golang" > temp/deploy/01_namespaces.yaml
    generate_pod_yaml_file $NR_CONTROLLERS "native-golang" "github.com/amurant/native_golang:" > temp/deploy/02_pod.yaml
popd

echo ">> Deploy manifests"

# Setup CRDs, Namespaces, RBAC rules
kubectl apply -f ./tests/yaml/

# Setup CRDs, Namespaces, RBAC rules
kubectl apply -f ./tests/native_golang/temp/deploy/

# Wait for pods to become ready
kubectl -n native-golang wait --for=condition=ready pod --all --timeout=3000s
