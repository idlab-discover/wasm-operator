# Master thesis project - Optimising memory usage of Kubernetes operators using WASM

:rocket: Builds upon [this proof of concept (PoC)](https://github.com/slinkydeveloper/extending-kubernetes-api-in-process-poc)

```text
+-- 📂controllers                       # All operators used for testing
|   +-- 📂comb-rust-controller          # Rust combined operator (no isolation)
|   +-- 📂ring-go-controller            # Go operator (container-based)
|   +-- 📂ring-rust-controller          # Rust operator (container-based and WASM-based)
|   :
+-- 📂devel                             # Tools for building & deploying
+-- 📂full_test                         # Script for running e2e test & benchmark
+-- 📂pkg
|   +-- 📂controller                    # Parent controller
|   +-- 📂kube-rs                       # Modified kube-rs library
|   +-- 📂kube-runtime-abi              # ABI for making Kubernetes API requests from within child operator
|   :
+-- 📂profile                           # Cgroup v2 memory usage measuring
+-- 📂test                              # Deployment files for tests
:
```

## Run all e2e tests and benchmarks

```console
> ./full_test/run.sh
```
