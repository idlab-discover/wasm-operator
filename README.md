# Kubernetes Operators in WebAssembly

This repository contains the code of a prototype runtime for running Kubernetes operators in WebAssembly. The goal is to improve the memory usage of a Kubernetes cluster by reducing the memory footprint of operators. This prototype reduces the overhead in three ways.

- It runs operators in a shared WebAssembly runtime to reduce the overhead of containerization.
- It swaps operators to disk when there are no changes to process.
- It uses the Rust programming language instead of Go.

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

## Get involved

This is an open source project, currently in the prototyping phase.
We greatly value feedback, bug reports, contributions,... during this stage of the project.

- to provide bug reports, feedback or suggestions, [create an issue](https://github.com/idlab-discover/wasm-operator/issues/new)
- to contribute code, see [contributing.md](docs/contributing.md)

## Getting started

### Setup of the project

A list of dependencies and the steps for setting up the project can be found in the [Setup documentation](./docs/setup.md).

Note that this does not deploy the parent operator (yet).
Due to this project still being in active development, child operators can't be loaded at runtime and have to be copied (in combination with the configuration) to the Docker image.
It is thus more appropriate to explain the setup of the operator in the [Usage section](./docs/usage.md).

### Using the prototype and deploying child operators

The project currently contains 3 operators that have been created to test the WASM prototype. The setup of the parent operator and these child operators can be found in [Usage documentation](./docs/usage.md). There is currently no reason why operators that can be compiled down to WASM wouldn't work out of the box, but this has currently not been tested. We welcome contributions / experiences about the use of the prototype.

> [!NOTE]
> The current [Usage documentation](./docs/usage.md) mainly focusses on the simple-rust-controller, since this is the most recent one.
> The instructions should however also work for the comb-rust-controller and ring-rust-controller since these are very similar.

### Testing the simple-rust, ring-rust and comb-rust controllers

The deployed operator should work as normal when the setup and usage sections have been fulfilled. In order to test this for the provided examples, we have provided a test function that can be executed. This is however very WIP. It can be found in the [Testing documentation](./docs/testing.md)

### Profiling the simple-rust, ring-rust and comb-rust controllers

Profiling currently happens through a Python script.
More information can be found (WIP) in the [Profiling documentation](./docs/profiling.md)

## Benchmark the solutions

The scripts below are the original way the project was to be tested.
They install the required dependencies in case they can't be found and execute the other steps outlined in the Setup and Usage sections above.

### Testing the native Rust, Go and WASM-Rust solutions for different parameters

```sh
./full_test/run.sh
```

## Testing the WASM-Rust solution with prediction

```sh
./full_test/run_wasm.sh
```

## Copyright

This code is released under the Apache License Version 2.0.

This prototype was initially developed by Tim Ramlot as part of his Master's dissertation.
This prototype was later extended by Kevin Van Landuyt as part of his Master's dissertation .
