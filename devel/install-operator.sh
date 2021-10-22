#!/usr/bin/env bash

SCRIPT_ROOT=$(dirname "${BASH_SOURCE}")
source "${SCRIPT_ROOT}/lib.sh"

cd "${SCRIPT_ROOT}/../"

check_tool docker
check_tool kind
check_tool helm

local_tag="local"

docker build -t "github.com/inteon/master-tim-2021:${local_tag}" .

kind load docker-image --name "${KIND_CLUSTER_NAME}" "github.com/inteon/master-tim-2021:${local_tag}"

helm upgrade master-tim "${SCRIPT_ROOT}/../deploy/chart/" \
	--install \
	--namespace controller-namespace \
	--create-namespace \
	--set image="github.com/inteon/master-tim-2021:${local_tag}"

