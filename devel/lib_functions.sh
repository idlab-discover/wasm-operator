#!/usr/bin/env bash
set -o errexit
set -o nounset
set -o pipefail

generate_wasm_yaml_file_simple() {

  cat <<EOF
name: "simplecontroller"
wasm: ./simple-pod-example.wasm
env:
- name: RUST_LOG
  value: "info"
- name: HEAP_MEM_SIZE
  value: "$HEAP_MEM_SIZE"
---
EOF

}

generate_wasm_yaml_file() {
  NR_CONTROLLERS=$1
  NAME=$2

  for ((CONTROLLER_NR = 0; CONTROLLER_NR < NR_CONTROLLERS; CONTROLLER_NR++)); do

    cat <<EOF
name: ${NAME}${CONTROLLER_NR}
wasm: ./ring-rust-example.controller${CONTROLLER_NR}.wasm
env:
- name: RUST_LOG
  value: "info"
- name: IN_NAMESPACE
  value: "${NAME}${CONTROLLER_NR}"
- name: OUT_NAMESPACE
  value: "${NAME}$(((CONTROLLER_NR + 1) % NR_CONTROLLERS))"
- name: HEAP_MEM_SIZE
  value: "$HEAP_MEM_SIZE"
---
EOF

  done
}

generate_pod_yaml_file() {
  NR_CONTROLLERS=$1
  NAME=$2
  IMAGE=$3

  for ((CONTROLLER_NR = 0; CONTROLLER_NR < NR_CONTROLLERS; CONTROLLER_NR++)); do

    cat <<EOF
apiVersion: v1
kind: Pod
metadata:
  name: controller${CONTROLLER_NR}
  namespace: ${NAME}
spec:
  serviceAccountName: custom-controller
  containers:
  - name: controller
    image: "${IMAGE}controller${CONTROLLER_NR}"
    env:
    - name: RUST_LOG
      value: "info"
    - name: IN_NAMESPACE
      value: "${NAME}${CONTROLLER_NR}"
    - name: OUT_NAMESPACE
      value: "${NAME}$(((CONTROLLER_NR + 1) % NR_CONTROLLERS))"
    - name: HEAP_MEM_SIZE
      value: "$HEAP_MEM_SIZE"
---
EOF

  done
}

generate_pod_yaml_file_combined() {
  NR_CONTROLLERS=$1
  NAME=$2
  IMAGE=$3

  cat <<EOF
apiVersion: v1
kind: Pod
metadata:
  name: controller
  namespace: ${NAME}
spec:
  serviceAccountName: custom-controller
  containers:
  - name: controller
    image: "${IMAGE}controller"
    env:
    - name: RUST_LOG
      value: "info"
    - name: NR_OPERATORS
      value: "${NR_CONTROLLERS}"
---
EOF
}

generate_pod_yaml_file_simple_rust() {
  SERVER=$1

  cat <<EOF
apiVersion: v1
kind: Pod
metadata:
  name: controller0
  namespace: wasm-rust-simple
spec:
  serviceAccountName: custom-controller
  containers:
  - name: controller
    image: "github.com/amurant/wasm_rust_simple:controller0"
    env:
    - name: RUST_LOG
      value: "info"
    - name: PREDICTION_SERVER
      value: "${SERVER}"
    resources:
      requests:
        memory: "640Mi"
      limits:
        memory: "1280Mi"
  
EOF

}

generate_namespace_yaml_file() {
  NR_CONTROLLERS=$1
  NAME=$2

  for ((CONTROLLER_NR = 0; CONTROLLER_NR < NR_CONTROLLERS; CONTROLLER_NR++)); do

    cat <<EOF
apiVersion: v1
kind: Namespace
metadata:
  name: "${NAME}${CONTROLLER_NR}"
---
EOF

  done
}

generate_namespace_yaml_file_simple() {
  NR_CONTROLLERS=$1
  NAME=$2

  for ((CONTROLLER_NR = 0; CONTROLLER_NR < NR_CONTROLLERS; CONTROLLER_NR++)); do

    cat <<EOF
apiVersion: v1
kind: Namespace
metadata:
  name: "${NAME}"
---
EOF

  done
}
