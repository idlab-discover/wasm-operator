#!/usr/bin/env bash
set -o errexit
set -o nounset
set -o pipefail

SCRIPT_ROOT=$(realpath $(dirname "${BASH_SOURCE}"))

FAKE_GO_PATH=$(mktemp -d)
cleanup_fake_gopath() { rm -rf "$FAKE_GO_PATH" || true; }
trap cleanup_fake_gopath EXIT

mkdir -p "${FAKE_GO_PATH}/github.com/amurant"
ln -sf "${SCRIPT_ROOT}/.." "${FAKE_GO_PATH}/github.com/amurant/ring-go-operator"

GO_PATH=$(go env GOPATH)
CODEGEN_PKG="${GO_PATH}/src/k8s.io/code-generator"

bash "${CODEGEN_PKG}"/generate-groups.sh \
  "deepcopy,client,informer,lister" \
  github.com/amurant/ring-go-operator/pkg/generated \
  github.com/amurant/ring-go-operator/pkg/apis \
  testresource:v1 \
  --output-base "${FAKE_GO_PATH}" \
  --go-header-file "${SCRIPT_ROOT}/boilerplate.go.txt"
