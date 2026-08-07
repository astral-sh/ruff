#!/usr/bin/env python3
"""Build Ruff with profile-guided optimization using tracked repository files."""

from __future__ import annotations

import argparse
import os
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
CORPUS_DIRECTORIES = (
    "crates/ruff_benchmark/resources",
    "scripts",
    "python/ruff-ecosystem/ruff_ecosystem",
)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", help="Host-native Rust target triple")
    parser.add_argument(
        "--target-dir",
        type=Path,
        help="Cargo target directory (default: CARGO_TARGET_DIR or target/ruff-pgo)",
    )
    parser.add_argument(
        "--profile-dir",
        type=Path,
        help="Raw profile directory (default: <target-dir>/profiles)",
    )
    parser.add_argument(
        "--llvm-profdata",
        type=Path,
        help="Override the active Rust toolchain's llvm-profdata executable",
    )
    parser.add_argument(
        "--train-only",
        action="store_true",
        help="Only produce <target-dir>/ruff.profdata for a subsequent release build",
    )
    args = parser.parse_args()

    host = rustc_host()
    target = args.target or host
    if target != host:
        parser.error(
            f"PGO training requires the host-native target {host}, got {target}"
        )

    target_dir = (
        args.target_dir
        or Path(
            os.environ.get("CARGO_TARGET_DIR", REPOSITORY_ROOT / "target" / "ruff-pgo")
        )
    ).resolve()
    profile_dir = (args.profile_dir or target_dir / "profiles").resolve()
    merged_profile = target_dir / "ruff.profdata"
    profiler = find_llvm_profdata(host, args.llvm_profdata)
    corpus = tracked_python_files()

    profile_dir.mkdir(parents=True, exist_ok=True)
    for profile in profile_dir.glob("ruff-*.profraw"):
        profile.unlink()

    environment = os.environ.copy()
    environment["CARGO_INCREMENTAL"] = "0"
    if target.endswith("-apple-darwin"):
        for variable in ("CFLAGS", "CXXFLAGS"):
            environment[variable] = append_flags(
                environment.get(variable), "-fno-profile-generate -fno-profile-use"
            )

    instrumented_target_dir = target_dir / "instrumented"
    instrumented_environment = environment | {
        "CARGO_TARGET_DIR": str(instrumented_target_dir),
        "RUSTFLAGS": append_flags(
            environment.get("RUSTFLAGS"), f"-Cprofile-generate={profile_dir}"
        ),
    }
    print("Building instrumented release Ruff", flush=True)
    run(cargo_command(target), environment=instrumented_environment)

    binary_name = "ruff.exe" if "windows" in target else "ruff"
    instrumented_binary = instrumented_target_dir / target / "release" / binary_name
    if not instrumented_binary.is_file():
        raise RuntimeError(f"Instrumented Ruff binary not found: {instrumented_binary}")

    training_environment = instrumented_environment | {
        "LLVM_PROFILE_FILE": str(profile_dir / "ruff-%m-%p.profraw")
    }
    common_arguments = [
        "--isolated",
        "--target-version",
        "py314",
        "--no-cache",
        "--silent",
    ]
    print(f"Training on {len(corpus)} tracked Python files", flush=True)
    run(
        [str(instrumented_binary), "check", *common_arguments, "--exit-zero", *corpus],
        environment=training_environment,
    )
    run(
        [
            str(instrumented_binary),
            "format",
            *common_arguments,
            "--check",
            *corpus,
        ],
        environment=training_environment,
        allowed_exit_codes=(0, 1),
    )

    profiles = sorted(profile_dir.glob("ruff-*.profraw"))
    if not profiles or any(profile.stat().st_size == 0 for profile in profiles):
        raise RuntimeError(f"No complete Ruff profiling data found in {profile_dir}")

    with tempfile.NamedTemporaryFile(
        dir=target_dir, prefix="ruff-", suffix=".profdata", delete=False
    ) as temporary_file:
        temporary_profile = Path(temporary_file.name)
    try:
        run(
            [
                str(profiler),
                "merge",
                "--output",
                str(temporary_profile),
                *map(str, profiles),
            ],
            environment=environment,
        )
        temporary_profile.replace(merged_profile)
    finally:
        temporary_profile.unlink(missing_ok=True)
    print(f"Merged PGO profile: {merged_profile}", flush=True)

    if args.train_only:
        return

    optimized_environment = environment | {
        "CARGO_TARGET_DIR": str(target_dir),
        "RUSTFLAGS": append_flags(
            environment.get("RUSTFLAGS"), f"-Cprofile-use={merged_profile}"
        ),
    }
    print("Building optimized release Ruff", flush=True)
    run(cargo_command(target), environment=optimized_environment)
    print(
        f"Optimized Ruff: {target_dir / target / 'release' / binary_name}", flush=True
    )


def rustc_host() -> str:
    version = subprocess.run(
        ["rustc", "--version", "--verbose"],
        cwd=REPOSITORY_ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    for line in version.splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ")
    raise RuntimeError("Could not determine the active Rust compiler's host target")


def find_llvm_profdata(host: str, override: Path | None) -> Path:
    if override is not None:
        profiler = override.resolve()
    else:
        sysroot = subprocess.run(
            ["rustc", "--print", "sysroot"],
            cwd=REPOSITORY_ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        binary_name = "llvm-profdata.exe" if "windows" in host else "llvm-profdata"
        profiler = Path(sysroot) / "lib" / "rustlib" / host / "bin" / binary_name

    if not profiler.is_file() or not os.access(profiler, os.X_OK):
        raise RuntimeError(
            f"Rust toolchain llvm-profdata not found: {profiler}; "
            "run `rustup component add llvm-tools-preview`"
        )
    return profiler


def tracked_python_files() -> list[str]:
    tracked_files = subprocess.run(
        ["git", "ls-files", "-z", "--", *CORPUS_DIRECTORIES],
        cwd=REPOSITORY_ROOT,
        check=True,
        capture_output=True,
    ).stdout.split(b"\0")
    paths = [
        os.fsdecode(path)
        for path in tracked_files
        if path and Path(os.fsdecode(path)).suffix in {".py", ".pyi"}
    ]
    if not paths:
        raise RuntimeError("No tracked Python files found in the Ruff training corpus")
    return paths


def cargo_command(target: str) -> list[str]:
    return [
        "cargo",
        "rustc",
        "--release",
        "--locked",
        "--package",
        "ruff",
        "--bin",
        "ruff",
        "--target",
        target,
        "--",
        "-C",
        "strip=symbols",
    ]


def append_flags(existing: str | None, additional: str) -> str:
    return " ".join(flag for flag in (existing, additional) if flag)


def run(
    command: list[str],
    *,
    environment: dict[str, str],
    allowed_exit_codes: tuple[int, ...] = (0,),
) -> None:
    print(f"> {shlex.join(command)}", flush=True)
    completed = subprocess.run(
        command, cwd=REPOSITORY_ROOT, env=environment, check=False
    )
    if completed.returncode not in allowed_exit_codes:
        raise subprocess.CalledProcessError(completed.returncode, command)


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
