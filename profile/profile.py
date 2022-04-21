#!/bin/python3
import os
import time
import datetime
from pathlib import Path

def h(x):
    """Translates a number of bytes to a human-readable string."""
    order = 0
    suffix = ['', 'k', 'M', 'G', 'T']
    max_order = len(suffix) - 1
    while abs(x) > 1024 and order < max_order:
        x /= 1024.0
        order += 1
    return '%.2f%s' % (x, suffix[order])

def log(*args):
    """Logs timestamped information to stdout."""
    ts = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S.%f")
    print(ts, *args)

class Location(str):
    def __call__(self, *args) -> 'Location':
        return Location(self.format(*args))

    def exists(self):
        return Path(self).exists()

    def read(self):
        with open(self, 'r') as f:
            return f.read()

    def write(self, value):
        with open(self, 'w') as f:
            f.write(value)

class Locations:
    PROC_CMDLINE = Location("/proc/{}/cmdline")
    PROC_CGROUP = Location("/proc/{}/cgroup")
    PROC_SMAP_ROLLUP = Location("/proc/{}/smaps_rollup")

    CGROUP_DIR = Location("/sys/fs/cgroup{}")
    MEM_CURRENT = Location("{}/memory.current")
    MEM_STAT = Location("{}/memory.stat")
    MEM_PRESSURE = Location("{}/memory.pressure")
    MEM_HIGH = Location("{}/memory.high")

    MEMINFO = Location("/proc/meminfo")

class ProcessList:
    def __init__(self):
        self.pids = {}
    
    def refresh(self):
        for process in os.listdir("/proc/"):
            if (not process.isdigit()) or (process in self.pids):
                continue

            pid = int(process)

            if not Locations.PROC_CMDLINE(pid).exists():
                continue

            try:
                self.pids[pid] = {
                    "pid": pid,
                    "cmdline": Locations.PROC_CMDLINE(pid).read().replace('\0', ' ').strip(),
                }
            except:
                continue
    
    def find(self, name):
        return [process for process in self.pids.values() if name == process["cmdline"]]
    
    def find_in(self, name):
        return [process for process in self.pids.values() if name in process["cmdline"]]
    
    def __str__(self):
        return "\n".join(
            "pid: {}\t cmdline: {}".format(pid['pid'], pid['cmdline'])
            for pid in self.pids.values()
        )

def remove_prefix(text, prefix):
    if text.startswith(prefix):
        return text[len(prefix):]
    return text

def get_containers_in_pod(path):
    return [
        os.path.join(path, d)
        for d in os.listdir(path)
        if Path(os.path.join(path, d)).is_dir()
    ]

def parse_shims(shim_processes):
    return {x["cmdline"].split(' ')[4]: x for x in shim_processes}

def parse_containers(processes):
    for process in processes:
        if ("cgroup_dir" in process) and ("container_id" in process):
            continue

        cgroup_dir = Locations.PROC_CGROUP(process["pid"]).read()
        cgroup_dir = [remove_prefix(x, "0::") for x in cgroup_dir.split('\n') if x.startswith("0::")][0].strip()
        cgroup_dir = Locations.CGROUP_DIR(cgroup_dir)
        
        process["cgroup_dir"] = cgroup_dir
        process["container_id"] = os.path.basename(cgroup_dir)
        process["pod_id"] = os.path.basename(os.path.dirname(cgroup_dir))

    return {x["pod_id"]: x for x in processes}

def process_cgroup_mem_current(cgroup_dir):
    return int(Locations.MEM_CURRENT(cgroup_dir).read())

def process_cgroup_pressure(cgroup_dir):
    pressure = Locations.MEM_PRESSURE(cgroup_dir).read().split('\n')
    return int(pressure[1].split()[4].split('=')[1]) # accumulated number of microseconds the whole process was delayed

def total_cgroupv2_usage(processes, pause_processes, shims_processes):
    main_cgroup_mem_current = 0
    pause_cgroup_mem_current = 0
    mem_pressure_full_total = 0

    main_cgroup_mem_max = 0

    for pod_id, process in processes.items():
        pause_process = pause_processes[pod_id]
        shim_process = shims_processes[pause_process["container_id"]]

        cgroup_mem = process_cgroup_mem_current(process["cgroup_dir"])
        main_cgroup_mem_current += cgroup_mem
        main_cgroup_mem_max = max(main_cgroup_mem_max, cgroup_mem)
        pause_cgroup_mem_current += process_cgroup_mem_current(pause_process["cgroup_dir"])

        pod_cgroup_dir = Path(process["cgroup_dir"]).parent

        mem_pressure_full_total += process_cgroup_pressure(pod_cgroup_dir)
    
    return {
        "nr_pods": len(processes),
        "main_cgroup_mem_current": main_cgroup_mem_current,
        "main_cgroup_mem_max": main_cgroup_mem_max,
        "pause_cgroup_mem_current": pause_cgroup_mem_current,
        "mem_pressure_full_total": mem_pressure_full_total,
    }

def update_memory_pressure(processes, max):
    for _, process in processes.items():
        pod_cgroup_dir = Path(process["cgroup_dir"]).parent
        Locations.MEM_HIGH(pod_cgroup_dir).write(str(max))

def get_total_memory():
    meminfo = Locations.MEMINFO.read().split('\n')
    total_memory = [x.removeprefix("MemTotal:").removesuffix("kB").strip() for x in meminfo if x.startswith("MemTotal:")][0]
    return int(total_memory) * 1000


def tuple_add(t):
    v_min, v_max = t
    diff = (v_max - v_min)
    return v_min + diff, v_max + diff

def tuple_half(t):
    v_min, v_max = t
    half = (v_max - v_min) // 2
    return v_min, v_max - half

class Modulator:
    def __init__(self) -> None:
        # config
        self.allowed_delta_level1 = 1_000
        self.allowed_delta_level2 = 10_000
        self.allowed_delta_level3 = 2000_000
        self.output_limits = (1, get_total_memory())
        
        # runtime vars
        self.counter = 0
        self.total_microseconds = None
        self.current_limit = None

    def update_runtime(self, results):
        total_microseconds = results["mem_pressure_full_total"]
        mem_usage = results["main_cgroup_mem_current"] + results["pause_cgroup_mem_current"]
        max_usage = results["main_cgroup_mem_max"]
        nr_pods = max(1, results["nr_pods"])

        if self.total_microseconds is None:
            self.total_microseconds = total_microseconds

        delta = total_microseconds - self.total_microseconds
        self.total_microseconds = total_microseconds
        return mem_usage, max_usage, delta, nr_pods

    def calculate_memory_max(self, results, nr_operators):
        mem_usage, max_usage, delta, nr_pods = self.update_runtime(results)

        if (delta > self.allowed_delta_level3) or (self.current_limit is None):
            self.counter = 0
            self.current_limit = max(1.5 * nr_pods * max_usage, nr_operators * 5*1024*1024)
        elif delta > self.allowed_delta_level2:
            self.counter = 0
            self.current_limit = 1.2 * nr_pods * max_usage
        elif delta > self.allowed_delta_level1:
            self.counter = max(self.counter + 1, 0)
            self.current_limit *= 1.02
        else:
            self.counter = min(self.counter - 1, 0)
            self.current_limit /= 1.02

        self.current_limit = max(self.output_limits[0], self.current_limit)
        self.current_limit = min(self.output_limits[1], self.current_limit)

        mem_max_per_pod = int(self.current_limit / nr_pods) + 1

        print(f'delta: {delta / 1000:.3f}ms\t mem: {h(mem_usage)}\t max: {h(self.current_limit)}\t max_per_pod: {h(mem_max_per_pod)}\t counter: {self.counter}')

        return mem_usage, self.current_limit, mem_max_per_pod

if __name__ == "__main__":
    import sys
    type, nr_operators, output_file = sys.argv[1], sys.argv[2], sys.argv[3]
    nr_operators = int(nr_operators)

    rust_pids = []
    go_pids = []
    wasm_pids = []

    with open(output_file, 'w') as file_object:
        file_object.write('time;mem_usage;mem_max;mem_max_per_pod\n')

    process_list = ProcessList()
    modulator = Modulator()

    while True:
        process_list.refresh()

        shim_processes = parse_shims(process_list.find_in("/usr/local/bin/containerd-shim-runc-v2"))
        pause_processes = parse_containers(process_list.find("/pause"))

        mem_usage, mem_max, mem_max_per_pod = None, None, None
        if type == "rust":
            rust_processes = parse_containers(process_list.find("/ring-rust-controller"))
            native_rust = total_cgroupv2_usage(rust_processes, pause_processes, shim_processes)
            mem_usage, mem_max, mem_max_per_pod = modulator.calculate_memory_max(native_rust, nr_operators)
            update_memory_pressure(rust_processes, mem_max_per_pod)
        elif type == "golang":
            go_processes = parse_containers(process_list.find("/ring-go-controller"))
            native_golang = total_cgroupv2_usage(go_processes, pause_processes, shim_processes)
            mem_usage, mem_max, mem_max_per_pod = modulator.calculate_memory_max(native_golang, nr_operators)
            update_memory_pressure(go_processes, mem_max_per_pod)
        elif type == "wasm":
            wasm_processes = parse_containers(process_list.find("/controller /"))
            wasm_rust = total_cgroupv2_usage(wasm_processes, pause_processes, shim_processes)
            mem_usage, mem_max, mem_max_per_pod = modulator.calculate_memory_max(wasm_rust, nr_operators)
            update_memory_pressure(wasm_processes, mem_max_per_pod)

        with open(output_file, 'a') as file_object:
            file_object.write(f'{time.time()};{mem_usage};{mem_max};{mem_max_per_pod}\n'.replace(".", ","))

        time.sleep(1)
