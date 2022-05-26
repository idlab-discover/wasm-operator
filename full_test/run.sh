#!/usr/bin/env bash
set -o errexit
set -o nounset
set -o pipefail
set -o xtrace

# ./full_test/run.sh > screen.out 2>&1

trap "exit" INT TERM
trap "sudo kill 0" EXIT

SCRIPT_ROOT=$(realpath $(dirname "${BASH_SOURCE}"))
source "${SCRIPT_ROOT}/../devel/tool.sh"

cd "${SCRIPT_ROOT}/.."

export HEAP_MEM_SIZE=0

for (( run = 0; run <= 5; run+=1 ))
do
    mkdir -p "test_results_run$run"

    for (( nrworkers = 70; nrworkers <= 100; nrworkers+=10 ))
    do
        nritters=500
        sleepperiod=$(( 200 + 3 * $nrworkers ))

        echo "operators: $nrworkers"
        echo "itter: $nritters"
        echo "sleepperiod: $sleepperiod"

        if [ ! -f ./test_results_run$run/out_comb_${nrworkers}.csv ]; then
            ./devel/create_cluster.sh
            ./devel/setup_native_rust_combined.sh $nrworkers
            ./devel/test.sh $nrworkers 5 "native-rust-comb" /tmp/setup_time.csv
            ./profile/profile.sh comb ./test_results_run$run/out_comb_${nrworkers}.csv &
            profilePID=$!
            ./devel/test.sh $nrworkers $nritters "native-rust-comb" ./test_results_run$run/out_comb_${nrworkers}_time.csv
            sleep $sleepperiod
            sudo pkill -P $profilePID
        fi

        if [ ! -f ./test_results_run$run/out_golang_${nrworkers}.csv ]; then
            ./devel/create_cluster.sh
            ./devel/setup_native_golang.sh $nrworkers
            ./devel/test.sh $nrworkers 5 "native-golang" /tmp/setup_time.csv
            ./profile/profile.sh golang ./test_results_run$run/out_golang_${nrworkers}.csv &
            profilePID=$!
            ./devel/test.sh $nrworkers $nritters "native-golang" ./test_results_run$run/out_golang_${nrworkers}_time.csv
            sleep $sleepperiod
            sudo pkill -P $profilePID
        fi

        if [ ! -f ./test_results_run$run/out_rust_${nrworkers}.csv ]; then
            ./devel/create_cluster.sh
            ./devel/setup_native_rust.sh $nrworkers
            ./devel/test.sh $nrworkers 5 "native-rust" /tmp/setup_time.csv
            ./profile/profile.sh rust ./test_results_run$run/out_rust_${nrworkers}.csv &
            profilePID=$!
            ./devel/test.sh $nrworkers $nritters "native-rust" ./test_results_run$run/out_rust_${nrworkers}_time.csv
            sleep $sleepperiod
            sudo pkill -P $profilePID
        fi

        export COMPILE_WITH_UNINSTANTIATE="TRUE"
        
        if [ ! -f ./test_results_run$run/out_wasm_${nrworkers}_uninst.csv ]; then
            ./devel/create_cluster.sh
            ./devel/setup_wasm_rust.sh $nrworkers
            ./devel/test.sh $nrworkers 5 "wasm-rust" /tmp/setup_time.csv
            ./profile/profile.sh wasm ./test_results_run$run/out_wasm_${nrworkers}_uninst.csv &
            profilePID=$!
            ./devel/test.sh $nrworkers $nritters "wasm-rust" ./test_results_run$run/out_wasm_${nrworkers}_time_uninst.csv
            sleep $(( $sleepperiod + 600 ))
            sudo pkill -P $profilePID
        fi

        export COMPILE_WITH_UNINSTANTIATE="FALSE"
        
        if [ ! -f ./test_results_run$run/out_wasm_${nrworkers}.csv ]; then
            ./devel/create_cluster.sh
            ./devel/setup_wasm_rust.sh $nrworkers
            ./devel/test.sh $nrworkers 5 "wasm-rust" /tmp/setup_time.csv
            ./profile/profile.sh wasm ./test_results_run$run/out_wasm_${nrworkers}.csv &
            profilePID=$!
            ./devel/test.sh $nrworkers $nritters "wasm-rust" ./test_results_run$run/out_wasm_${nrworkers}_time.csv
            sleep $(( $sleepperiod + 600 ))
            sudo pkill -P $profilePID
        fi
    done
done

export HEAP_MEM_SIZE=0

for (( run = 0; run <= 5; run+=1 ))
do
    mkdir -p "test_heap_results_run$run"

    for (( heap_mem_size = 0; heap_mem_size <= 3 * 1024 * 1024; heap_mem_size+=1024 * 1024 ))
    do
        nritters=500
        nrworkers=60
        sleepperiod=$(( 200 + 3 * $nrworkers ))

        echo "operators: $nrworkers"
        echo "itter: $nritters"
        echo "sleepperiod: $sleepperiod"

        export HEAP_MEM_SIZE=$heap_mem_size

        if [ ! -f ./test_heap_results_run$run/${heap_mem_size}_out_rust_${nrworkers}.csv ]; then
            ./devel/create_cluster.sh
            ./devel/setup_native_rust.sh $nrworkers
            ./devel/test.sh $nrworkers 5 "native-rust" /tmp/setup_time.csv
            ./profile/profile.sh rust ./test_heap_results_run$run/${heap_mem_size}_out_rust_${nrworkers}.csv &
            profilePID=$!
            ./devel/test.sh $nrworkers 200 "native-rust" ./test_heap_results_run$run/${heap_mem_size}_out_rust_${nrworkers}_time.csv
            sleep $sleepperiod
            sudo pkill -P $profilePID
        fi

        export COMPILE_WITH_UNINSTANTIATE="TRUE"
        
        if [ ! -f ./test_heap_results_run$run/${heap_mem_size}_out_wasm_${nrworkers}_uninst.csv ]; then
            ./devel/create_cluster.sh
            ./devel/setup_wasm_rust.sh $nrworkers
            ./profile/profile.sh wasm ./test_heap_results_run$run/${heap_mem_size}_out_wasm_${nrworkers}_uninst.csv &
            profilePID=$!
            ./devel/test.sh $nrworkers 200 "wasm-rust" ./test_heap_results_run$run/${heap_mem_size}_out_wasm_${nrworkers}_time_uninst.csv
            sleep $(( $sleepperiod + 600 ))
            sudo pkill -P $profilePID
        fi

        export COMPILE_WITH_UNINSTANTIATE="FALSE"
        
        if [ ! -f ./test_heap_results_run$run/${heap_mem_size}_out_wasm_${nrworkers}.csv ]; then
            ./devel/create_cluster.sh
            ./devel/setup_wasm_rust.sh $nrworkers
            ./profile/profile.sh wasm ./test_heap_results_run$run/${heap_mem_size}_out_wasm_${nrworkers}.csv &
            profilePID=$!
            ./devel/test.sh $nrworkers 200 "wasm-rust" ./test_heap_results_run$run/${heap_mem_size}_out_wasm_${nrworkers}_time.csv
            sleep $(( $sleepperiod + 600 ))
            sudo pkill -P $profilePID
        fi
    done
done
