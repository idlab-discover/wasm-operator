#!/usr/bin/env bash
set -o errexit
set -o nounset
set -o pipefail
set -o xtrace


trap "exit" INT TERM
trap "sudo kill 0" EXIT

SCRIPT_ROOT=$(realpath $(dirname "${BASH_SOURCE}"))
source "${SCRIPT_ROOT}/../devel/tool.sh"

cd "${SCRIPT_ROOT}/.."
nrworkers=20
sleepperiod=60
run=1
nritters=100

export COMPILE_WITH_UNINSTANTIATE="TRUE"
export HEAP_MEM_SIZE=0
export RUST_BACKTRACE=1
./devel/create_cluster.sh
./devel/setup_wasm_rust.sh $nrworkers
./devel/test.sh $nrworkers 5 "wasm-rust" /tmp/setup_time.csv
#./profile/profile.sh wasm ./test_results_run$run/out_wasm_${nrworkers}_uninst.csv &
#profilePID=$!
./devel/test.sh $nrworkers $nritters "wasm-rust" ./test_results_run$run/out_wasm_${nrworkers}_time_uninst.csv


kubectl logs controller0 -n wasm-rust | head -n 30


sleep $((  60 ))
#sudo pkill -P $profilePID