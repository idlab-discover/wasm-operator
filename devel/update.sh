#!/usr/bin/env bash
set -o errexit
set -o nounset
set -o pipefail

ROOT_DIR=$(realpath $(dirname $(dirname "${BASH_SOURCE}")))

cd "${ROOT_DIR}/pkg/controller" && cargo clean 
cd "${ROOT_DIR}/pkg/kube-rs" && cargo clean 
cd "${ROOT_DIR}/pkg/kube-runtime-abi" && cargo clean
cd "${ROOT_DIR}/pkg/wasm-delay-queue" && cargo clean 

cd "${ROOT_DIR}/controllers/ring-rust-controller" && cargo clean 
cd "${ROOT_DIR}/controllers/simple-rust-controller" && cargo clean 
