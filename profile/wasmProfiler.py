import math
import os
import time
from datetime import datetime
from pathlib import Path

import psutil


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


# func found on https://sleeplessbeastie.eu/2019/06/10/how-to-display-memory-used-by-processes-in-human-readable-form-using-python-script/
def pretty(nbytes):
    metric = ("B", "kB", "MB", "GB", "TB")
    if nbytes == 0:
        return "%s %s" % ("0", "B")
    nunit = int(math.floor(math.log(nbytes, 1024)))
    nsize = round(nbytes / (math.pow(1024, nunit)), 2)
    return "%s %s" % (format(nsize, ".2f"), metric[nunit])


if __name__ == "__main__":
    process_list = ProcessList()
    process_list.refresh()
    container = process_list.find("/controller /")

    import sys

    output_file = sys.argv[1]

    with open(output_file, "w") as file_object:
        file_object.write("time;uss;pss\n")

    for i in range(100000000000000):
        p = psutil.Process(container[0]["pid"])
        meminfo = p.memory_full_info()
        uss = meminfo.uss
        pss = meminfo.pss

        print(
            f"uss {pretty(p.memory_full_info().uss)}, pss={pretty(p.memory_full_info().pss)} "
        )
        with open(output_file, "a") as file_object:
            file_object.write(f"{datetime.now()};{uss};{pss}\n".replace(".", ","))

        time.sleep(0.2)
