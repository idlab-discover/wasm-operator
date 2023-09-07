#!/usr/bin/env bash
set -o errexit
set -o nounset
set -o pipefail


SCRIPT_ROOT=$(realpath $(dirname "${BASH_SOURCE}"))
source "${SCRIPT_ROOT}/tool.sh"
source "${SCRIPT_ROOT}/lib_functions.sh"


check_tool kind
check_tool docker
check_tool pip3

local_tag="webserver"

pushd prediction/webserver
    mkdir -p temp/


    if [ -f ".venv/bin/activate" ]; then
    echo "new requirements generated"
        source .venv/bin/activate && pip3 freeze > requirements.txt 
    fi
    echo ">> Build the docker image flask server"

    
    docker build -f dockerFile -t "docker.io/kevinvl123/prediction_webserver:${local_tag}" .

    echo ">> Load the docker images"

    kind load docker-image --name "${KIND_CLUSTER_NAME}" "docker.io/kevinvl123/prediction_webserver:${local_tag}"

popd

kubectl apply -f ./tests/yaml/deploymentFlask.yaml

