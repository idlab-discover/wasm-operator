# Profiling the solution
> Original file: [`profile/profile.sh`](../profile/profile.sh) (with args: `wasm ./test_results_run$run/out_wasm_${nrworkers}_uninst.csv &`)

```sh
sudo "${SCRIPT_ROOT}/profile.py <TYPE> <OUTPUT_FILE>"
```

With type = "rust", "comb", "golang" or "wasm"
