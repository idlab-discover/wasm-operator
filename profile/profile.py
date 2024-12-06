#!/bin/python3
import os
import time
from datetime import datetime
from pathlib import Path


def h(x):
    """Translates a number of bytes to a human-readable string."""
    order = 0
    suffix = ["", "k", "M", "G", "T"]
    max_order = len(suffix) - 1
    while abs(x) > 1024 and order < max_order:
        x /= 1024.0
        order += 1
    return "%.2f%s" % (x, suffix[order])


def log(*args):
    """Logs timestamped information to stdout."""
    ts = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S.%f")
    print(ts, *args)


class Location(str):
    def __call__(self, *args) -> "Location":
        return Location(self.format(*args))

    def exists(self):
        return Path(self).exists()

    def read(self):
        with open(self, "r") as f:
            return f.read()

    def write(self, value):
        with open(self, "w") as f:
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
                    "cmdline": Locations.PROC_CMDLINE(pid)
                    .read()
                    .replace("\0", " ")
                    .strip(),
                }
            except Exception as e:
                print(f"Exception {e} occurred, but can safely be ignored")
                continue

    def find(self, name):
        return [process for process in self.pids.values() if name == process["cmdline"]]

    def find_in(self, name):
        return [process for process in self.pids.values() if name in process["cmdline"]]

    def __str__(self):
        return "\n".join(
            "pid: {}\t cmdline: {}".format(pid["pid"], pid["cmdline"])
            for pid in self.pids.values()
        )


def remove_prefix(text, prefix):
    if text.startswith(prefix):
        return text[len(prefix) :]
    return text


def parse_shims(shim_processes):
    for shim_process in shim_processes:
        if ("cgroup_dir" in shim_process) and ("container_id" in shim_process):
            continue

        cgroup_dir = Locations.PROC_CGROUP(shim_process["pid"]).read()
        cgroup_dir = [
            remove_prefix(x, "0::")
            for x in cgroup_dir.split("\n")
            if x.startswith("0::")
        ][0].strip()
        cgroup_dir = Locations.CGROUP_DIR(cgroup_dir)

        shim_process["cgroup_dir"] = cgroup_dir

    return {x["cmdline"].split(" ")[4]: x for x in shim_processes}


def parse_containers(processes):
    for process in processes:
        if ("cgroup_dir" in process) and ("container_id" in process):
            continue

        cgroup_dir = Locations.PROC_CGROUP(process["pid"]).read()
        cgroup_dir = [
            remove_prefix(x, "0::")
            for x in cgroup_dir.split("\n")
            if x.startswith("0::")
        ][0].strip()
        cgroup_dir = Locations.CGROUP_DIR(cgroup_dir)

        process["cgroup_dir"] = cgroup_dir
        process["container_id"] = (
            os.path.basename(cgroup_dir)
            .removeprefix("cri-containerd-")
            .removesuffix(".scope")
        )
        process["pod_id"] = os.path.basename(os.path.dirname(cgroup_dir))

    return {x["pod_id"]: x for x in processes}


def process_cgroup_mem_current(cgroup_dir):
    return int(Locations.MEM_CURRENT(cgroup_dir).read())


def process_cgroup_pressure(cgroup_dir):
    pressure = Locations.MEM_PRESSURE(cgroup_dir).read().split("\n")
    return int(
        pressure[1].split()[4].split("=")[1]
    )  # accumulated number of microseconds the whole process was delayed


def total_cgroupv2_usage(processes, pause_processes, shim_processes):
    main_cgroup_mem_current = 0
    pause_cgroup_mem_current = 0
    mem_pressure_full_total = 0

    main_cgroup_mem_max = 0

    for pod_id, process in processes.items():
        pause_process = pause_processes[pod_id]
        shim_process = shim_processes[pause_process["container_id"]]

        cgroup_mem = process_cgroup_mem_current(process["cgroup_dir"])
        main_cgroup_mem_current += cgroup_mem
        main_cgroup_mem_max = max(main_cgroup_mem_max, cgroup_mem)
        pause_cgroup_mem_current += process_cgroup_mem_current(
            pause_process["cgroup_dir"]
        )

        pod_cgroup_dir = Path(process["cgroup_dir"]).parent

        mem_pressure_full_total += process_cgroup_pressure(pod_cgroup_dir)

    # all containerd-releated processes run in the same cgroup (so only account once)
    containerd_mem = process_cgroup_mem_current(shim_process["cgroup_dir"])
    containerd_pressure = process_cgroup_pressure(shim_process["cgroup_dir"])
    # containerd_pressure = 0

    return {
        "app_mem": main_cgroup_mem_current,
        "app_max": main_cgroup_mem_max,
        "app_presssure": mem_pressure_full_total,
        "pause_mem": pause_cgroup_mem_current,
        "containerd_mem": containerd_mem,
        "containerd_pressure": containerd_pressure,
    }


def update_memory_pressure(processes, max):
    for _, process in processes.items():
        pod_cgroup_dir = Path(process["cgroup_dir"]).parent
        Locations.MEM_HIGH(pod_cgroup_dir).write(str(max))


def get_total_memory():
    meminfo = Locations.MEMINFO.read().split("\n")
    total_memory = [
        x.removeprefix("MemTotal:").removesuffix("kB").strip()
        for x in meminfo
        if x.startswith("MemTotal:")
    ][0]
    return int(total_memory) * 1000


class Modulator:
    def __init__(self) -> None:
        # config
        self.warmup_period = 20
        self.allowed_delta_level1 = 400
        self.allowed_delta_level2 = 10_000
        self.allowed_delta_level3 = 2000_000
        self.output_limits = (1, get_total_memory())

        # last metrics
        self.app_processes = []
        self.pause_processes = []
        self.shim_processes = []

        # runtime vars
        self.name = None
        self.counter = 0

        self.pause_mem = 0
        self.app_mem = 0
        self.app_max = 0
        self.app_delta = None
        self.app_total_microseconds = None
        self.app_limit = None

        self.containerd_mem = 0
        self.containerd_delta = None
        self.containerd_total_microseconds = None
        self.containerd_limit = None

    def update_runtime(self, name, app_processes, pause_processes, shim_processes):
        self.name = name
        self.app_processes, self.pause_processes, self.shim_processes = (
            app_processes,
            pause_processes,
            shim_processes,
        )
        results = total_cgroupv2_usage(
            self.app_processes, self.pause_processes, self.shim_processes
        )

        self.pause_mem = results["pause_mem"]
        self.app_mem = results["app_mem"]
        self.app_max = results["app_max"]
        app_total_microseconds = results["app_presssure"]
        self.app_delta = (
            0
            if (self.app_total_microseconds is None)
            else (app_total_microseconds - self.app_total_microseconds)
        )
        self.app_total_microseconds = app_total_microseconds

        self.containerd_mem = results["containerd_mem"]
        containerd_total_microseconds = results["containerd_pressure"]
        self.containerd_delta = (
            0
            if (self.containerd_total_microseconds is None)
            else (containerd_total_microseconds - self.containerd_total_microseconds)
        )
        self.containerd_total_microseconds = containerd_total_microseconds

    def delta_calculate(self, delta, limit, mem_current):
        if (limit is None) or (self.counter < self.warmup_period):
            limit = float("inf")

        elif self.counter == self.warmup_period + 1:
            limit = 1.2 * mem_current  # shock into full reclaim mode

        else:
            if delta > self.allowed_delta_level1:
                if self.name == "golang":
                    limit *= 1.02
                else:
                    limit *= 1.02

                if delta > self.allowed_delta_level3:
                    limit = max(limit, 1.3 * mem_current)
                elif delta > self.allowed_delta_level2:
                    limit = max(limit, 1.2 * mem_current)
            else:
                if self.name == "golang":
                    limit /= 1.02
                else:
                    limit /= 1.03

        return limit

    def clean_limit(self, limit):
        limit = max(self.output_limits[0], limit)
        limit = min(self.output_limits[1], limit)
        return limit

    def update_max(self):
        nr_pods = max(1, len(self.app_processes))

        if (self.app_limit is None) or (self.counter < self.warmup_period):
            if self.app_max < 1024:  # wait for usage to become more than 1KiB
                self.counter = 0

        # update app limit
        self.app_limit = self.delta_calculate(
            self.app_delta, self.app_limit, nr_pods * self.app_max
        )

        # update containerd limit
        self.containerd_limit = self.delta_calculate(
            self.containerd_delta, self.containerd_limit, self.containerd_mem
        )

        self.counter += 1

        self.app_limit = self.clean_limit(self.app_limit)
        mem_max_per_pod = int(self.app_limit / nr_pods) + 1
        # update_memory_pressure(self.app_processes, mem_max_per_pod)

        self.containerd_limit = int(self.clean_limit(self.containerd_limit)) + 1
        # update_memory_pressure({k: v for k,v in [next(iter(self.shim_processes.items()))]}, self.containerd_limit)
        curtime = datetime.now().strftime("%H:%M:%S")
        print(
            f"{curtime}s"
            + f"app_delta: {self.app_delta / 1000:.3f}ms\t"
            + f"app_mem: {h(self.app_mem)}\t"
            + f"app_max: {h(self.app_limit)}\t"
            + f"app_max_per_pod: {h(mem_max_per_pod)}\t"
            +
            # f'containerd_delta: {self.containerd_delta / 1000:.3f}ms\t' +
            # f'containerd_mem: {h(self.containerd_mem)}\t' +
            # f'containerd_max: {h(self.containerd_limit)}\t' +
            # f'pause_mem: {h(self.pause_mem)}\t' +
            f"counter: {self.counter}"
        )

        return (
            self.app_mem,
            self.app_limit,
            self.containerd_mem,
            self.containerd_limit,
            self.pause_mem,
        )


if __name__ == "__main__":
    import sys

    type, output_file = sys.argv[1], sys.argv[2]

    rust_pids = []
    go_pids = []
    wasm_pids = []

    with open(output_file, "w") as file_object:
        file_object.write(
            "time;app_mem;app_limit;containerd_mem;containerd_limit;pause_mem\n"
        )

    process_list = ProcessList()
    modulator = Modulator()

    while True:
        process_list.refresh()

        shim_processes = parse_shims(
            process_list.find_in("/usr/local/bin/containerd-shim-runc-v2")
        )
        pause_processes = parse_containers(process_list.find("/pause"))

        if type == "rust":
            modulator.update_runtime(
                "rust",
                parse_containers(process_list.find("/ring-rust-controller")),
                pause_processes,
                shim_processes,
            )
        elif type == "comb":
            modulator.update_runtime(
                "comb",
                parse_containers(process_list.find("/comb-rust-controller")),
                pause_processes,
                shim_processes,
            )
        elif type == "golang":
            modulator.update_runtime(
                "golang",
                parse_containers(process_list.find("/ring-go-controller")),
                pause_processes,
                shim_processes,
            )
        elif type == "wasm":
            modulator.update_runtime(
                "wasm",
                parse_containers(process_list.find("/controller /")),
                pause_processes,
                shim_processes,
            )

        app_mem, app_limit, containerd_mem, containerd_limit, pause_mem = (
            modulator.update_max()
        )

        with open(output_file, "a") as file_object:
            file_object.write(
                f"{datetime.now()};{app_mem};{app_limit};{containerd_mem};{containerd_limit};{pause_mem}\n".replace(
                    ".", ","
                )
            )

        time.sleep(0.1)
