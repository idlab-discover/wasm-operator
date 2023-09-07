#!/usr/bin/env bash
set -o errexit
set -o nounset
set -o pipefail

SCRIPT_ROOT=$(realpath $(dirname "${BASH_SOURCE}"))
source "${SCRIPT_ROOT}/tool.sh"

check_tool kubectl

test_controller_for_nonce() {
    run=$1
    nr_controllers=$2
    namespace_prefix=$3
    nonce=$4
    out_file=$5

    start=`date +%s%N`
    cat << EOF | kubectl apply -f -
apiVersion: amurant.io/v1
kind: TestResource
metadata:
    name: ${run}
    namespace: ${namespace_prefix}0
spec:
    nonce: ${nonce}
EOF

    last_index=$((nr_controllers-1))
    #kubectl get all --all-namespaces
    #kubectl logs 'pod/controller' -n 'native-rust-comb'
    echo "${run} - ${nonce}: waiting for ${namespace_prefix}${last_index}"
    while true
    do
        kubectl wait -n ${namespace_prefix}${last_index} TestResource ${run} --for=jsonpath='{.spec.nonce}'=$nonce || {
            echo "${run} - ${nonce}: FAILED waiting for ${namespace_prefix}${last_index}; retrying"

            sleep 5
            continue
        }

        break
    done
    echo "${run} - ${nonce}: FINISHED waiting for ${namespace_prefix}"

    end=`date +%s%N`
    runtime=$(((end-start)/1000000))
    echo "${start};${end};${runtime}" >> $out_file
    echo "${run} - ${nonce}: ${namespace_prefix}${last_index} is ready (${runtime}ms)"
}

test_controller() {
    run=$1
    nr_controllers=$2
    namespace_prefix=$3
    nr_cycles=$4
    out_file=$5

    echo "starttime;endtime;roundtime" > $out_file

    for (( j = 0; j < $nr_cycles; j++ ))
    do
        test_controller_for_nonce $run $nr_controllers $namespace_prefix $j $out_file
    done
}


NR_CONTROLLERS=$1
NR_CYCLES=$2
TYPE=$3 # "wasm-rust", "native-rust" or "native-golang"
OUT_FILE=$4

kubectl delete TestResource --all-namespaces --all

sudo swapoff -a

test_controller "run0" $NR_CONTROLLERS $TYPE $NR_CYCLES $OUT_FILE

kubectl delete TestResource --all-namespaces --all
