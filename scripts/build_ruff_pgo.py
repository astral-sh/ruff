"""Build Ruff with profile-guided optimization using pinned ecosystem projects."""

# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import argparse
import os
import re
import shlex
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
EXCLUDED_DIRECTORIES = frozenset({"_tests", "_vendor", "test", "tests"})


@dataclass(frozen=True, slots=True)
class EcosystemProject:
    name: str
    repository: str
    revision: str
    source_directories: tuple[str, ...]

    def __post_init__(self) -> None:
        if re.fullmatch(r"[0-9a-f]{40}", self.revision) is None:
            raise ValueError(
                f"{self.repository} must be pinned to a full Git commit SHA, "
                f"got {self.revision!r}"
            )

    @property
    def url(self) -> str:
        return f"https://github.com/{self.repository}.git"


# Train on a subset of the pinned ecosystem projects that we already use for
# linting, formatting, or type checking. The goal is to create a representative
# corpus that includes scientific computing, synchronous and asynchronous code,
# applications, libraries, and type stubs.
#
# But it wasn't a highly optimized selection process. (For example, during
# development, we added Zulip and Warehouse, which reduced Ruff's CPU time by
# 0.35% while increasing its wheel size by 0.44%.)
CORPUS_PROJECTS = (
    EcosystemProject(
        name="pytest",
        repository="pytest-dev/pytest",
        revision="28e86a6c2ae0173831e4925a4af89b02a2936d09",
        source_directories=("src/_pytest",),
    ),
    EcosystemProject(
        name="httpx",
        repository="encode/httpx",
        revision="b5addb64f0161ff6bfe94c124ef76f6a1fba5254",
        source_directories=("httpx",),
    ),
    EcosystemProject(
        name="fastapi",
        repository="fastapi/fastapi",
        revision="a375f6b948b99fa4260129856bbf11d037f363ef",
        source_directories=("fastapi",),
    ),
    EcosystemProject(
        name="anyio",
        repository="agronholm/anyio",
        revision="ffe91331adb912c5d150f5d373f7cd28a0e96a62",
        source_directories=("src/anyio",),
    ),
    EcosystemProject(
        name="zulip",
        repository="zulip/zulip",
        revision="ccddbba7a3074283ccaac3bde35fd32b19faf042",
        source_directories=("zerver/views", "zerver/models"),
    ),
    EcosystemProject(
        name="warehouse",
        repository="pypi/warehouse",
        revision="5a4d2cadec641b5d6a6847d0127940e0f532f184",
        source_directories=(
            "warehouse/accounts",
            "warehouse/oidc",
            "warehouse/forklift",
        ),
    ),
    EcosystemProject(
        name="pip",
        repository="pypa/pip",
        revision="d1fd55753405fd728a0751a578e27c1054acdf48",
        source_directories=("src/pip/_internal",),
    ),
    EcosystemProject(
        name="sphinx",
        repository="sphinx-doc/sphinx",
        revision="b06d92e80eed130e1dd4e67cac4afa1267424f1a",
        source_directories=(
            "sphinx/builders",
            "sphinx/ext/autodoc",
            "sphinx/domains/python",
        ),
    ),
    EcosystemProject(
        name="astropy",
        repository="astropy/astropy",
        revision="b779108c7cec25c840c0f744fdf2a1550441e309",
        source_directories=("astropy/units",),
    ),
    EcosystemProject(
        name="typeshed",
        repository="python/typeshed",
        revision="e0efbeef901e9b6998d016e1ab9352678f09ae77",
        source_directories=(
            "stdlib/asyncio",
            "stdlib/collections",
            "stubs/requests",
        ),
    ),
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
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--train-only",
        action="store_true",
        help="Only produce <target-dir>/ruff.profdata for a subsequent release build",
    )
    mode.add_argument(
        "--prepare-corpus",
        action="store_true",
        help="Only download and prepare the pinned ecosystem training corpus",
    )
    args = parser.parse_args()

    target_dir = (
        args.target_dir
        or Path(
            os.environ.get("CARGO_TARGET_DIR", REPOSITORY_ROOT / "target" / "ruff-pgo")
        )
    ).resolve()
    profile_dir = (args.profile_dir or target_dir / "profiles").resolve()
    merged_profile = target_dir / "ruff.profdata"

    environment = os.environ.copy()
    if args.prepare_corpus:
        corpus = ecosystem_python_files(target_dir / "corpus", environment=environment)
        write_corpus_arguments(target_dir, corpus)
        print(f"Prepared {len(corpus)} ecosystem Python files", flush=True)
        return

    host = rustc_host()
    target = args.target or host
    if target != host:
        parser.error(
            f"PGO training requires the host-native target {host}, got {target}"
        )

    profiler = find_llvm_profdata(host, args.llvm_profdata)
    corpus = ecosystem_python_files(target_dir / "corpus", environment=environment)
    corpus_arguments = write_corpus_arguments(target_dir, corpus)

    profile_dir.mkdir(parents=True, exist_ok=True)
    for profile in profile_dir.glob("ruff-*.profraw"):
        profile.unlink()

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

    profiles = train_ruff(
        instrumented_binary,
        corpus_arguments,
        profile_dir,
        corpus_size=len(corpus),
        environment=instrumented_environment,
    )
    merge_profiles(profiler, profiles, merged_profile, environment=environment)

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


def train_ruff(
    binary: Path,
    corpus_arguments: Path,
    profile_directory: Path,
    *,
    corpus_size: int,
    environment: dict[str, str],
) -> list[Path]:
    common_arguments = [
        "--isolated",
        "--target-version",
        "py314",
        "--no-cache",
        "--silent",
    ]
    workloads = (
        ("check", "--exit-zero", (0,)),
        ("format", "--check", (0, 1)),
    )
    print(f"Training on {corpus_size} ecosystem Python files", flush=True)
    profiles = []

    for mode, mode_argument, allowed_exit_codes in workloads:
        run(
            [
                str(binary),
                mode,
                *common_arguments,
                mode_argument,
                f"@{corpus_arguments}",
            ],
            environment=environment
            | {
                "LLVM_PROFILE_FILE": str(
                    profile_directory / f"ruff-{mode}-%m-%p.profraw"
                )
            },
            allowed_exit_codes=allowed_exit_codes,
        )

        workload_profiles = sorted(profile_directory.glob(f"ruff-{mode}-*.profraw"))
        if not workload_profiles or any(
            profile.stat().st_size == 0 for profile in workload_profiles
        ):
            raise RuntimeError(
                f"No complete Ruff {mode} profiling data found in {profile_directory}"
            )
        profiles.extend(workload_profiles)

    return profiles


def merge_profiles(
    profiler: Path,
    profiles: list[Path],
    destination: Path,
    *,
    environment: dict[str, str],
) -> None:
    profile_size = sum(profile.stat().st_size for profile in profiles)

    with tempfile.NamedTemporaryFile(
        dir=destination.parent, prefix="ruff-", suffix=".profdata", delete=False
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
        temporary_profile.replace(destination)
    finally:
        temporary_profile.unlink(missing_ok=True)
    print(
        f"Merged {len(profiles)} PGO profiles ({profile_size:,} bytes): {destination}",
        flush=True,
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


def ecosystem_python_files(
    corpus_directory: Path, *, environment: dict[str, str]
) -> list[str]:
    corpus_directory.mkdir(parents=True, exist_ok=True)
    git_environment = environment | {
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_TERMINAL_PROMPT": "0",
        "GIT_LFS_SKIP_SMUDGE": "1",
    }
    paths: list[str] = []

    for project in CORPUS_PROJECTS:
        checkout = corpus_directory / project.name
        checkout.mkdir(parents=True, exist_ok=True)
        git = ["git", "-c", f"core.hooksPath={os.devnull}", "-C", str(checkout)]

        if not (checkout / ".git").is_dir():
            print(f"Preparing {project.repository}@{project.revision}", flush=True)
            run([*git, "init", "--quiet"], environment=git_environment)
            run(
                [
                    *git,
                    "remote",
                    "add",
                    "origin",
                    project.url,
                ],
                environment=git_environment,
            )

        remote = subprocess.run(
            [*git, "config", "--local", "--get", "remote.origin.url"],
            cwd=REPOSITORY_ROOT,
            env=git_environment,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        if remote != project.url:
            raise RuntimeError(
                f"Unexpected origin for cached {project.name} checkout: "
                f"expected {project.url}, got {remote}"
            )

        run(
            [*git, "sparse-checkout", "set", "--cone", *project.source_directories],
            environment=git_environment,
        )

        current_revision = subprocess.run(
            [*git, "rev-parse", "--verify", "HEAD"],
            cwd=REPOSITORY_ROOT,
            env=git_environment,
            check=False,
            capture_output=True,
            text=True,
        )
        if (
            current_revision.returncode != 0
            or current_revision.stdout.strip() != project.revision
        ):
            run_git_with_retry(
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
                environment=git_environment,
            )

        run_git_with_retry(
            [
                *git,
                "checkout",
                "--quiet",
                "--detach",
                "--force",
                "--no-recurse-submodules",
                project.revision,
            ],
            environment=git_environment,
        )

        for source_directory in project.source_directories:
            source = checkout / source_directory
            if not source.is_dir():
                raise RuntimeError(
                    f"Missing training source directory {source_directory!r} "
                    f"in {project.repository}@{project.revision}"
                )

        tracked_files = subprocess.run(
            [*git, "ls-files", "-z", "--", *project.source_directories],
            cwd=REPOSITORY_ROOT,
            env=git_environment,
            check=True,
            capture_output=True,
        ).stdout.split(b"\0")
        project_paths = [
            str(path)
            for tracked_file in tracked_files
            if tracked_file
            and (path := checkout / os.fsdecode(tracked_file)).suffix in {".py", ".pyi"}
            and path.is_file()
            and not path.is_symlink()
            and not EXCLUDED_DIRECTORIES.intersection(
                path.relative_to(checkout).parts[:-1]
            )
        ]

        if not project_paths:
            raise RuntimeError(
                f"No Python training files found in {project.repository}"
            )
        paths.extend(sorted(project_paths))
        print(f"  {project.name}: {len(project_paths)} Python files", flush=True)

    return paths


def run_git_with_retry(command: list[str], *, environment: dict[str, str]) -> None:
    for attempt in range(3):
        try:
            run(command, environment=environment)
            return
        except subprocess.CalledProcessError:
            if attempt == 2:
                raise
            delay = 2**attempt
            print(
                f"Git command failed; retrying in {delay}s (attempt {attempt + 2} of 3)",
                file=sys.stderr,
                flush=True,
            )
            time.sleep(delay)


def write_corpus_arguments(target_directory: Path, corpus: list[str]) -> Path:
    arguments = target_directory / "ruff-pgo.args"
    arguments.write_text("\n".join(corpus) + "\n", encoding="utf-8", newline="\n")
    return arguments


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
    logged_arguments = 16
    displayed_command = shlex.join(command[:logged_arguments])
    if len(command) > logged_arguments:
        displayed_command += (
            f" ... ({len(command) - logged_arguments} arguments omitted)"
        )
    print(f"> {displayed_command}", flush=True)
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
