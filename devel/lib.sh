#!/usr/bin/env bash
set -o errexit
set -o nounset
set -o pipefail

LIB_SCRIPT_ROOT=$(realpath $(dirname "${BASH_SOURCE}"))
TOOLS_PATH=$(realpath "${LIB_SCRIPT_ROOT}/bin")
mkdir -p "${TOOLS_PATH}"
PATH="$PATH:$TOOLS_PATH"

fn_exists() { declare -F "$1" > /dev/null; }

check_tool() {
  tool="$1"
  if ! fn_exists "check_tool_${tool}"; then
    echo "ERROR: tool ${tool} not found"
    exit 1
  fi

  "check_tool_${tool}"
}

executable_exist() {
  tool="$1"
  if command -v "$tool" &>/dev/null; then
    return 0 # executable was found
  fi

  return 1 # executable was not found
}

check_tool_kind() {
  executable_exist kind || {
    curl -Lo "${TOOLS_PATH}/kind" "https://kind.sigs.k8s.io/dl/v0.11.1/kind-linux-amd64"
    chmod +x "${TOOLS_PATH}/kind"
  }
}

check_tool_kubectl() {
  executable_exist kubectl || {
    curl -Lo "${TOOLS_PATH}/kubectl" "https://dl.k8s.io/release/v1.22.1/bin/linux/amd64/kubectl"
    chmod +x "${TOOLS_PATH}/kubectl"
  }
}

check_tool_helm() {
  executable_exist helm || {
    pushd "${TOOLS_PATH}"

    curl -Lo helm.tar.gz "https://get.helm.sh/helm-v3.6.3-linux-amd64.tar.gz"

    tar -zxvf helm.tar.gz

    mv ./linux-amd64/helm "./helm"

    rm -rf ./linux-amd64/
    rm -rf helm.tar.gz
    popd
  }
}

check_tool_kube-apiserver() { check_tool_kube-apiserver_and_etcd; }

check_tool_etcd() { check_tool_kube-apiserver_and_etcd; }

check_tool_kube-apiserver_and_etcd() {
  (
    executable_exist kube-apiserver &&
    executable_exist etcd
  ) || {
    pushd "${TOOLS_PATH}"

    curl -Lo kubebuilder-tools.tar.gz https://storage.googleapis.com/kubebuilder-tools/kubebuilder-tools-1.22.0-linux-amd64.tar.gz
    
    tar -zxvf kubebuilder-tools.tar.gz

    mv ./kubebuilder/bin/kube-apiserver ./kube-apiserver
    mv ./kubebuilder/bin/etcd ./etcd

    rm -rf ./kubebuilder/
    rm -rf kubebuilder-tools.tar.gz
    popd
  }
}

check_tool_sccache() {
  executable_exist sccache || {
    pushd "${TOOLS_PATH}"
    curl -Lo sccache.tar.gz https://github.com/mozilla/sccache/releases/download/v0.2.15/sccache-v0.2.15-x86_64-unknown-linux-musl.tar.gz

    tar -zxvf sccache.tar.gz

    mv ./sccache-v0.2.15-x86_64-unknown-linux-musl/sccache "./sccache"

    rm -rf ./sccache-v0.2.15-x86_64-unknown-linux-musl/
    rm -rf sccache.tar.gz
    popd
  }
}

check_tool_rust() {
  executable_exist rustup || {
    read -p "Installing rust globally; Press enter to continue"

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  }

  rustup target add wasm32-wasi
  cargo install cargo-wasi

  check_tool sccache
  RUSTC_WRAPPER=sccache
}

check_tool_go() {
  executable_exist go || {
    pushd "${TOOLS_PATH}"
    curl -Lo go1.17.3.linux-amd64.tar.gz https://go.dev/dl/go1.17.3.linux-amd64.tar.gz

    sudo rm -rf /usr/local/go
    sudo tar -C /usr/local -xzf go1.17.3.linux-amd64.tar.gz

    rm -rf go1.17.3.linux-amd64.tar.gz
    popd
  }
}

check_tool_docker() {
  executable_exist docker || {
    read -p "Installing docker globally; Press enter to continue"

    curl --proto '=https' --tlsv1.2 -sSf https://get.docker.com | sh
  }

  sudo mkdir /sys/fs/cgroup/systemd || true
  sudo mount -t cgroup -o none,name=systemd cgroup /sys/fs/cgroup/systemd || true

  sudo /etc/init.d/docker start &>/dev/null || true
}

# Config variables
KIND_CLUSTER_NAME="kind"
