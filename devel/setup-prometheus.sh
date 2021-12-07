#!/usr/bin/env bash
# set -o errexit
# set -o nounset
# set -o pipefail

SCRIPT_ROOT=$(realpath $(dirname "${BASH_SOURCE}"))
source "${SCRIPT_ROOT}/lib.sh"

check_tool helm

if ! helm -n prom list | grep -q "prometheus"; then
    echo "Netdata not installed; installing..."

    helm repo add prometheus-community https://prometheus-community.github.io/helm-charts

    kubectl create ns prom

    helm install --wait prometheus prometheus-community/kube-prometheus-stack -n prom
    kubectl -n prom apply -f "${SCRIPT_ROOT}/custom-dashboard.yaml"
fi

admin_password=$(kubectl -n prom get secret prometheus-grafana -o jsonpath="{.data.admin-password}" | base64 --decode)

echo "Prometheus is now running on http://localhost:9090"
echo "Grafana is now running on http://localhost:8000 (admin:${admin_password})"

kubectl -n prom port-forward svc/prometheus-kube-prometheus-prometheus 9090:9090 &
kubectl -n prom port-forward svc/prometheus-grafana 8000:80

trap function() {
    echo "Cleaning up..."
    kill %1;
} EXIT
