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

cd "${SCRIPT_ROOT}/.."

# Build the WASM binary & parent controller
pushd controllers/ring-go-controller
go build -ldflags "-s -w" -o ./bin/ring-go-controller
popd

kubectl apply -f ./tests/yaml/

# Build the docker image
pushd tests/native_golang
rm -rf ./temp/ && mkdir -p ./temp/deploy/

cp ../../controllers/ring-go-controller/bin/ring-go-controller ./temp/

local_tag="local"

docker build -f Dockerfile -t "github.com/amurant/native_golang:${local_tag}" ./temp/

kind load docker-image --name "${KIND_CLUSTER_NAME}" "github.com/amurant/native_golang:${local_tag}"

# Apply the yaml manifests
NR_CONTROLLERS=10

generate_namespace_yaml_file $NR_CONTROLLERS "native-golang" > temp/deploy/01_namespaces.yaml
generate_pod_yaml_file $NR_CONTROLLERS "native-golang" "github.com/amurant/native_golang:${local_tag}" > temp/deploy/02_pod.yaml
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
