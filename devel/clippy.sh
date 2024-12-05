#!/usr/bin/env bash
set -o errexit
set -o nounset
set -o pipefail

ROOT_DIR=$(realpath $(dirname $(dirname "${BASH_SOURCE}")))

# Required for /pkg/controller to compile
export COMPILE_WITH_UNINSTANTIATE="TRUE"
# Required for /controllers/comb-rust-controller and /controllers/ring-rust-controller to compile
export COMPILE_NONCE="REPLACEMEREPLACEME"

cd "${ROOT_DIR}/pkg/controller" && cargo clippy --all
cd "${ROOT_DIR}/pkg/kube-rs" && cargo clippy --all
cd "${ROOT_DIR}/pkg/kube-runtime-abi" && cargo clippy --all
cd "${ROOT_DIR}/pkg/wasm-delay-queue" && cargo clippy --all

cd "${ROOT_DIR}/controllers/comb-rust-controller" && cargo clippy --all -F client
cd "${ROOT_DIR}/controllers/mongodbSpammer" && cargo clippy --all
cd "${ROOT_DIR}/controllers/ring-rust-controller" && cargo clippy --all -F client
cd "${ROOT_DIR}/controllers/simple-rust-controller" && cargo clippy --all -F client
cd "${ROOT_DIR}/controllers/value-changer" && cargo clippy --all
