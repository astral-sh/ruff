# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Give other development workloads priority over this task's child processes."""

from __future__ import annotations

import argparse
import csv
import itertools
import json
import os
import signal
import statistics
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).parent
BINARIES = {
    "baseline": "main-baseline-final",
    "original": "main-original-final",
    "progressive": "main-progressive-final",
}
QUIET_SINCE = None


def emit(event, **fields):
    print(json.dumps({"time": time.time(), "event": event, **fields}), flush=True)


def competitors():
    global QUIET_SINCE
    result = subprocess.run(
        ["ps", "-axo", "pid=,ppid=,pcpu=,comm="],
        check=True,
        text=True,
        capture_output=True,
    )
    rows = [line.split(None, 3) for line in result.stdout.splitlines()]
    own = {os.getpid()}
    while True:
        expanded = own | {int(pid) for pid, parent, *_ in rows if int(parent) in own}
        if expanded == own:
            break
        own = expanded
    busy = []
    for pid, _parent, cpu, command in rows:
        if int(pid) in own:
            continue
        name = Path(command).name.lower()
        if (
            "benchmark" in name
            or name.startswith("ty_ide-")
            or name in {"rustc", "cargo", "cargo-nextest", "hyperfine", "samply"}
            or "/auto-import-" in command
        ):
            busy.append({"pid": int(pid), "cpu": float(cpu), "command": command})
    if busy:
        QUIET_SINCE = None
    elif QUIET_SINCE is None:
        QUIET_SINCE = time.monotonic()
    return busy


def await_quiet(seconds=10):
    previous = None
    while True:
        busy = competitors()
        if busy:
            identity = tuple(item["pid"] for item in busy)
            if identity != previous:
                emit("defer", competitors=busy)
                previous = identity
        else:
            if time.monotonic() - QUIET_SINCE >= seconds:
                return
        time.sleep(1)


def run_cooperatively(command, stdout, stderr, discard_on_interference):
    await_quiet()
    emit("start", command=command)
    child = subprocess.Popen(
        command, stdout=stdout, stderr=stderr, start_new_session=True
    )
    try:
        while child.poll() is None:
            busy = competitors()
            if not discard_on_interference:
                # Let simultaneous builds finish: suspending a compiler that owns
                # a shared cache lock could block the other session indefinitely.
                busy = [
                    item
                    for item in busy
                    if "benchmark" in Path(item["command"]).name
                    or "/auto-import-" in item["command"]
                ]
            if busy and child.poll() is None:
                if discard_on_interference:
                    emit("discard", competitors=busy, pid=child.pid)
                    os.killpg(child.pid, signal.SIGTERM)
                    try:
                        child.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(child.pid, signal.SIGKILL)
                        child.wait()
                    return False
                emit("pause", competitors=busy, pid=child.pid)
                os.killpg(child.pid, signal.SIGSTOP)
                await_quiet()
                emit("resume", pid=child.pid)
                os.killpg(child.pid, signal.SIGCONT)
            time.sleep(1)
        if child.returncode:
            raise subprocess.CalledProcessError(child.returncode, command)
        emit("complete", pid=child.pid)
        return True
    finally:
        if child.poll() is None:
            os.killpg(child.pid, signal.SIGCONT)
            os.killpg(child.pid, signal.SIGTERM)
            child.wait()


def compare(tag, samples, payload):
    for block, order in enumerate(itertools.permutations(BINARIES)):
        for layout in ("regular",):
            for name in order:
                destination = ROOT / f"{tag}-{layout}-{name}-{block}"
                if destination.with_suffix(".done").exists():
                    continue
                attempt = 0
                while True:
                    prefix = ROOT / f"{destination.name}-attempt{attempt}"
                    if prefix.with_suffix(".csv").exists():
                        attempt += 1
                        continue
                    command = [
                        "/usr/bin/time",
                        "-l",
                        str(ROOT / BINARIES[name]),
                        "--latency-experiment",
                        str(prefix),
                        str(samples),
                        str(payload),
                        layout,
                    ]
                    with prefix.with_suffix(".csv").open(
                        "w"
                    ) as out, prefix.with_suffix(".log").open("w") as err:
                        valid = run_cooperatively(command, out, err, True)
                    if valid:
                        rows = list(csv.DictReader(prefix.with_suffix(".csv").open()))
                        assert len(rows) == samples
                        # Keep every rejected attempt; publish only complete quiet runs.
                        for source in ROOT.glob(prefix.name + ".*"):
                            destination.with_suffix(source.suffix).write_bytes(
                                source.read_bytes()
                            )
                        # The memory report uses a hyphen suffix rather than an extension.
                        for source in ROOT.glob(prefix.name + "-*.json"):
                            suffix = source.name.removeprefix(prefix.name)
                            (ROOT / (destination.name + suffix)).write_bytes(
                                source.read_bytes()
                            )
                        destination.with_suffix(".done").write_text(str(prefix) + "\n")
                        values = {
                            key: statistics.median(float(row[key]) for row in rows)
                            for key in rows[0]
                            if key != "sample"
                        }
                        emit(
                            "result",
                            block=block,
                            layout=layout,
                            approach=name,
                            **values,
                        )
                        break
                    attempt += 1


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="mode", required=True)
    benchmark = sub.add_parser("compare")
    benchmark.add_argument("tag")
    benchmark.add_argument("samples", type=int)
    benchmark.add_argument("payload", type=int)
    command = sub.add_parser("run")
    command.add_argument("log_prefix", type=Path)
    command.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.mode == "compare":
        compare(args.tag, args.samples, args.payload)
    else:
        with args.log_prefix.with_suffix(".out").open(
            "w"
        ) as out, args.log_prefix.with_suffix(".err").open("w") as err:
            run_cooperatively(args.command, out, err, False)


if __name__ == "__main__":
    main()
