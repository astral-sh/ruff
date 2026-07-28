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
# mypy-primer = { git = "https://github.com/hauntsaninja/mypy_primer" }
# ///

"""Clone a mypy-primer project and set up a virtualenv with its dependencies installed.

For ecosystem-report reproduction, always select the project's ecosystem-analyzer Python version and bypass the adjacent lock with the exact mypy-primer revision pinned by ecosystem-analyzer:

uv run --python <version> --with "mypy-primer @ git+https://github.com/hauntsaninja/mypy_primer@<mypy-primer-revision>" --no-project python scripts/setup_primer_project.py <project-name> [directory] [options]
"""

from __future__ import annotations

import argparse
import shlex
import subprocess
import sys
from pathlib import Path
from typing import NoReturn

from mypy_primer.model import Project
from mypy_primer.projects import get_projects


def find_project(name: str) -> Project:
    projects = get_projects()
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
    args = parser.parse_args()

    project = find_project(args.project)
    revision = args.revision or project.revision

    target_dir = Path(args.directory or project.name).resolve()

    # Use a full clone only when a historical ecosystem report revision must be checked out.
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

    # Create venv (matching primer's Venv.make_venv())
    venv_dir = target_dir / ".venv"
    print(f"Creating virtualenv at {venv_dir}...")
    subprocess.run(
        ["uv", "venv", str(venv_dir), "--python", sys.executable, "--seed", "--clear"],
        check=True,
    )

    venv_python = venv_dir / "bin" / "python"
    install_base = f"uv pip install --python {shlex.quote(str(venv_python))}"
    if args.exclude_newer:
        install_base += f" --exclude-newer {shlex.quote(args.exclude_newer)}"

    # Run custom install command if the project defines one (matching primer's setup())
    if project.install_cmd:
        assert "{install}" in project.install_cmd
        install_cmd = project.install_cmd.format(install=install_base)
        print(f"Running install command: {install_cmd}")
        # Primer install commands are trusted project metadata and may use shell syntax.
        subprocess.run(install_cmd, cwd=target_dir, shell=True, check=True)  # noqa: S602

    # Install listed dependencies (matching primer's setup())
    if project.deps:
        deps_cmd_parts = shlex.split(install_base) + project.deps
        print(f"Installing dependencies: {', '.join(project.deps)}")
        subprocess.run(deps_cmd_parts, cwd=target_dir, check=True)

    print(f"\nDone! Project set up at {target_dir}")
    print(f"Activate the venv with: source {venv_dir}/bin/activate")
    print("\nProject-specific ty command:")
    print("  ty_binary=/path/to/ty")
    ty_command = get_ty_command(project, ty_binary='"$ty_binary"', venv_dir=venv_dir)
    print(f"  {ty_command}")


if __name__ == "__main__":
    main()
