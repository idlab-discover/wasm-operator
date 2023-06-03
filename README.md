# Kubernetes Operators in WebAssembly

This repository contains the code of a prototype runtime for running Kubernetes operators in WebAssembly. The goal is to improve the memory usage of a Kubernetes cluster by reducing the memory footprint of operators. This prototype reduces the overhead in three ways.

* It runs operators in a shared WebAssembly runtime to reduce the overhead of containerization.
* It swaps operators to disk when there are no changes to process.
* It uses the Rust programming language instead of Go.

For more information, read the paper [Adapting Kubernetes controllers to the edge: on-demand control planes using Wasm and WASI](https://doi.org/10.48550/arXiv.2209.01077) accepted to CloudNet 2022.

This project builds upon [this proof of concept](https://github.com/slinkydeveloper/extending-kubernetes-api-in-process-poc).

```text
+-- 📂controllers                       # All child operators / components used for testing
|   +-- 📂comb-rust-controller          # Rust combined operator (no isolation)
|   +-- 📂ring-go-controller            # Go operator (container-based)
|   +-- 📂ring-rust-controller          # Rust operator (container-based and WASM-based)
|   +-- 📂simple-rust-controller        # simple child operator (container-based and WASM-based)
|   +-- 📂value-changer                 # script to change watched resources based on traces to emulate resource changes
    +-- 📂mongodbSpammer                # script that spams a mongodb server, to test influence of heavy load server on reconcile time

|   :
+-- 📂devel                             # Tools for building & deploying
+-- 📂full_test                         # Scripts for running e2e test & benchmark
    +-- run_wasm.sh                     # Script to run the  wasm based operator inside our framework, this is the main script
+-- 📂pkg
|   +-- 📂controller                    # Parent controller
|   +-- 📂kube-rs                       # Modified kube-rs library
|   +-- 📂kube-runtime-abi              # ABI for making Kubernetes API requests from within child operator
|   :
+-- 📂profile                           # Cgroup v2 memory usage measuring
+-- 📂test                              # Deployment files for tests
+-- 📂prediction                        # Prediction related benchmarks/server
    +-- 📂models                        # Tests/experiments using different prediction models
    +-- 📂webserver                     # Webserver flask api that predicts future values
:
```

## Run all e2e tests and benchmarks (old)

```console
> ./full_test/run.sh
```

## Run wasm test only

```console
> ./full_test/run_wasm.sh
```

## Copyright

This code is released under the Apache License Version 2.0.

This prototype was initially developed by Tim Ramlot as part of his Master's dissertation.
This prototype was later extended by Kevin Van Landuyt as part of his Master's dissertation .
