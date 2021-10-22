#!/usr/bin/env bash
set -e

SCRIPT_ROOT=$(dirname "${BASH_SOURCE}")
source "${SCRIPT_ROOT}/lib.sh"

cd "${SCRIPT_ROOT}/../"

pushd ext-simple-pod

cargo wasi build --release

popd

cargo build --release
