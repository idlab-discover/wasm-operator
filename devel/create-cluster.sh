#!/usr/bin/env bash
set -o errexit
set -o nounset
set -o pipefail

SCRIPT_ROOT=$(realpath $(dirname "${BASH_SOURCE}"))
source "${SCRIPT_ROOT}/lib.sh"

check_tool kind

kind delete clusters "${KIND_CLUSTER_NAME}"

kind create cluster \
  --name "${KIND_CLUSTER_NAME}"

## networking bug workaround
container_id=$(docker ps -q -f name="${KIND_CLUSTER_NAME}-control-plane")
intern_ip=$(docker exec $container_id /bin/sh -c "/sbin/ip -o -4 addr list eth0 | awk '{print \$4}' | cut -d/ -f1")
sed -i "s|server: https://127.0.0.1:.*|server: https://$intern_ip:6443|g" ~/.kube/config
