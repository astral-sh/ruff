#!/usr/bin/env -S uv run --script
#
# /// script
# requires-python = ">=3.11"
# dependencies = ["mypy-primer"]
#
# [tool.uv]
# # This is the default for ad hoc use. Historical ecosystem reproduction must
# # bypass the adjacent lock and select ecosystem-analyzer's exact mypy-primer
# # revision and project Python version, as shown in the module docstring.
# # `exclude-newer` still constrains mypy-primer's registry dependencies.
# exclude-newer = "7 days"
#
# [tool.uv.sources]
# # Keep this revision and the script's lockfile in sync with ecosystem-analyzer's
# # mypy-primer pin so memory reports and ecosystem jobs use the same project definitions.
# mypy-primer = { git = "https://github.com/hauntsaninja/mypy_primer", rev = "6d6eebd8d37c9b8931381e79aa99808d9378c988" }
# ///

"""Clone a mypy-primer project and set up a virtualenv with its dependencies installed.

For ecosystem-report reproduction, always select the project's ecosystem-analyzer Python version and bypass the adjacent lock with the exact mypy-primer revision pinned by ecosystem-analyzer:

uv run --python <version> --with "mypy-primer @ git+https://github.com/hauntsaninja/mypy_primer@<mypy-primer-revision>" --no-project python scripts/setup_primer_project.py <project-name> [directory] [options]
"""

from __future__ import annotations

import argparse
import os
import shlex
import subprocess
import sys
import time
import tomllib
from pathlib import Path
from typing import NoReturn

from mypy_primer.model import Project
from mypy_primer.projects import get_projects

ADDITIONAL_PROJECTS = (
    Project(
        location="https://github.com/encode/httpx",
        mypy_cmd=None,
        pyright_cmd=None,
        paths=["httpx"],
    ),
    Project(
        location="https://github.com/fastapi/fastapi",
        mypy_cmd=None,
        pyright_cmd=None,
        paths=["fastapi"],
    ),
    Project(
        location="https://github.com/pypi/warehouse",
        mypy_cmd=None,
        pyright_cmd=None,
        paths=["warehouse"],
        deps=[
            "pyramid",
            "pyramid-jinja2",
            "sqlalchemy",
            "pydantic",
            "requests",
            "redis",
            "packaging",
            "cryptography",
        ],
    ),
    Project(
        location="https://github.com/astropy/astropy",
        mypy_cmd=None,
        pyright_cmd=None,
        paths=["astropy"],
    ),
    Project(
        location="https://github.com/python/typeshed",
        mypy_cmd=None,
        pyright_cmd=None,
        paths=["stdlib", "stubs"],
    ),
)


def find_project(name: str) -> Project:
    projects = [*get_projects(), *ADDITIONAL_PROJECTS]
    for p in projects:
        if p.name == name:
            return p
    _project_not_found(name, projects)


def _project_not_found(name: str, projects: list[Project]) -> NoReturn:
    print(f"error: project {name!r} not found", file=sys.stderr)
    print("available projects:", file=sys.stderr)
    for p in sorted(projects, key=lambda p: p.name):
        print(f"  {p.name}", file=sys.stderr)
    sys.exit(1)


class _FormatMap:
    def __init__(self, **values: str | list[str] | None) -> None:
        self.values = values

    def __getitem__(self, key: str) -> str:
        if key not in self.values:
            raise KeyError(key)
        value = self.values[key]
        if value is None:
            raise ValueError(f"Required {key} to be specified")
        if isinstance(value, list):
            return " ".join(value)
        return value


def get_ty_command(project: Project, *, ty_binary: str, venv_dir: Path) -> str:
    ty_cmd = project.ty_cmd
    if ty_cmd is None:
        ty_cmd = "{ty} check {paths}" if project.paths else "{ty} check"
    assert "{ty}" in ty_cmd
    ty_cmd = ty_cmd.format_map(_FormatMap(ty=ty_binary, paths=project.paths))
    return f"{ty_cmd} --python {shlex.quote(str(venv_dir))} --output-format concise"


def run_git_with_retry(command: list[str], *, environment: dict[str, str]) -> None:
    for attempt in range(3):
        try:
            subprocess.run(command, env=environment, check=True)
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


def clone_project(
    project: Project,
    target_dir: Path,
    *,
    revision: str | None,
    sparse_directories: list[str],
) -> None:
    if not sparse_directories:
        clone_cmd = [
            "git",
            "clone",
            "--recurse-submodules",
            project.location,
            str(target_dir),
        ]
        if not revision:
            clone_cmd += ["--depth", "1"]
        print(f"Cloning {project.location} into {target_dir}...")
        subprocess.run(clone_cmd, check=True)

        if revision:
            print(f"Checking out revision {revision}...")
            subprocess.run(["git", "checkout", revision], cwd=target_dir, check=True)
            subprocess.run(
                ["git", "submodule", "update", "--init", "--recursive"],
                cwd=target_dir,
                check=True,
            )
        return

    if revision is None:
        raise ValueError("Sparse project checkouts require an explicit revision")

    target_dir.mkdir(parents=True, exist_ok=True)
    environment = os.environ | {
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_TERMINAL_PROMPT": "0",
        "GIT_LFS_SKIP_SMUDGE": "1",
    }
    git = ["git", "-c", f"core.hooksPath={os.devnull}", "-C", str(target_dir)]

    if not (target_dir / ".git").is_dir():
        print(f"Preparing {project.location}@{revision}", flush=True)
        subprocess.run([*git, "init", "--quiet"], env=environment, check=True)
        subprocess.run(
            [*git, "remote", "add", "origin", project.location],
            env=environment,
            check=True,
        )

    remote = subprocess.run(
        [*git, "config", "--local", "--get", "remote.origin.url"],
        env=environment,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if remote != project.location:
        raise RuntimeError(
            f"Unexpected origin for cached {project.name} checkout: "
            f"expected {project.location}, got {remote}"
        )

    subprocess.run(
        [*git, "sparse-checkout", "set", "--cone", *sparse_directories],
        env=environment,
        check=True,
    )
    current_revision = subprocess.run(
        [*git, "rev-parse", "--verify", "HEAD"],
        env=environment,
        check=False,
        capture_output=True,
        text=True,
    )
    if current_revision.returncode != 0 or current_revision.stdout.strip() != revision:
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
                revision,
            ],
            environment=environment,
        )

    run_git_with_retry(
        [
            *git,
            "checkout",
            "--quiet",
            "--detach",
            "--force",
            "--no-recurse-submodules",
            revision,
        ],
        environment=environment,
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("project", help="Name of a mypy-primer project")
    parser.add_argument(
        "directory",
        nargs="?",
        help="Directory to clone into (default: project name)",
    )
    parser.add_argument(
        "--revision",
        help="Git revision to check out before installing dependencies",
    )
    parser.add_argument(
        "--exclude-newer",
        help="Limit dependency resolution to packages uploaded before this timestamp",
    )
    parser.add_argument(
        "--python",
        default=sys.executable,
        help="Python interpreter or version to use for the project virtualenv",
    )
    parser.add_argument(
        "--venv-directory",
        type=Path,
        help="Virtualenv directory (default: <project>/.venv)",
    )
    parser.add_argument(
        "--sparse",
        action="append",
        default=[],
        metavar="DIRECTORY",
        help="Fetch only this source directory; may be specified more than once",
    )
    parser.add_argument(
        "--only-binary",
        action="store_true",
        help="Install dependencies exclusively from prebuilt wheels",
    )
    parser.add_argument(
        "--install-project-dependencies",
        action="store_true",
        help="Also install dependencies declared in the project's pyproject.toml",
    )
    parser.add_argument(
        "--skip-install",
        action="store_true",
        help="Only prepare the project checkout",
    )
    parser.add_argument(
        "--print-ty-command",
        action="store_true",
        help="Print the project-specific ty command without setting up the project",
    )
    args = parser.parse_args()

    project = find_project(args.project)
    revision = args.revision or project.revision

    target_dir = Path(args.directory or project.name).resolve()
    venv_dir = (args.venv_directory or target_dir / ".venv").resolve()
    if args.print_ty_command:
        print(get_ty_command(project, ty_binary="{ty}", venv_dir=venv_dir))
        return

    clone_project(
        project,
        target_dir,
        revision=revision,
        sparse_directories=args.sparse,
    )
    if args.skip_install:
        return

    print(f"Creating virtualenv at {venv_dir}...")
    subprocess.run(
        ["uv", "venv", str(venv_dir), "--python", args.python, "--seed", "--clear"],
        cwd=target_dir,
        check=True,
    )

    venv_python = venv_dir / (
        "Scripts/python.exe" if sys.platform == "win32" else "bin/python"
    )
    install_base = f"uv pip install --python {shlex.quote(str(venv_python))}"
    if args.exclude_newer:
        install_base += f" --exclude-newer {shlex.quote(args.exclude_newer)}"
    if args.only_binary:
        install_base += " --only-binary :all:"

    # Run custom install command if the project defines one (matching primer's setup())
    if project.install_cmd:
        assert "{install}" in project.install_cmd
        install_cmd = project.install_cmd.format(install=install_base)
        print(f"Running install command: {install_cmd}")
        # Primer install commands are trusted project metadata and may use shell syntax.
        subprocess.run(install_cmd, cwd=target_dir, shell=True, check=True)  # noqa: S602

    dependencies = project.deps or []
    manifest = target_dir / "pyproject.toml"
    install_project_dependencies = False
    if args.install_project_dependencies and manifest.is_file():
        with manifest.open("rb") as stream:
            metadata = tomllib.load(stream)
        install_project_dependencies = bool(
            metadata.get("project", {}).get("dependencies", ())
        )

    if dependencies or install_project_dependencies:
        deps_cmd_parts = shlex.split(install_base)
        if install_project_dependencies:
            deps_cmd_parts.extend(("--requirements", str(manifest)))
        deps_cmd_parts.extend(dependencies)
        print(f"Installing dependencies for {project.name}")
        subprocess.run(deps_cmd_parts, cwd=target_dir, check=True)

    print(f"\nDone! Project set up at {target_dir}")
    activation_script = (
        "Scripts/activate" if sys.platform == "win32" else "bin/activate"
    )
    print(f"Activate the venv with: source {venv_dir / activation_script}")
    print("\nProject-specific ty command:")
    print("  ty_binary=/path/to/ty")
    ty_command = get_ty_command(project, ty_binary='"$ty_binary"', venv_dir=venv_dir)
    print(f"  {ty_command}")


if __name__ == "__main__":
    main()
