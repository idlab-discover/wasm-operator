#!/usr/bin/env bash
set -e

SCRIPT_ROOT=$(dirname "${BASH_SOURCE}")
source "${SCRIPT_ROOT}/lib.sh"

cd "${SCRIPT_ROOT}/../"

check_tool docker
check_tool kind

local_tag="local"

docker build -f native.dockerfile -t "github.com/amurant/native_controller:${local_tag}" .

kind load docker-image --name "${KIND_CLUSTER_NAME}" "github.com/amurant/native_controller:${local_tag}"

cd "./poc/"

mkdir temp || true
mkdir temp/native || true

cp ../deploy/chart/crds/crd.yaml temp/00_crd.yaml
kubectl apply -f ./temp/00_crd.yaml

NR_CONTROLLERS=1

cat << EOF >> temp/01_namespaces.yaml
kind: ClusterRole
apiVersion: rbac.authorization.k8s.io/v1
metadata:
  name: parent-controller
rules:
- apiGroups:
  - amurant.io
  resources:
  - resources
  verbs:
  - "*"
---
kind: ClusterRoleBinding
apiVersion: rbac.authorization.k8s.io/v1
metadata:
  name: parent-controller
subjects:
- kind: ServiceAccount
  name: parent-controller
  namespace: controller-namespace
roleRef:
  kind: ClusterRole
  name: parent-controller
  apiGroup: rbac.authorization.k8s.io
---
apiVersion: v1
kind: Namespace
metadata:
  name: "controller-namespace"
EOF

for (( VARIABLE = 0; VARIABLE < NR_CONTROLLERS; VARIABLE++ ))
do

cat << EOF >> temp/01_namespaces.yaml
---
apiVersion: v1
kind: Namespace
metadata:
  name: namespace${VARIABLE}
EOF

cat << EOF > temp/native/controller${VARIABLE}.yaml
apiVersion: v1
kind: Pod
metadata:
  name: controller${VARIABLE}
  namespace: controller-namespace
spec:
  serviceAccountName: parent-controller
  containers:
  - name: controller
    image: "github.com/amurant/native_controller:${local_tag}"
    env:
    - name: IN_NAMESPACE
      value: "namespace${VARIABLE}"
    - name: OUT_NAMESPACE
      value: "namespace$(((VARIABLE+1) % NR_CONTROLLERS))"
EOF

done

cat << EOF > temp/02_resource.yaml
apiVersion: amurant.io/v1
kind: Resource
metadata:
    name: run001
    namespace: namespace0
spec:
    nonce: 0
EOF

kubectl apply -f ./temp/

kubectl apply -f ./temp/native/
