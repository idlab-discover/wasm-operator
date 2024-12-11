# Testing the simple-rust, ring-rust and comb-rust controllers
>
> Original file: [`devel/test.sh`](../devel/test.sh) (with args: `<NR_WORKERS> <NR_CYCLES> <TYPE> <OUT_FILE>`)  
> With TYPE = "wasm-rust", "native-rust" or "native-golang"

```sh
export run="run0"
export namespace_prefix="wasm-rust"
export nr_controllers=1
export nonce=0

kubectl delete TestResource --all-namespaces --all

# Create resource for testing the operator
kubectl apply -f - <<EOF
apiVersion: amurant.io/v1
kind: TestResource
metadata:
    name: ${run}
    namespace: ${namespace_prefix}
spec:
    nonce: ${nonce}
EOF

echo "${run} - ${nonce}: waiting for ${namespace_prefix}"
while true
do
    kubectl wait -n ${namespace_prefix}${last_index} TestResource ${run} --for=jsonpath='{.spec.nonce}'=$nonce || {
        echo "${run} - ${nonce}: FAILED waiting for ${namespace_prefix}${last_index}; retrying"

        sleep 5
        continue
    }

    break
done
echo "${run} - ${nonce}: FINISHED waiting for ${namespace_prefix}"

kubectl delete TestResource --all-namespaces --all
```

> [!NOTE]
> The original script does this for multiple cycles (using a for loop)
