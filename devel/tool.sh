#!/usr/bin/env bash
if ! (return 0 2>/dev/null); then
  set -o errexit
  set -o nounset
  set -o pipefail
fi

LIB_SCRIPT_ROOT=$(realpath $(dirname "${BASH_SOURCE}"))
TOOLS_PATH=$(realpath "${LIB_SCRIPT_ROOT}/bin")
mkdir -p "${TOOLS_PATH}"
PATH="$PATH:$TOOLS_PATH"
PATH="$PATH:/usr/local/go/bin/"

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
    curl -Lo "${TOOLS_PATH}/kind" "https://kind.sigs.k8s.io/dl/v0.13.0/kind-linux-amd64"
    chmod +x "${TOOLS_PATH}/kind"
  }
}

check_tool_kubectl() {
  executable_exist kubectl || {
    curl -Lo "${TOOLS_PATH}/kubectl" "https://dl.k8s.io/release/v1.24.0/bin/linux/amd64/kubectl"
    chmod +x "${TOOLS_PATH}/kubectl"
  }
}

check_tool_helm() {
  executable_exist helm || {
    pushd "${TOOLS_PATH}"

    curl -Lo helm.tar.gz "https://get.helm.sh/helm-v3.8.2-linux-amd64.tar.gz"

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

    curl -Lo kubebuilder-tools.tar.gz https://storage.googleapis.com/kubebuilder-tools/kubebuilder-tools-1.23.6-linux-amd64.tar.gz
    
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

    source $HOME/.cargo/env
  }

  rustup target add wasm32-wasi
  cargo install cargo-wasi

  check_tool sccache
  RUSTC_WRAPPER=sccache
}

check_tool_go() {
  executable_exist go || {
    pushd "${TOOLS_PATH}"
    curl -Lo go1.18.2.linux-amd64.tar.gz https://go.dev/dl/go1.18.2.linux-amd64.tar.gz

    sudo rm -rf /usr/local/go
    sudo tar -C /usr/local -xzf go1.18.2.linux-amd64.tar.gz

    rm -rf go1.18.2.linux-amd64.tar.gz
    popd
  }
}

check_tool_docker() {
  # TODO: remove this UBUNTU workaround
  # .wslconfig should contain 'kernelCommandLine=systemd.unified_cgroup_hierarchy=false systemd.legacy_systemd_cgroup_controller=false cgroup_no_v1=all'
  if sudo mount -l | grep -q "/sys/fs/cgroup/unified"; then
    echo "switching to cgroup v2 (non-hybrid)"
    sudo umount -l /sys/fs/cgroup/unified
    sudo umount -l /sys/fs/cgroup
    sleep 2
    sudo mount -t cgroup2 -o rw,nosuid,nodev,noexec,relatime,nsdelegate cgroup2 /sys/fs/cgroup
  fi

  executable_exist docker || {
    read -p "Installing docker globally; Press enter to continue"
      
    curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo gpg --dearmor -o /usr/share/keyrings/docker-archive-keyring.gpg
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/docker-archive-keyring.gpg] https://download.docker.com/linux/ubuntu $(lsb_release -cs) stable" | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null

    sudo apt update -qq >/dev/null 2>&1
    sudo apt install -qq -y docker-ce apt-transport-https >/dev/null 2>&1
    sudo systemctl restart docker
    sudo systemctl enable docker >/dev/null 2>&1
  }
  
  if ! groups | grep -q "docker"; then
    sudo getent group docker || sudo groupadd docker
    sudo usermod -a -G docker $USER

    cmdline=$(cat /proc/$$/cmdline | xargs -0 echo)
    exec sudo su -l $USER -c "cd $(pwd); $cmdline"
  fi
}

check_tool_wasm-opt() {
  executable_exist wasm-opt || {
    pushd "${TOOLS_PATH}"

    curl -Lo binaryen-version_105.tar.gz https://github.com/WebAssembly/binaryen/releases/download/version_105/binaryen-version_105-x86_64-linux.tar.gz
    
    tar -zxvf binaryen-version_105.tar.gz

    mv ./binaryen-version_105/bin/wasm-opt ./wasm-opt

    rm -rf ./binaryen-version_105/
    rm -rf binaryen-version_105.tar.gz
    popd
  }
}

check_tool_wasm2wat() {
  executable_exist wasm2wat || {
    pushd "${TOOLS_PATH}"

    curl -Lo wabt-1.0.27.tar.gz https://github.com/WebAssembly/wabt/releases/download/1.0.27/wabt-1.0.27-ubuntu.tar.gz
    
    tar -zxvf wabt-1.0.27.tar.gz

    mv ./wabt-1.0.27/bin/wasm2wat ./wasm2wat
    mv ./wabt-1.0.27/bin/wat2wasm ./wat2wasm
    mv ./wabt-1.0.27/bin/wasm-strip ./wasm-strip
    mv ./wabt-1.0.27/bin/wasm2c ./wasm2c
    mv ./wabt-1.0.27/bin/wasm-decompile ./wasm-decompile
    mv ./wabt-1.0.27/bin/wasm-objdump ./wasm-objdump

    rm -rf ./wabt-1.0.27/
    rm -rf wabt-1.0.27.tar.gz
    popd
  }
}

check_tool_heaptrack() {
  executable_exist heaptrack || {
    pushd "${TOOLS_PATH}"

    curl -Lo heaptrack https://download.kde.org/stable/heaptrack/1.3.0/heaptrack-v1.3.0-x86_64.AppImage

    chmod +x ./heaptrack

    popd
  }
}

check_tool_Cross() {
  executable_exist cross || {
    pushd "${TOOLS_PATH}"

    cargo install cross --git https://github.com/cross-rs/cross

    popd
  }
}





# Config variables
KIND_CLUSTER_NAME="kind"

if ! (return 0 2>/dev/null); then
  if [[ $# -eq 0 ]]; then
    echo "usage:"
    echo "$ source $0"
    echo "or"
    echo "$ $0 kubectl get pods"
    exit 0
  fi

  check_tool $1

  $@
fi
