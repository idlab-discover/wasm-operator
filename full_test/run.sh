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

mkdir -p "test_results"

: '
for (( j = 1; j <= 10; j+=1 ))
do
    if [ ! -f ./test_results/out_golang_$j.csv ]; then
        ./devel/create_cluster.sh
        ./devel/setup_native_golang.sh $j
        sudo ./profile/profile.py golang $j ./test_results/out_golang_$j.csv &
        profilePID=$!
        ./devel/test.sh $j 200 "native-golang" ./test_results/out_golang_${j}_time.csv
        sleep 240
        sudo pkill -P $profilePID
    fi
    
    if [ ! -f ./test_results/out_rust_$j.csv ]; then
        ./devel/create_cluster.sh
        ./devel/setup_native_rust.sh $j
        sudo ./profile/profile.py rust $j ./test_results/out_rust_$j.csv &
        profilePID=$!
        ./devel/test.sh $j 200 "native-rust" ./test_results/out_rust_${j}_time.csv
        sleep 240
        sudo pkill -P $profilePID
    fi
    
    export COMPILE_WITH_UNINSTANTIATE="TRUE"

    if [ ! -f ./test_results/out_wasm_${j}_uninst.csv ]; then
        ./devel/create_cluster.sh
        ./devel/setup_wasm_rust.sh $j
        sudo ./profile/profile.py wasm $j ./test_results/out_wasm_${j}_uninst.csv &
        profilePID=$!
        ./devel/test.sh $j 200 "wasm-rust" ./test_results/out_wasm_${j}_uninst_time.csv
        sleep 240
        sudo pkill -P $profilePID
    fi

    export COMPILE_WITH_UNINSTANTIATE="FALSE"

    if [ ! -f ./test_results/out_wasm_$j.csv ]; then
        ./devel/create_cluster.sh
        ./devel/setup_wasm_rust.sh $j
        sudo ./profile/profile.py wasm $j ./test_results/out_wasm_$j.csv &
        profilePID=$!
        ./devel/test.sh $j 200 "wasm-rust" ./test_results/out_wasm_${j}_time.csv
        sleep 240
        sudo pkill -P $profilePID
    fi
done
'

for (( j = 10; j <= 100; j+=20 ))
do
    if [ ! -f ./test_results/out_comb_$j.csv ]; then
        ./devel/create_cluster.sh
        ./devel/setup_native_rust_combined.sh $j
        sudo ./profile/profile.py comb $j ./test_results/out_comb_$j.csv &
        profilePID=$!
        ./devel/test.sh $j 200 "native-rust-comb" ./test_results/out_comb_${j}_time.csv
        sleep 240
        sudo pkill -P $profilePID
    fi

    if [ ! -f ./test_results/out_golang_$j.csv ]; then
        ./devel/create_cluster.sh
        ./devel/setup_native_golang.sh $j
        sudo ./profile/profile.py golang $j ./test_results/out_golang_$j.csv &
        profilePID=$!
        ./devel/test.sh $j 200 "native-golang" ./test_results/out_golang_${j}_time.csv
        sleep 240
        sudo pkill -P $profilePID
    fi

    if [ ! -f ./test_results/out_rust_$j.csv ]; then
        ./devel/create_cluster.sh
        ./devel/setup_native_rust.sh $j
        sudo ./profile/profile.py rust $j ./test_results/out_rust_$j.csv &
        profilePID=$!
        ./devel/test.sh $j 200 "native-rust" ./test_results/out_rust_${j}_time.csv
        sleep 240
        sudo pkill -P $profilePID
    fi

    export COMPILE_WITH_UNINSTANTIATE="TRUE"
    
    if [ ! -f ./test_results/out_wasm_${j}_uninst.csv ]; then
        ./devel/create_cluster.sh
        ./devel/setup_wasm_rust.sh $j
        sudo ./profile/profile.py wasm $j ./test_results/out_wasm_${j}_uninst.csv &
        profilePID=$!
        ./devel/test.sh $j 200 "wasm-rust" ./test_results/out_wasm_${j}_time_uninst.csv
        sleep 240
        sudo pkill -P $profilePID
    fi

    export COMPILE_WITH_UNINSTANTIATE="FALSE"
    
    if [ ! -f ./test_results/out_wasm_$j.csv ]; then
        ./devel/create_cluster.sh
        ./devel/setup_wasm_rust.sh $j
        sudo ./profile/profile.py wasm $j ./test_results/out_wasm_$j.csv &
        profilePID=$!
        ./devel/test.sh $j 200 "wasm-rust" ./test_results/out_wasm_${j}_time.csv
        sleep 240
        sudo pkill -P $profilePID
    fi
done
