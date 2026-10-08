"""Resource measurements and paired command comparisons for native release builds."""

from __future__ import annotations

import contextlib
import hashlib
import json
import math
import os
import platform
import random
import statistics
import subprocess
import threading
import time
from pathlib import Path

import psutil


def save(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def host_context():
    try:
        frequency = psutil.cpu_freq()
    except (OSError, RuntimeError, NotImplementedError):
        frequency = None
    return {
        "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "platform": platform.uname()._asdict(),
        "logical_cpus": psutil.cpu_count(),
        "physical_cpus": psutil.cpu_count(logical=False),
        "memory": psutil.virtual_memory()._asdict(),
        "cpu_times": psutil.cpu_times()._asdict(),
        "cpu_frequency": frequency._asdict() if frequency else None,
    }


def measure_build(command, *, cwd, environment, output):
    output = Path(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    command = list(map(str, command))
    resource = None
    usage_before = None
    if os.name != "nt":
        import resource

        usage_before = resource.getrusage(resource.RUSAGE_CHILDREN)
    started = time.perf_counter()
    process = subprocess.Popen(
        command,
        cwd=cwd,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        errors="replace",
    )

    def copy_log():
        with output.with_suffix(".log").open("w", encoding="utf-8") as stream:
            for line in process.stdout:
                stream.write(line)
                stream.flush()
                print(line, end="", flush=True)

    reader = threading.Thread(target=copy_log)
    reader.start()
    root = psutil.Process(process.pid)
    peak_single = peak_sum = peak_working_set = peak_private = 0
    last_cpu = {}
    with output.with_suffix(".memory.jsonl").open("w", encoding="utf-8") as stream:
        while process.poll() is None:
            entries = []
            try:
                children = [root, *root.children(recursive=True)]
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                children = []
            for child in children:
                try:
                    with child.oneshot():
                        memory = child.memory_info()
                        cpu = child.cpu_times()
                        key = (child.pid, child.create_time())
                        last_cpu[key] = (cpu.user, cpu.system)
                        entries.append(
                            {
                                "pid": child.pid,
                                "created": key[1],
                                "name": child.name(),
                                "rss_bytes": memory.rss,
                                "user_seconds": cpu.user,
                                "system_seconds": cpu.system,
                                "private_bytes": getattr(memory, "private", None),
                            }
                        )
                    peak_single = max(peak_single, memory.rss)
                    peak_working_set = max(
                        peak_working_set, getattr(memory, "peak_wset", 0)
                    )
                    peak_private = max(
                        peak_private, getattr(memory, "peak_pagefile", 0)
                    )
                except (psutil.NoSuchProcess, psutil.AccessDenied):
                    continue
            total = sum(item["rss_bytes"] for item in entries)
            peak_sum = max(peak_sum, total)
            stream.write(
                json.dumps(
                    {
                        "elapsed_seconds": time.perf_counter() - started,
                        "sum_rss_bytes": total,
                        "processes": entries,
                    }
                )
                + "\n"
            )
            stream.flush()
            with contextlib.suppress(subprocess.TimeoutExpired):
                process.wait(timeout=0.5)
    returncode = process.wait()
    elapsed = time.perf_counter() - started
    usage_after = resource.getrusage(resource.RUSAGE_CHILDREN) if resource else None
    reader.join()
    result = {
        "command": command,
        "cwd": str(cwd),
        "elapsed_seconds": elapsed,
        "exit_code": returncode,
        "sampled_peak_process_rss_bytes": peak_single,
        "sampled_peak_sum_rss_bytes": peak_sum,
        "windows_observed_peak_working_set_bytes": peak_working_set or None,
        "windows_observed_peak_private_bytes": peak_private or None,
        "sampled_user_seconds": sum(value[0] for value in last_cpu.values()),
        "sampled_system_seconds": sum(value[1] for value in last_cpu.values()),
        "unix_user_seconds": usage_after.ru_utime - usage_before.ru_utime
        if resource
        else None,
        "unix_system_seconds": usage_after.ru_stime - usage_before.ru_stime
        if resource
        else None,
        "sampling_interval_seconds": 0.5,
        "rustflags": environment.get("RUSTFLAGS"),
        "codegen_units_override": environment.get(
            "CARGO_PROFILE_RELEASE_CODEGEN_UNITS"
        ),
        "notes": [
            "Sampled peaks and CPU totals can miss short-lived processes.",
            "Summed RSS double-counts shared pages; it is not unique physical memory.",
        ],
    }
    save(output, result)
    if returncode:
        raise subprocess.CalledProcessError(returncode, command)
    return result


def wait_for_idle(output, *, timeout_seconds=180):
    samples = []
    for _ in range(timeout_seconds):
        samples.append(psutil.cpu_percent(interval=1))
        if len(samples) >= 5 and max(samples[-5:]) < 10:
            result = {"idle": True, "cpu_busy_percent": samples, "host": host_context()}
            save(output, result)
            return result
    save(output, {"idle": False, "cpu_busy_percent": samples, "host": host_context()})
    raise RuntimeError("Runner did not become idle; no timing round started")


def percentile(values, fraction):
    ordered = sorted(values)
    position = (len(ordered) - 1) * fraction
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower)


def timing_summary(values):
    return {
        "median_seconds": statistics.median(values),
        "p10_seconds": percentile(values, 0.1),
        "p90_seconds": percentile(values, 0.9),
        "iqr_seconds": percentile(values, 0.75) - percentile(values, 0.25),
        "samples_seconds": values,
    }


def paired_summary(baseline, candidate):
    ratios = [
        math.log(second / first)
        for first, second in zip(baseline, candidate, strict=True)
    ]
    generator = random.Random(20261008)
    bootstraps = [
        statistics.median(generator.choices(ratios, k=len(ratios))) for _ in range(5000)
    ]
    return {
        "median_change_percent": 100 * math.expm1(statistics.median(ratios)),
        "ci95_percent": [
            100 * math.expm1(percentile(bootstraps, q)) for q in (0.025, 0.975)
        ],
        "method": "Median paired log ratio; 5000 paired bootstrap resamples, percentile interval. Multiple comparisons are not corrected.",
    }


def compare_commands(
    *,
    name,
    baseline,
    candidate,
    cwd,
    environment,
    output,
    repetitions=30,
    warmups=2,
    allowed_exit_codes=(0,),
    before_each=None,
    fingerprint=None,
):
    output = Path(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    commands = {
        "baseline": list(map(str, baseline)),
        "candidate": list(map(str, candidate)),
    }
    reference = None
    reference_set = False

    def invoke(label, timed):
        nonlocal reference, reference_set
        if before_each:
            before_each()
        started = time.perf_counter()
        completed = subprocess.run(
            commands[label], cwd=cwd, env=environment, capture_output=True, check=False
        )
        elapsed = time.perf_counter() - started
        if completed.returncode not in allowed_exit_codes:
            raise RuntimeError(
                f"{name} {label} exited {completed.returncode}: {completed.stderr.decode(errors='replace')}"
            )
        value = {
            "returncode": completed.returncode,
            "output": fingerprint(completed)
            if fingerprint
            else {
                "stdout_sha256": hashlib.sha256(completed.stdout).hexdigest(),
                "stderr_sha256": hashlib.sha256(completed.stderr).hexdigest(),
            },
        }
        if not reference_set:
            reference = value
            reference_set = True
            output.with_suffix(".stdout").write_bytes(completed.stdout)
            output.with_suffix(".stderr").write_bytes(completed.stderr)
        elif value != reference:
            save(
                output.with_suffix(".mismatch.json"),
                {
                    "name": name,
                    "binary": label,
                    "expected": reference,
                    "actual": value,
                    "timed": timed,
                },
            )
            raise RuntimeError(f"{name}: output mismatch for {label}")
        return elapsed

    for _ in range(warmups):
        for label in commands:
            invoke(label, False)
    samples = {label: [] for label in commands}
    with output.with_suffix(".samples.jsonl").open("w", encoding="utf-8") as stream:
        for trial in range(repetitions):
            order = (
                ("baseline", "candidate")
                if trial % 2 == 0
                else ("candidate", "baseline")
            )
            for label in order:
                elapsed = invoke(label, True)
                samples[label].append(elapsed)
                stream.write(
                    json.dumps(
                        {
                            "case": name,
                            "trial": trial,
                            "binary": label,
                            "seconds": elapsed,
                            "outputs_equal": True,
                        }
                    )
                    + "\n"
                )
                stream.flush()
    timings = {label: timing_summary(values) for label, values in samples.items()}
    result = {
        "name": name,
        "outputs_equal": True,
        "commands": commands,
        "repetitions": repetitions,
        "warmups": warmups,
        "timings": timings,
        "candidate_change_percent": 100
        * (
            timings["candidate"]["median_seconds"]
            / timings["baseline"]["median_seconds"]
            - 1
        ),
        "paired_change": paired_summary(samples["baseline"], samples["candidate"]),
        "fingerprint": reference,
    }
    save(output, result)
    return result
