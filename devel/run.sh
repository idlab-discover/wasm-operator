#!/usr/bin/env bash
set -e

SCRIPT_ROOT=$(dirname "${BASH_SOURCE}")
source "${SCRIPT_ROOT}/lib.sh"

cd "${SCRIPT_ROOT}/../"

mkdir -p ./wasm/
cp ./ext-simple-pod/simple-pod.yaml ./wasm/simple_pod.yaml
cp ./ext-simple-pod/target/wasm32-wasi/release/simple-pod.wasi.wasm ./wasm/simple_pod.wasm
RUST_LOG=rust_host=debug,cranelift=warn,kube=debug cargo run -- ./wasm/
