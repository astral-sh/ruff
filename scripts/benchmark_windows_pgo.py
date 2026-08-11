"""Compare production Windows Ruff binaries on independent, pinned projects."""

# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import os
import platform
import re
import shutil
import statistics
import subprocess
import sys
import time
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True, slots=True)
class Project:
    name: str
    repository: str
    revision: str
    source: str

    def __post_init__(self) -> None:
        if re.fullmatch(r"[0-9a-f]{40}", self.revision) is None:
            raise ValueError(f"{self.repository} is not pinned to a full commit SHA")


PROJECTS = (
    Project(
        "django",
        "django/django",
        "e2a424605ac2e7e6e799496542fb2997207e2f23",
        "django",
    ),
    Project(
        "pandas",
        "pandas-dev/pandas",
        "300a0cd8d3539fc9ca8539fbffd31809cc2f1fa5",
        "pandas",
    ),
    Project(
        "scikit-learn",
        "scikit-learn/scikit-learn",
        "1074736921eecc3ba84743404696bdcaf877c023",
        "sklearn",
    ),
    Project(
        "scipy",
        "scipy/scipy",
        "e2f4f50d940839ad13b3fc3305550cb834bd4fe2",
        "scipy",
    ),
    Project(
        "sympy",
        "sympy/sympy",
        "b16eebb5e19bc6a8d1da48f97ff1c8b87217c5b3",
        "sympy",
    ),
)

LABELS = ("baseline", "pgo")
MODES = ("check", "format")


class FileTime(ctypes.Structure):
    _fields_ = [
        ("low", ctypes.c_uint32),
        ("high", ctypes.c_uint32),
    ]


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    for label in LABELS:
        parser.add_argument(f"--{label}", required=True, type=Path)
    parser.add_argument("--workspace", required=True, type=Path)
    parser.add_argument("--json-output", required=True, type=Path)
    parser.add_argument("--markdown-output", required=True, type=Path)
    parser.add_argument("--pairs", type=int, default=16)
    parser.add_argument("--warmups", type=int, default=2)
    args = parser.parse_args()
    if args.pairs < 8 or args.warmups < 1:
        parser.error("at least eight paired rounds and one warmup are required")
    return args


def command(arguments: list[str], *, environment: dict[str, str]) -> None:
    print(f"+ {' '.join(arguments)}", flush=True)
    subprocess.run(arguments, env=environment, check=True)


def prepare_projects(directory: Path) -> dict[str, Path]:
    directory.mkdir(parents=True, exist_ok=True)
    environment = os.environ | {
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_TERMINAL_PROMPT": "0",
        "GIT_LFS_SKIP_SMUDGE": "1",
    }
    checkouts: dict[str, Path] = {}
    for project in PROJECTS:
        checkout = directory / project.name
        checkout.mkdir(parents=True, exist_ok=True)
        git = [
            "git",
            "-c",
            f"core.hooksPath={os.devnull}",
            "-c",
            "core.longpaths=true",
            "-C",
            str(checkout),
        ]
        url = f"https://github.com/{project.repository}.git"
        if not (checkout / ".git").is_dir():
            command([*git, "init", "--quiet"], environment=environment)
            command([*git, "remote", "add", "origin", url], environment=environment)
        actual_url = subprocess.check_output(
            [*git, "config", "--local", "--get", "remote.origin.url"],
            text=True,
            env=environment,
        ).strip()
        if actual_url != url:
            raise RuntimeError(f"unexpected remote for {project.name}: {actual_url}")
        command(
            [*git, "sparse-checkout", "set", "--cone", project.source],
            environment=environment,
        )
        revision = subprocess.run(
            [*git, "rev-parse", "--verify", "HEAD"],
            capture_output=True,
            text=True,
            env=environment,
            check=False,
        )
        if revision.returncode or revision.stdout.strip() != project.revision:
            command(
                [
                    *git,
                    "fetch",
                    "--quiet",
                    "--no-tags",
                    "--no-recurse-submodules",
                    "--depth=1",
                    "--filter=blob:none",
                    "origin",
                    project.revision,
                ],
                environment=environment,
            )
        command(
            [
                *git,
                "checkout",
                "--quiet",
                "--detach",
                "--force",
                "--no-recurse-submodules",
                project.revision,
            ],
            environment=environment,
        )
        actual_revision = subprocess.check_output(
            [*git, "rev-parse", "HEAD"], text=True, env=environment
        ).strip()
        if actual_revision != project.revision:
            raise RuntimeError(
                f"unexpected revision for {project.name}: {actual_revision}"
            )
        if not (checkout / project.source).is_dir():
            raise RuntimeError(f"missing held-out sources: {checkout / project.source}")
        checkouts[project.name] = checkout
    return checkouts


def extract_binary(source: Path, destination: Path) -> Path:
    source = source.resolve(strict=True)
    if source.is_file() and source.suffix.lower() == ".exe":
        return source
    executables = list(source.rglob("ruff.exe")) if source.is_dir() else []
    if len(executables) == 1:
        return executables[0].resolve(strict=True)
    archives = list(source.rglob("ruff-x86_64-pc-windows-msvc.zip"))
    if len(archives) != 1:
        raise RuntimeError(
            f"expected one Windows release archive in {source}: {archives}"
        )
    with zipfile.ZipFile(archives[0]) as archive:
        members = [
            member
            for member in archive.infolist()
            if not member.is_dir() and Path(member.filename).name.lower() == "ruff.exe"
        ]
        if len(members) != 1:
            raise RuntimeError(f"expected one ruff.exe in {archives[0]}")
        destination.parent.mkdir(parents=True, exist_ok=True)
        with (
            archive.open(members[0]) as input_stream,
            destination.open("wb") as output_stream,
        ):
            shutil.copyfileobj(input_stream, output_stream)
    return destination.resolve(strict=True)


def kernel32() -> Any:
    library = ctypes.WinDLL("kernel32", use_last_error=True)  # type: ignore[attr-defined]
    library.GetCurrentProcess.restype = ctypes.c_void_p
    library.GetProcessAffinityMask.argtypes = [
        ctypes.c_void_p,
        ctypes.POINTER(ctypes.c_size_t),
        ctypes.POINTER(ctypes.c_size_t),
    ]
    library.SetProcessAffinityMask.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
    library.GetProcessTimes.argtypes = [
        ctypes.c_void_p,
        ctypes.POINTER(FileTime),
        ctypes.POINTER(FileTime),
        ctypes.POINTER(FileTime),
        ctypes.POINTER(FileTime),
    ]
    library.QueryProcessCycleTime.argtypes = [
        ctypes.c_void_p,
        ctypes.POINTER(ctypes.c_uint64),
    ]
    return library


def pin_process(library: Any) -> tuple[int, int]:
    process_mask = ctypes.c_size_t()
    system_mask = ctypes.c_size_t()
    current_process = library.GetCurrentProcess()
    if not library.GetProcessAffinityMask(
        current_process, ctypes.byref(process_mask), ctypes.byref(system_mask)
    ):
        raise ctypes.WinError(ctypes.get_last_error())  # type: ignore[attr-defined]
    available = process_mask.value & system_mask.value
    processors = [
        bit for bit in range(available.bit_length()) if available & (1 << bit)
    ][:8]
    selected = sum(1 << processor for processor in processors)
    if not selected or not library.SetProcessAffinityMask(current_process, selected):
        raise ctypes.WinError(ctypes.get_last_error())  # type: ignore[attr-defined]
    return selected, len(processors)


def process_usage(library: Any, process: subprocess.Popen[bytes]) -> dict[str, int]:
    created = FileTime()
    exited = FileTime()
    kernel = FileTime()
    user = FileTime()
    handle = ctypes.c_void_p(int(process._handle))  # type: ignore[attr-defined]
    if not library.GetProcessTimes(
        handle,
        ctypes.byref(created),
        ctypes.byref(exited),
        ctypes.byref(kernel),
        ctypes.byref(user),
    ):
        raise ctypes.WinError(ctypes.get_last_error())  # type: ignore[attr-defined]
    cycles = ctypes.c_uint64()
    if not library.QueryProcessCycleTime(handle, ctypes.byref(cycles)):
        raise ctypes.WinError(ctypes.get_last_error())  # type: ignore[attr-defined]
    user_ns = ((user.high << 32) | user.low) * 100
    kernel_ns = ((kernel.high << 32) | kernel.low) * 100
    return {
        "cpu_ns": user_ns + kernel_ns,
        "user_ns": user_ns,
        "kernel_ns": kernel_ns,
        "cpu_cycles": cycles.value,
    }


def evaluation_environment(parallelism: int) -> dict[str, str]:
    environment = os.environ.copy()
    for variable in (
        "LLVM_PROFILE_FILE",
        "RUFF_CACHE_DIR",
        "RUFF_OUTPUT_FILE",
        "RUFF_OUTPUT_FORMAT",
        "RUFF_TRACE",
        "RAYON_NUM_THREADS",
    ):
        environment.pop(variable, None)
    environment.update(
        {
            "NO_COLOR": "1",
            "PYTHONDONTWRITEBYTECODE": "1",
            "RAYON_NUM_THREADS": str(parallelism),
        }
    )
    return environment


def workload_arguments(
    project: Project, checkout: Path, mode: str, *, capture: bool
) -> list[str]:
    common = ["--isolated", "--target-version", "py314", "--no-cache"]
    if not capture:
        common.append("--silent")
    if mode == "check":
        options = ["--exit-zero"]
        if capture:
            options.extend(["--output-format", "concise"])
    elif mode == "format":
        options = ["--diff"] if capture else ["--check"]
    else:
        raise ValueError(f"unknown Ruff workload: {mode}")
    return [mode, *common, *options, str(checkout / project.source)]


def execute(
    library: Any,
    binary: Path,
    project: Project,
    checkout: Path,
    mode: str,
    parallelism: int,
    *,
    capture: bool = False,
) -> dict[str, Any]:
    started = time.perf_counter_ns()
    process = subprocess.Popen(
        [str(binary), *workload_arguments(project, checkout, mode, capture=capture)],
        cwd=checkout,
        env=evaluation_environment(parallelism),
        stdout=subprocess.PIPE if capture else subprocess.DEVNULL,
        stderr=subprocess.PIPE,
    )
    stdout, stderr = process.communicate(timeout=180)
    wall_ns = time.perf_counter_ns() - started
    allowed = {0} if mode == "check" else {0, 1}
    if process.returncode not in allowed:
        message = stderr.decode("utf-8", errors="replace")[:4000]
        raise RuntimeError(
            f"{project.name}/{mode} failed ({process.returncode}): {message}"
        )
    result: dict[str, Any] = process_usage(library, process)
    result["wall_ns"] = wall_ns
    result["returncode"] = process.returncode
    if capture:
        normalized = (stdout or b"").replace(b"\r\n", b"\n")
        normalized_stderr = stderr.replace(b"\r\n", b"\n")
        result["stdout_sha256"] = hashlib.sha256(normalized).hexdigest()
        result["stderr_sha256"] = hashlib.sha256(normalized_stderr).hexdigest()
        result["stdout_bytes"] = len(normalized)
        result["stdout_lines"] = len(normalized.splitlines())
    return result


def check_unchanged(project: Project, checkout: Path) -> None:
    status = subprocess.check_output(
        [
            "git",
            "--no-optional-locks",
            "-C",
            str(checkout),
            "status",
            "--porcelain",
            "--untracked-files=no",
            "--",
            project.source,
        ],
        text=True,
    )
    if status:
        raise RuntimeError(f"{project.name}: benchmark modified tracked source files")


def verify_equivalence(
    library: Any,
    binaries: dict[str, Path],
    project: Project,
    checkout: Path,
    mode: str,
    parallelism: int,
) -> dict[str, Any]:
    def run(threads: int) -> dict[str, dict[str, Any]]:
        return {
            label: execute(
                library,
                binary,
                project,
                checkout,
                mode,
                threads,
                capture=True,
            )
            for label, binary in binaries.items()
        }

    def signatures(outputs: dict[str, dict[str, Any]]) -> set[tuple[Any, ...]]:
        return {
            (sample["returncode"], sample["stdout_sha256"], sample["stderr_sha256"])
            for sample in outputs.values()
        }

    outputs = run(parallelism)
    verification_threads = parallelism
    if len(signatures(outputs)) != 1:
        print(f"Retrying {project.name}/{mode} equivalence single-threaded", flush=True)
        outputs = run(1)
        verification_threads = 1
    if len(signatures(outputs)) != 1:
        raise RuntimeError(f"{project.name}/{mode}: Ruff outputs are not equivalent")
    sample = outputs["baseline"]
    if not sample["stdout_bytes"]:
        raise RuntimeError(
            f"{project.name}/{mode}: semantic preflight emitted no output"
        )
    if mode == "format":
        check_unchanged(project, checkout)
    return {
        "returncode": sample["returncode"],
        "stdout_sha256": sample["stdout_sha256"],
        "stderr_sha256": sample["stderr_sha256"],
        "stdout_bytes": sample["stdout_bytes"],
        "stdout_lines": sample["stdout_lines"],
        "verification_threads": verification_threads,
    }


def summarize(samples: list[dict[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for label in LABELS:
        values = [sample[label] for sample in samples]
        result[label] = {
            "wall_median_ms": round(
                statistics.median(sample["wall_ns"] for sample in values) / 1e6, 4
            ),
            "cpu_median_ms": round(
                statistics.median(sample["cpu_ns"] for sample in values) / 1e6, 4
            ),
            "cycles_median": round(
                statistics.median(sample["cpu_cycles"] for sample in values)
            ),
        }
    for metric in ("wall_ns", "cpu_ns", "cpu_cycles"):
        ratios = [
            sample["pgo"][metric] / sample["baseline"][metric] for sample in samples
        ]
        result[f"{metric}_median_ratio"] = round(statistics.median(ratios), 8)
        result[f"{metric}_reduction_percent"] = round(
            100 * (1 - statistics.median(ratios)), 4
        )
    return result


def aggregates(report: dict[str, Any]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for mode in ("all", *MODES):
        workloads = [
            workload
            for workload in report.get("workloads", [])
            if mode == "all" or workload["mode"] == mode
        ]
        if not workloads:
            continue
        entry: dict[str, Any] = {"workloads": len(workloads)}
        for metric in ("wall_ns", "cpu_ns", "cpu_cycles"):
            ratio = statistics.geometric_mean(
                workload["summary"][f"{metric}_median_ratio"] for workload in workloads
            )
            entry[f"{metric}_ratio"] = round(ratio, 8)
            entry[f"{metric}_reduction_percent"] = round(100 * (1 - ratio), 4)
        result[mode] = entry
    return result


def markdown_report(report: dict[str, Any]) -> str:
    lines = [
        "# Windows x86-64 Ruff PGO benchmark",
        "",
        "Matched production executables, pinned held-out projects, exact lint "
        "diagnostic/formatted-output checks, and cache-disabled alternating paired runs.",
        "",
        "| Project | Mode | Baseline CPU | PGO CPU | CPU reduction | "
        "Wall reduction | Cycle reduction |",
        "| --- | --- | ---: | ---: | ---: | ---: | ---: |",
    ]
    for workload in report.get("workloads", []):
        summary = workload["summary"]
        lines.append(
            f"| {workload['project']} | {workload['mode']} | "
            f"{summary['baseline']['cpu_median_ms']:.2f} ms | "
            f"{summary['pgo']['cpu_median_ms']:.2f} ms | "
            f"{summary['cpu_ns_reduction_percent']:+.2f}% | "
            f"{summary['wall_ns_reduction_percent']:+.2f}% | "
            f"{summary['cpu_cycles_reduction_percent']:+.2f}% |"
        )
    for mode, aggregate in aggregates(report).items():
        label = {"all": "All workloads", "check": "Linting", "format": "Formatting"}[
            mode
        ]
        lines.append(
            f"| **{label} geomean** | — | — | — | "
            f"**{aggregate['cpu_ns_reduction_percent']:+.2f}%** | "
            f"**{aggregate['wall_ns_reduction_percent']:+.2f}%** | "
            f"**{aggregate['cpu_cycles_reduction_percent']:+.2f}%** |"
        )
    if error := report.get("error"):
        lines.extend(["", f"**Incomplete:** {error}"])
    lines.extend(
        [
            "",
            "Positive percentages mean PGO used less CPU time, elapsed time, "
            "or processor cycles. Source files were never modified.",
        ]
    )
    return "\n".join(lines) + "\n"


def checkpoint(
    report: dict[str, Any], json_output: Path, markdown_output: Path
) -> None:
    json_output.parent.mkdir(parents=True, exist_ok=True)
    markdown_output.parent.mkdir(parents=True, exist_ok=True)
    report["aggregate"] = aggregates(report)
    json_output.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    markdown_output.write_text(markdown_report(report), encoding="utf-8")


def main() -> None:
    args = arguments()
    if os.name != "nt":
        raise RuntimeError("this benchmark requires a native Windows runner")
    workspace = args.workspace.resolve()
    workspace.mkdir(parents=True, exist_ok=True)
    report: dict[str, Any] = {
        "platform": platform.platform(),
        "python": sys.version,
        "pairs": args.pairs,
        "warmups": args.warmups,
        "training_contamination": False,
        "source_mutations": False,
        "projects": {},
        "binaries": {},
        "workloads": [],
    }
    checkpoint(report, args.json_output, args.markdown_output)
    try:
        library = kernel32()
        affinity, parallelism = pin_process(library)
        report["cpu_affinity_mask"] = hex(affinity)
        report["rayon_threads"] = parallelism
        binaries: dict[str, Path] = {}
        for label in LABELS:
            binary = extract_binary(
                getattr(args, label), workspace / "executables" / label / "ruff.exe"
            )
            version = subprocess.check_output(
                [str(binary), "--version"], text=True
            ).strip()
            binaries[label] = binary
            report["binaries"][label] = {
                "path": str(binary),
                "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "bytes": binary.stat().st_size,
                "version": version,
            }
        if (
            report["binaries"]["baseline"]["version"]
            != report["binaries"]["pgo"]["version"]
        ):
            raise RuntimeError("baseline and PGO Ruff versions differ")
        checkouts = prepare_projects(workspace / "holdouts")
        for project in PROJECTS:
            package = checkouts[project.name] / project.source
            files = [
                path
                for path in package.rglob("*")
                if path.is_file() and path.suffix in {".py", ".pyi"}
            ]
            report["projects"][project.name] = {
                "repository": f"https://github.com/{project.repository}",
                "revision": project.revision,
                "source": project.source,
                "python_files": len(files),
                "python_bytes": sum(path.stat().st_size for path in files),
            }
        checkpoint(report, args.json_output, args.markdown_output)

        for project in PROJECTS:
            checkout = checkouts[project.name]
            for mode in MODES:
                print(
                    f"Validating {project.name}/{mode} output equivalence", flush=True
                )
                equivalence = verify_equivalence(
                    library, binaries, project, checkout, mode, parallelism
                )
                for index in range(args.warmups):
                    order = LABELS if index % 2 == 0 else tuple(reversed(LABELS))
                    for label in order:
                        execute(
                            library,
                            binaries[label],
                            project,
                            checkout,
                            mode,
                            parallelism,
                        )

                samples: list[dict[str, Any]] = []
                for index in range(args.pairs):
                    order = LABELS if index % 2 == 0 else tuple(reversed(LABELS))
                    sample: dict[str, Any] = {"round": index, "order": list(order)}
                    for label in order:
                        sample[label] = execute(
                            library,
                            binaries[label],
                            project,
                            checkout,
                            mode,
                            parallelism,
                        )
                    if sample["baseline"]["returncode"] != sample["pgo"]["returncode"]:
                        raise RuntimeError(
                            f"{project.name}/{mode}: timed exit codes differ"
                        )
                    samples.append(sample)
                if mode == "format":
                    check_unchanged(project, checkout)
                workload = {
                    "project": project.name,
                    "mode": mode,
                    "threads": parallelism,
                    "equivalence": equivalence,
                    "summary": summarize(samples),
                    "samples": samples,
                }
                report["workloads"].append(workload)
                checkpoint(report, args.json_output, args.markdown_output)
                summary = workload["summary"]
                print(
                    f"{project.name}/{mode}: CPU "
                    f"{summary['cpu_ns_reduction_percent']:+.2f}%, wall "
                    f"{summary['wall_ns_reduction_percent']:+.2f}%, cycles "
                    f"{summary['cpu_cycles_reduction_percent']:+.2f}%",
                    flush=True,
                )
        print(markdown_report(report), flush=True)
    except Exception as error:
        report["error"] = f"{type(error).__name__}: {error}"
        checkpoint(report, args.json_output, args.markdown_output)
        raise


if __name__ == "__main__":
    main()
