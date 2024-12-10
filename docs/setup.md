# Setup of the WASM-operator

The project is setup with automated scripts that manage this process automatically.
This guide however allows to fine-tune the deployment and to gain a better
understanding of the project.
We will highlight which shell file automates the process

## Dependencies
>
> These are managed by [`devel/tool.sh`](../devel/tool.sh) and often installed
> (through curl in a local folder) when not present

### Setting up the Kubernetes environment

- [Kind](https://kind.sigs.k8s.io/) - Used as a local, lightweight Kubernetes cluster
- [Kubectl](https://kubernetes.io/docs/reference/kubectl/)
- [Docker](https://www.docker.com/)

### Compiling the projects

- [Rust](https://www.rust-lang.org/) - Required to build the parent controller
and child controllers
- [Go](https://go.dev/) - Required to build the `ring-go-controller` which is
used as a comparison
- [Cross](https://crates.io/crates/cross) - Easier cross-compilation than with
cargo, while providing isolation through Docker containers
- [wasm-opt](https://github.com/WebAssembly/binaryen) - Required to optimize
the WASM output from cross

### Optional
>
> - [sccache](https://github.com/mozilla/sccache) - Compiler caching tool which
can be used to speed up compilation through setting `export RUSTC_WRAPPER=sccache`
> - [Python3 + pip3](https://www.python.org/) - Can be used to setup the
webserver for predictions locally

### Tools mentioned in [`devel/tool.sh`](../devel/tool.sh), but not used

- [sccache](https://github.com/mozilla/sccache)  
  Automatically installed when executing a shell script from devel
- [Python3 + pip3](https://www.python.org/)
- [Helm](https://helm.sh/)
- [kube-apiserver + etcd](https://github.com/kubernetes-sigs/kubebuilder)

## Creating a Kind cluster
>
> Original file: [`devel/create_cluster.sh`](../devel/create_cluster.sh)

**Kind** (Kubernetes IN Docker) is a tool designed to run Kubernetes clusters
locally using Docker containers. It is lightweight, easy to configure, and ideal
for testing and development environments.

It is the recommended way of testing out the project and thus most tested.
Be sure to create an issue if any problems arise on other Kubernetes
environments however.

The following code snippet creates a Kind cluster using our default config.
This config does the following:

- Mounts the containerd directory
- Sets static values for the `dnsDomain`, `podSubnet` and `serviceSubnet`
- Increases the `maxPods` setting for the kubelet to support up to 1100 pods
- Enables performance improvement for etcd

```sh
kind create cluster \
  --name "wasm-operator" \
  --config "./devel/kind-config.yaml"
```

## Setting up the Flask server
>
> Original file: [`devel/setup_flask_server.sh`](../devel/setup_flask_server.sh)

The Flask server is in order to enable prediction.
It is deployed within our Kubernetes cluster and provides POST method
"/prediction" to enable predictions on when to wake up.
The code can be found in the `./prediction/webserver` directory.

To build the docker container and load the image into Kind:

```sh
docker build -t prediction_webserver:webserver ./prediction/webserver
kind load docker-image --name wasm-operator prediction_webserver:webserver
```

We then just need to create a Deployment + Service to deploy and expose our pod.
This can be done using the following manifest:

```sh
kubectl apply -f ./tests/yaml/deploymentFlask.yaml
```

## Building the parent WASM-operator
>
> Original file: [`devel/setup_wasm_rust_simple.sh`](../devel/setup_wasm_rust_simple.sh)
> / [`devel/setup_wasm_rust.sh`](../devel/setup_wasm_rust.sh)

The operator can be built using cross. Setting the target to x86_64-unknown-linux-musl
allows the binary to remain light weight and work on many Linux distributions
due to static linking with musl libc.
It is a great fit for the Docker image we're going to use: gcr.io/distroless/cc:nonroot

The parent operator currently does not support loading child operators at runtime.
Due to the difficulties with mounting volumes in Kubernetes environments,
the operator image copies over the config file (wasm_config.yaml)
and the WASM files for the child operators.

```sh
cd ./pkg/controller
export COMPILE_WITH_UNINSTANTIATE=TRUE
cross build --release --target=x86_64-unknown-linux-musl
```
