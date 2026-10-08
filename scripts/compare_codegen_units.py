"""Compare complete Ruff PGO pipelines and native runtime workloads."""

# /// script
# requires-python = ">=3.11"
# dependencies = ["psutil==7.0.0"]
# ///

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

import build_ruff_pgo as pipeline
from cgu_measurements import (
    compare_commands,
    host_context,
    measure_build,
    wait_for_idle,
)

SOURCE_REVISION = "7c6f8fc8fc5655dfa47e125b05453007b9e7a6c7"
CPYTHON_REVISION = "58ed60b7415e218ce3d608302e39b5e55bfb0e88"
REPOSITORY = Path(__file__).resolve().parent.parent


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def command_output(command: list[str], environment: dict[str, str]) -> str:
    return subprocess.run(
        command,
        cwd=REPOSITORY,
        env=environment,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def file_record(path: Path) -> dict:
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": digest(path)}


def prepare_corpora(
    root: Path, environment: dict[str, str]
) -> tuple[list[str], list[str]]:
    training = pipeline.ecosystem_python_files(root / "corpus", environment=environment)
    original_projects = pipeline.CORPUS_PROJECTS
    try:
        pipeline.CORPUS_PROJECTS = (
            pipeline.EcosystemProject(
                name="cpython",
                repository="python/cpython",
                revision=CPYTHON_REVISION,
                source_directories=("Lib",),
            ),
        )
        held_out = pipeline.ecosystem_python_files(
            root / "held-out", environment=environment
        )
    finally:
        pipeline.CORPUS_PROJECTS = original_projects
    files = [*training, *held_out]
    write_json(
        root / "evidence" / "corpus.json",
        {
            "training_projects": [
                {"name": project.name, "revision": project.revision}
                for project in original_projects
            ],
            "held_out_cpython_revision": CPYTHON_REVISION,
            "training_count": len(training),
            "held_out_count": len(held_out),
            "files": {
                str(Path(path).relative_to(root)): digest(Path(path)) for path in files
            },
        },
    )
    return training, held_out


def build_binaries(
    root: Path, environment: dict[str, str], training: list[str]
) -> dict:
    target = environment["UV_CGU_TARGET"]
    binary_name = "ruff.exe" if "windows" in target else "ruff"
    original_run = pipeline.run
    original_prepare = pipeline.ecosystem_python_files
    original_environment = os.environ.copy()
    original_arguments = sys.argv[:]
    results = {}
    try:
        # Both builds train on exactly the same immutable files and argument order.
        pipeline.ecosystem_python_files = lambda *args, **kwargs: training
        for units in (16, 1):
            directory = root / f"cgu{units}"
            directory.mkdir(parents=True, exist_ok=True)
            stages = []

            def run(
                command,
                *,
                environment,
                allowed_exit_codes=(0,),
                units=units,
                stages=stages,
            ):
                if command[:2] == ["cargo", "rustc"]:
                    stage = "instrumented" if not stages else "profile-use"
                    result = measure_build(
                        command,
                        cwd=REPOSITORY,
                        environment=environment,
                        output=root / "evidence" / f"cgu{units}-{stage}.json",
                    )
                    stages.append(result)
                else:
                    original_run(
                        command,
                        environment=environment,
                        allowed_exit_codes=allowed_exit_codes,
                    )

            pipeline.run = run
            os.environ.clear()
            os.environ.update(
                environment | {"CARGO_PROFILE_RELEASE_CODEGEN_UNITS": str(units)}
            )
            sys.argv = [
                str(Path(pipeline.__file__)),
                "--target",
                target,
                "--target-dir",
                str(directory),
            ]
            pipeline.main()
            if len(stages) != 2:
                raise RuntimeError(f"Expected two PGO build stages, got {len(stages)}")
            destination = root / "binaries" / f"cgu{units}" / binary_name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(directory / target / "release" / binary_name, destination)
            results[str(units)] = {
                "stages": stages,
                "instrumented_binary": file_record(
                    directory / "instrumented" / target / "release" / binary_name
                ),
                "binary": file_record(destination),
                "version": command_output([str(destination), "--version"], environment),
            }
            write_json(root / "evidence" / "builds.json", results)
    finally:
        pipeline.run = original_run
        pipeline.ecosystem_python_files = original_prepare
        os.environ.clear()
        os.environ.update(original_environment)
        sys.argv = original_arguments
    if results["16"]["version"] != results["1"]["version"]:
        raise RuntimeError("The binary versions differ")
    return results


def argument_file(path: Path, files: list[str]) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(files) + "\n", encoding="utf-8", newline="\n")
    return f"@{path}"


def formatted_files(
    root: Path, binaries: dict[str, Path], files: list[str], environment: dict[str, str]
) -> None:
    working = root / "formatted"
    reference = None
    records = {}
    for units, binary in binaries.items():
        if working.exists():
            shutil.rmtree(working)
        working.mkdir()
        paths = []
        for path in files:
            source = Path(path)
            destination = working / source.relative_to(root)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
            paths.append(str(destination))
        arguments = argument_file(root / "formatted.args", paths)
        completed = subprocess.run(
            [
                str(binary),
                "format",
                "--isolated",
                "--target-version",
                "py314",
                "--no-cache",
                arguments,
            ],
            cwd=REPOSITORY,
            env=environment,
            check=False,
            capture_output=True,
        )
        fingerprint = {
            "returncode": completed.returncode,
            "stdout_sha256": hashlib.sha256(completed.stdout).hexdigest(),
            "stderr_sha256": hashlib.sha256(completed.stderr).hexdigest(),
            "files": {
                str(Path(path).relative_to(working)): digest(Path(path))
                for path in paths
            },
        }
        records[units] = fingerprint
        write_json(root / "evidence" / "formatted-files.json", records)
        if completed.returncode != 0:
            raise RuntimeError(
                f"Formatter failed: {completed.stderr.decode(errors='replace')}"
            )
        if reference is not None and fingerprint != reference:
            raise RuntimeError("The two binaries produced different formatted files")
        reference = fingerprint


def runtime(
    root: Path,
    environment: dict[str, str],
    training: list[str],
    held_out: list[str],
    repetitions: int,
) -> None:
    target = environment["UV_CGU_TARGET"]
    binary_name = "ruff.exe" if "windows" in target else "ruff"
    binaries = {
        str(units): root / "binaries" / f"cgu{units}" / binary_name for units in (16, 1)
    }
    metadata = json.loads((root / "evidence" / "builds.json").read_text())
    for units, binary in binaries.items():
        if digest(binary) != metadata[units]["binary"]["sha256"]:
            raise RuntimeError(f"Binary hash mismatch: {binary}")
        binary.chmod(binary.stat().st_mode | 0o111)
    arguments = {}
    for name in ("pytest", "pip", "astropy"):
        subset = [
            path
            for path in training
            if Path(path).is_relative_to(root / "corpus" / name)
        ]
        arguments[name] = argument_file(root / "arguments" / f"{name}.args", subset)
    arguments["corpus"] = argument_file(root / "arguments" / "corpus.args", training)
    arguments["cpython"] = argument_file(root / "arguments" / "cpython.args", held_out)
    formatted_files(root, binaries, [*training, *held_out], environment)
    cases = [
        ("startup-version", ["--version"], {}, (0,)),
        ("startup-help", ["--help"], {}, (0,)),
    ]
    for project in ("pytest", "pip", "astropy", "corpus", "cpython"):
        for mode in ("check", "format"):
            flags = ["--output-format", "json"] if mode == "check" else ["--check"]
            cases.append(
                (
                    f"{project}-{mode}",
                    [
                        mode,
                        "--isolated",
                        "--target-version",
                        "py314",
                        "--no-cache",
                        *flags,
                        arguments[project],
                    ],
                    {},
                    (0, 1),
                )
            )
    for mode in ("check", "format"):
        flags = ["--output-format", "json"] if mode == "check" else ["--check"]
        cases.append(
            (
                f"cpython-{mode}-one-thread",
                [
                    mode,
                    "--isolated",
                    "--target-version",
                    "py314",
                    "--no-cache",
                    *flags,
                    arguments["cpython"],
                ],
                {"RAYON_NUM_THREADS": "1"},
                (0, 1),
            )
        )
        cases.append(
            (
                f"corpus-{mode}-warm-cache",
                [
                    mode,
                    "--isolated",
                    "--target-version",
                    "py314",
                    "--cache-dir",
                    str(root / "runtime-cache" / mode),
                    *flags,
                    arguments["corpus"],
                ],
                {},
                (0, 1),
            )
        )
    comparisons = {}
    for round_number in (1, 2):
        idle = wait_for_idle(root / "evidence" / f"idle-runtime-{round_number}.json")
        write_json(
            root / "evidence" / f"host-runtime-{round_number}.json",
            {"host": host_context(), "idle": idle},
        )
        for name, flags, overrides, allowed in cases:
            print(f"Runtime round {round_number}: {name}", flush=True)
            result = compare_commands(
                name=name,
                baseline=[str(binaries["16"]), *flags],
                candidate=[str(binaries["1"]), *flags],
                cwd=REPOSITORY,
                environment=environment | overrides,
                output=root / "evidence" / f"runtime-{round_number}-{name}.json",
                repetitions=repetitions,
                warmups=2,
                allowed_exit_codes=allowed,
            )
            comparisons[f"round-{round_number}/{name}"] = result
            write_json(root / "evidence" / "runtime.json", comparisons)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-only", action="store_true")
    parser.add_argument("--repetitions", type=int, default=30)
    args = parser.parse_args()
    root = Path(os.environ["UV_CGU_ROOT"]).resolve()
    root.mkdir(parents=True, exist_ok=True)
    target = os.environ["UV_CGU_TARGET"]
    if pipeline.rustc_host() != target:
        raise RuntimeError(f"Native host does not match requested target {target}")
    environment = os.environ.copy()
    for key in (
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "CARGO_ENCODED_RUSTFLAGS",
        "LLVM_PROFILE_FILE",
        "RAYON_NUM_THREADS",
    ):
        environment.pop(key, None)
    environment.update(
        {
            "CARGO_INCREMENTAL": "0",
            "CARGO_BUILD_JOBS": str(os.cpu_count()),
            "RUSTUP_TOOLCHAIN": "1.99.0",
            "NO_COLOR": "1",
            "TERM": "dumb",
        }
    )
    if target == "aarch64-apple-darwin":
        environment["RUSTFLAGS"] = (
            "-C linker=rust-lld -C linker-flavor=ld64.lld -C link-arg=--icf=safe"
        )
        environment["MACOSX_DEPLOYMENT_TARGET"] = "11.0"
    elif target == "x86_64-pc-windows-msvc":
        environment["RUSTFLAGS"] = "-C target-feature=+crt-static"
    elif target == "x86_64-unknown-linux-gnu":
        environment["RUSTFLAGS"] = ""
    else:
        raise RuntimeError(f"Unsupported native target: {target}")
    write_json(
        root / "evidence" / "context.json",
        {
            "source_revision": SOURCE_REVISION,
            "experiment_revision": command_output(
                ["git", "rev-parse", "HEAD"], environment
            ),
            "host": host_context(),
            "target": target,
            "rustc": command_output(["rustc", "-vV"], environment),
            "cargo": command_output(["cargo", "-vV"], environment),
            "rustflags": environment["RUSTFLAGS"],
            "notes": [
                "One clean complete PGO pipeline per configuration; order 16 then 1 on the same runner.",
                "Cargo downloads and corpus preparation occur before build timing; OS page-cache order effects remain possible.",
                "Existing parser, AST, and Salsa codegen-unit overrides remain at 1 in both configurations.",
                "Production PGO script and native release flags; Linux final build uses host GNU environment rather than the packaged manylinux container.",
            ],
        },
    )
    training, held_out = prepare_corpora(root, environment)
    if not args.runtime_only:
        subprocess.run(
            ["cargo", "fetch", "--locked", "--target", target],
            cwd=REPOSITORY,
            env=environment,
            check=True,
        )
        build_binaries(root, environment, training)
    runtime(root, environment, training, held_out, args.repetitions)


if __name__ == "__main__":
    main()
