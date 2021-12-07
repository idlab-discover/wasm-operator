#!/usr/bin/env bash
set -o errexit
set -o nounset
set -o pipefail

SCRIPT_ROOT=$(realpath $(dirname "${BASH_SOURCE}"))
source "${SCRIPT_ROOT}/lib.sh"

check_tool helm

cd "${SCRIPT_ROOT}/.."

if ! helm list | grep -q "netdata"; then
    echo "Netdata not installed; installing..."

    helm repo add netdata https://netdata.github.io/helmchart/

    helm upgrade netdata netdata/netdata \
        --install \
        --wait
fi

# kubectl port-forward svc/netdata 8888:19999
