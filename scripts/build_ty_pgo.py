"""Build ty with profile-guided optimization using pinned ecosystem projects."""

# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import argparse
import json
import os
import queue
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass
from pathlib import Path

RUST_WORKSPACE_ROOT = Path(__file__).resolve().parent.parent
DEPENDENCY_EXCLUDE_NEWER = "2026-08-06T08:40:30Z"
EXCLUDED_DIRECTORIES = frozenset({"_tests", "_vendor", "test", "tests"})
EXCLUDED_ENVIRONMENT_VARIABLES = (
    "CONDA_PREFIX",
    "PYTHONPATH",
    "TY_CONFIG_FILE",
    "TY_LOG",
    "TY_LOG_PROFILE",
    "TY_OUTPUT_FORMAT",
    "TY_UV",
    "UV",
    "VIRTUAL_ENV",
)


@dataclass(frozen=True, slots=True)
class EcosystemProject:
    name: str
    revision: str
    source_directories: tuple[str, ...]
    python_version: str

    def __post_init__(self) -> None:
        if re.fullmatch(r"[0-9a-f]{40}", self.revision) is None:
            raise ValueError(
                f"{self.name} must be pinned to a full Git commit SHA, "
                f"got {self.revision!r}"
            )
        if re.fullmatch(r"3\.\d+", self.python_version) is None:
            raise ValueError(
                f"{self.name} must specify a Python major.minor version, "
                f"got {self.python_version!r}"
            )


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
        revision="28e86a6c2ae0173831e4925a4af89b02a2936d09",
        source_directories=("src/_pytest",),
        python_version="3.10",
    ),
    EcosystemProject(
        name="httpx",
        revision="b5addb64f0161ff6bfe94c124ef76f6a1fba5254",
        source_directories=("httpx",),
        python_version="3.9",
    ),
    EcosystemProject(
        name="fastapi",
        revision="a375f6b948b99fa4260129856bbf11d037f363ef",
        source_directories=("fastapi",),
        python_version="3.11",
    ),
    EcosystemProject(
        name="anyio",
        revision="ffe91331adb912c5d150f5d373f7cd28a0e96a62",
        source_directories=("src/anyio",),
        python_version="3.10",
    ),
    EcosystemProject(
        name="zulip",
        revision="ccddbba7a3074283ccaac3bde35fd32b19faf042",
        source_directories=("zerver/views", "zerver/models"),
        python_version="3.10",
    ),
    EcosystemProject(
        name="warehouse",
        revision="5a4d2cadec641b5d6a6847d0127940e0f532f184",
        source_directories=(
            "warehouse/accounts",
            "warehouse/oidc",
            "warehouse/forklift",
        ),
        python_version="3.12",
    ),
    EcosystemProject(
        name="pip",
        revision="d1fd55753405fd728a0751a578e27c1054acdf48",
        source_directories=("src/pip/_internal",),
        python_version="3.10",
    ),
    EcosystemProject(
        name="sphinx",
        revision="b06d92e80eed130e1dd4e67cac4afa1267424f1a",
        source_directories=(
            "sphinx/builders",
            "sphinx/ext/autodoc",
            "sphinx/domains/python",
        ),
        python_version="3.12",
    ),
    EcosystemProject(
        name="astropy",
        revision="b779108c7cec25c840c0f744fdf2a1550441e309",
        source_directories=("astropy/units",),
        python_version="3.11",
    ),
    EcosystemProject(
        name="typeshed",
        revision="e0efbeef901e9b6998d016e1ab9352678f09ae77",
        source_directories=(
            "stdlib/asyncio",
            "stdlib/collections",
            "stubs/requests",
        ),
        python_version="3.10",
    ),
)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", help="Host-native Rust target triple")
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Use debug builds to validate the complete PGO pipeline",
    )
    parser.add_argument(
        "--target-dir",
        type=Path,
        help="Cargo target directory (default: CARGO_TARGET_DIR or target/ty-pgo)",
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
        help="Only produce <target-dir>/ty.profdata for a subsequent release build",
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
            os.environ.get(
                "CARGO_TARGET_DIR", RUST_WORKSPACE_ROOT / "target" / "ty-pgo"
            )
        )
    ).resolve()
    profile_dir = (args.profile_dir or target_dir / "profiles").resolve()
    merged_profile = target_dir / "ty.profdata"

    environment = os.environ.copy()
    if args.prepare_corpus:
        prepare_project_environments(
            target_dir / "environments",
            target_dir / "corpus",
            environment=environment,
            install_dependencies=False,
        )
        corpus = ecosystem_python_files(target_dir / "corpus", environment=environment)
        print(f"Prepared {len(corpus)} ecosystem Python files", flush=True)
        return

    host = rustc_host()
    target = args.target or host
    if target != host:
        parser.error(
            f"PGO training requires the host-native target {host}, got {target}"
        )

    profiler = find_llvm_profdata(host, args.llvm_profdata)
    project_environments = prepare_project_environments(
        target_dir / "environments", target_dir / "corpus", environment=environment
    )
    corpus = ecosystem_python_files(target_dir / "corpus", environment=environment)

    profile_dir.mkdir(parents=True, exist_ok=True)
    for profile in profile_dir.glob("ty-*.profraw"):
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
    profile = "debug" if args.debug else "release"
    print(f"Building instrumented {profile} ty", flush=True)
    run(cargo_command(target, debug=args.debug), environment=instrumented_environment)

    binary_name = "ty.exe" if "windows" in target else "ty"
    instrumented_binary = instrumented_target_dir / target / profile / binary_name
    if not instrumented_binary.is_file():
        raise RuntimeError(f"Instrumented ty binary not found: {instrumented_binary}")

    profiles = train_ty(
        instrumented_binary,
        target_dir / "corpus",
        profile_dir,
        corpus_size=len(corpus),
        project_environments=project_environments,
        environment=instrumented_environment,
    )
    merge_profiles(profiler, profiles, merged_profile, environment=environment)
    hot_count = profile_hot_count(profiler, merged_profile, environment=environment)
    hot_count_path = target_dir / "ty.profile-hot-count"
    hot_count_path.write_text(f"{hot_count}\n", encoding="utf-8", newline="\n")
    print(f"Using 95th-percentile PGO hot count: {hot_count}", flush=True)

    if args.train_only:
        return

    optimized_environment = environment | {
        "CARGO_TARGET_DIR": str(target_dir),
        "RUSTFLAGS": append_flags(
            environment.get("RUSTFLAGS"),
            f"-Cprofile-use={merged_profile} "
            f"-Cllvm-args=--profile-summary-hot-count={hot_count}",
        ),
    }
    print(f"Building profile-guided {profile} ty", flush=True)
    run(cargo_command(target, debug=args.debug), environment=optimized_environment)
    print(
        f"Profile-guided ty: {target_dir / target / profile / binary_name}", flush=True
    )


def train_ty(
    binary: Path,
    corpus_directory: Path,
    profile_directory: Path,
    *,
    corpus_size: int,
    project_environments: dict[str, Path],
    environment: dict[str, str],
) -> list[Path]:
    print(f"Training on {corpus_size} ecosystem Python files", flush=True)
    profiles = []
    for project in CORPUS_PROJECTS:
        checkout = corpus_directory / project.name
        training_environment = environment | {
            "LLVM_PROFILE_FILE": str(
                profile_directory / f"ty-{project.name}-%m-%p.profraw"
            )
        }
        for variable in EXCLUDED_ENVIRONMENT_VARIABLES:
            training_environment.pop(variable, None)

        print(f"Profiling ty on {project.name}", flush=True)
        run(
            [
                str(binary),
                "check",
                "--python",
                str(project_environments[project.name]),
                "--project",
                str(checkout),
                "--python-version",
                project.python_version,
                *(
                    argument
                    for directory in sorted(EXCLUDED_DIRECTORIES)
                    for argument in ("--exclude", f"{directory}/")
                ),
                "--exit-zero",
                "--no-progress",
                "-qq",
                *(str(checkout / source) for source in project.source_directories),
            ],
            environment=training_environment,
        )

        workload_profiles = sorted(
            profile_directory.glob(f"ty-{project.name}-*.profraw")
        )
        if not workload_profiles or any(
            profile.stat().st_size == 0 for profile in workload_profiles
        ):
            raise RuntimeError(
                f"No complete ty {project.name} profiling data found "
                f"in {profile_directory}"
            )
        profiles.extend(workload_profiles)

    profile_language_server(binary, profile_directory, environment)
    language_server_profiles = sorted(
        profile_directory.glob("ty-language-server-*.profraw")
    )
    if not language_server_profiles or any(
        profile.stat().st_size == 0 for profile in language_server_profiles
    ):
        raise RuntimeError(
            f"No complete ty language-server profiling data found "
            f"in {profile_directory}"
        )
    profiles.extend(language_server_profiles)
    return profiles


def prepare_project_environments(
    environment_directory: Path,
    corpus_directory: Path,
    *,
    environment: dict[str, str],
    install_dependencies: bool = True,
) -> dict[str, Path]:
    uv = shutil.which("uv", path=environment.get("PATH"))
    if uv is None:
        raise RuntimeError("uv is required to install PGO training dependencies")

    environment_directory.mkdir(parents=True, exist_ok=True)
    interpreters: dict[str, Path] = {}
    for project in CORPUS_PROJECTS:
        checkout = corpus_directory / project.name
        destination = environment_directory / project.name
        interpreter = destination / (
            "Scripts/python.exe" if sys.platform == "win32" else "bin/python"
        )
        command = [
            uv,
            "run",
            "--locked",
            "--script",
            str(RUST_WORKSPACE_ROOT / "scripts" / "setup_primer_project.py"),
            project.name,
            str(checkout),
            "--revision",
            project.revision,
            *(
                argument
                for source_directory in project.source_directories
                for argument in ("--sparse", source_directory)
            ),
        ]
        if install_dependencies:
            command.extend(
                (
                    "--python",
                    project.python_version,
                    "--venv-directory",
                    str(destination),
                    "--exclude-newer",
                    DEPENDENCY_EXCLUDE_NEWER,
                    "--only-binary",
                    "--install-project-dependencies",
                )
            )
        else:
            command.append("--skip-install")
        run(command, environment=environment)

        if install_dependencies:
            if not interpreter.is_file():
                raise RuntimeError(
                    f"No Python interpreter found for {project.name}: {interpreter}"
                )
            interpreters[project.name] = interpreter

    return interpreters


def merge_profiles(
    profiler: Path,
    profiles: list[Path],
    destination: Path,
    *,
    environment: dict[str, str],
) -> None:
    profile_size = sum(profile.stat().st_size for profile in profiles)

    with tempfile.NamedTemporaryFile(
        dir=destination.parent, prefix="ty-", suffix=".profdata", delete=False
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


def profile_hot_count(
    profiler: Path, profile: Path, *, environment: dict[str, str]
) -> int:
    # LLVM uses the 95th percentile for profile-guided size optimization, but
    # defaults to the 99th percentile for hot-code optimization. Align them to
    # avoid aggressively expanding moderately hot functions.
    summary = subprocess.run(
        [
            str(profiler),
            "show",
            "--detailed-summary",
            "--detailed-summary-cutoffs=950000",
            str(profile),
        ],
        cwd=RUST_WORKSPACE_ROOT,
        env=environment,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    match = re.search(
        r"with count >= (\d+) account for 95% of the total counts\.", summary
    )
    if match is None:
        raise RuntimeError("Could not determine the 95th-percentile PGO hot count")

    count = int(match.group(1))
    if count <= 0:
        raise RuntimeError(f"PGO hot count must be positive, got {count}")
    return count


def profile_language_server(
    binary: Path, profile_directory: Path, environment: dict[str, str]
) -> None:
    print("Profiling ty language-server incremental edits", flush=True)
    server_environment = environment | {
        "LLVM_PROFILE_FILE": str(profile_directory / "ty-language-server-%m-%p.profraw")
    }
    for variable in EXCLUDED_ENVIRONMENT_VARIABLES:
        server_environment.pop(variable, None)

    with tempfile.TemporaryDirectory(
        prefix="ty-pgo-language-server-", dir=profile_directory.parent
    ) as temporary:
        root = Path(temporary)
        (root / "pyproject.toml").write_text(
            '[project]\nname = "ty-pgo"\nversion = "0.0.0"\n'
            f'requires-python = ">={sys.version_info.major}.{sys.version_info.minor}"\n',
            encoding="utf-8",
        )
        models = root / "models.py"
        models_text = (
            "from dataclasses import dataclass\n\n"
            "@dataclass\nclass User:\n    name: str\n    age: int\n\n"
            'def load_user() -> User:\n    return User("Ada", 37)\n'
        )
        models.write_text(models_text, encoding="utf-8")
        service = root / "service.py"
        service_text = (
            "from models import User, load_user\n\n"
            "def describe(user: User) -> str:\n    return user.name.upper()\n\n"
            "user = load_user()\nresult = describe(user)\n"
            "next_age = user.age + 1\n"
        )
        service.write_text(service_text, encoding="utf-8")

        process = subprocess.Popen(
            [str(binary), "server"],
            cwd=root,
            env=server_environment,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=0,
        )
        if process.stdin is None or process.stdout is None or process.stderr is None:
            process.kill()
            process.wait()
            raise RuntimeError("Could not open language-server pipes")

        stdin = process.stdin
        stdout = process.stdout
        responses: queue.Queue[dict[str, object] | Exception] = queue.Queue()
        request_id = 0

        def read_responses() -> None:
            try:
                while True:
                    headers: dict[str, str] = {}
                    while True:
                        line = stdout.readline()
                        if not line:
                            raise RuntimeError("Language server closed its output")
                        if line in (b"\r\n", b"\n"):
                            break
                        name, _, value = line.decode("ascii").partition(":")
                        headers[name.lower()] = value.strip()

                    remaining = int(headers["content-length"])
                    chunks: list[bytes] = []
                    while remaining:
                        chunk = stdout.read(remaining)
                        if not chunk:
                            raise RuntimeError("Language server closed its output")
                        chunks.append(chunk)
                        remaining -= len(chunk)
                    response = json.loads(b"".join(chunks))
                    if not isinstance(response, dict):
                        raise RuntimeError("Expected a JSON-RPC response object")
                    responses.put(response)
            except (OSError, RuntimeError, UnicodeError, ValueError, KeyError) as error:
                responses.put(error)

        def send(message: dict[str, object]) -> None:
            payload = json.dumps(message, separators=(",", ":")).encode("utf-8")
            stdin.write(f"Content-Length: {len(payload)}\r\n\r\n".encode("ascii"))
            stdin.write(payload)

        def request(method: str, params: dict[str, object] | None = None) -> object:
            nonlocal request_id
            request_id += 1
            identifier = request_id
            send(
                {"jsonrpc": "2.0", "id": identifier, "method": method, "params": params}
            )
            deadline = time.monotonic() + 15
            while True:
                try:
                    response = responses.get(
                        timeout=max(0, deadline - time.monotonic())
                    )
                except queue.Empty as error:
                    raise TimeoutError(
                        f"Language-server request timed out: {method}"
                    ) from error
                if isinstance(response, Exception):
                    raise RuntimeError(
                        "Could not read language-server output"
                    ) from response
                if "method" in response and "id" in response:
                    result = (
                        [] if response["method"] == "workspace/configuration" else None
                    )
                    send({"jsonrpc": "2.0", "id": response["id"], "result": result})
                elif response.get("id") == identifier:
                    if "error" in response:
                        raise RuntimeError(f"{method} failed: {response['error']}")
                    return response.get("result")

        def notify(method: str, params: dict[str, object] | None = None) -> None:
            send({"jsonrpc": "2.0", "method": method, "params": params or {}})

        def diagnostics(path: Path) -> list[object]:
            result = request(
                "textDocument/diagnostic", {"textDocument": {"uri": path.as_uri()}}
            )
            if not isinstance(result, dict):
                raise RuntimeError("Expected a full document diagnostic report")
            return result.get("items", [])

        reader = threading.Thread(target=read_responses, daemon=True)
        reader.start()
        try:
            initialized = request(
                "initialize",
                {
                    "processId": os.getpid(),
                    "rootUri": root.as_uri(),
                    "workspaceFolders": [{"uri": root.as_uri(), "name": root.name}],
                    "capabilities": {
                        "workspace": {"configuration": False},
                        "textDocument": {"diagnostic": {"dynamicRegistration": False}},
                    },
                },
            )
            if not isinstance(initialized, dict) or not initialized.get(
                "capabilities", {}
            ).get("diagnosticProvider"):
                raise RuntimeError("Language server does not support pull diagnostics")
            notify("initialized")

            for path, text in ((models, models_text), (service, service_text)):
                notify(
                    "textDocument/didOpen",
                    {
                        "textDocument": {
                            "uri": path.as_uri(),
                            "languageId": "python",
                            "version": 1,
                            "text": text,
                        }
                    },
                )
            if diagnostics(models) or diagnostics(service):
                raise RuntimeError("Expected clean initial language-server diagnostics")

            for method, position in (
                ("textDocument/hover", {"line": 7, "character": 17}),
                ("textDocument/definition", {"line": 5, "character": 8}),
                ("textDocument/completion", {"line": 5, "character": 16}),
            ):
                result = request(
                    method,
                    {"textDocument": {"uri": service.as_uri()}, "position": position},
                )
                if not result:
                    raise RuntimeError(f"Expected a nonempty {method} result")

            for index in range(12):
                invalid = index % 2 == 0
                changed = (
                    models_text.replace("age: int", "age: str")
                    if invalid
                    else models_text
                )
                notify(
                    "textDocument/didChange",
                    {
                        "textDocument": {
                            "uri": models.as_uri(),
                            "version": index + 2,
                        },
                        "contentChanges": [{"text": changed}],
                    },
                )
                if (
                    bool(diagnostics(models)) != invalid
                    or bool(diagnostics(service)) != invalid
                ):
                    raise RuntimeError(
                        "Incremental cross-file diagnostics did not update"
                    )

            request("shutdown")
            notify("exit")
            process.stdin.close()
            if process.wait(timeout=10):
                raise RuntimeError(
                    process.stderr.read().decode("utf-8", errors="replace")
                )
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            reader.join(timeout=1)


def rustc_host() -> str:
    version = subprocess.run(
        ["rustc", "--version", "--verbose"],
        cwd=RUST_WORKSPACE_ROOT,
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
            cwd=RUST_WORKSPACE_ROOT,
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
    git_environment = environment | {
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_TERMINAL_PROMPT": "0",
        "GIT_LFS_SKIP_SMUDGE": "1",
    }
    paths: list[str] = []

    for project in CORPUS_PROJECTS:
        checkout = corpus_directory / project.name
        git = ["git", "-c", f"core.hooksPath={os.devnull}", "-C", str(checkout)]

        for source_directory in project.source_directories:
            source = checkout / source_directory
            if not source.is_dir():
                raise RuntimeError(
                    f"Missing training source directory {source_directory!r} "
                    f"in {project.name}@{project.revision}"
                )

        tracked_files = subprocess.run(
            [*git, "ls-files", "-z", "--", *project.source_directories],
            cwd=RUST_WORKSPACE_ROOT,
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
            raise RuntimeError(f"No Python training files found in {project.name}")
        paths.extend(sorted(project_paths))
        print(f"  {project.name}: {len(project_paths)} Python files", flush=True)

    return paths


def cargo_command(target: str, *, debug: bool = False) -> list[str]:
    return [
        "cargo",
        "rustc",
        *(() if debug else ("--release",)),
        "--locked",
        "--package",
        "ty",
        "--bin",
        "ty",
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
        command, cwd=RUST_WORKSPACE_ROOT, env=environment, check=False
    )
    if completed.returncode not in allowed_exit_codes:
        raise subprocess.CalledProcessError(completed.returncode, command)


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
