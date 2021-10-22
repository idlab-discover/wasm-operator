#!/usr/bin/env bash
set -e

SCRIPT_ROOT=$(dirname "${BASH_SOURCE}")
source "${SCRIPT_ROOT}/lib.sh"

check_tool kind

kind create cluster \
  --name "${KIND_CLUSTER_NAME}"
