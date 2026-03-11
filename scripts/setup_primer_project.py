#!/usr/bin/env -S uv run --script
#
# /// script
# requires-python = ">=3.10"
# dependencies = ["mypy-primer"]
#
# [tool.uv.sources]
# mypy-primer = { git = "https://github.com/hauntsaninja/mypy_primer" }
# ///

"""Clone a mypy-primer project and set up a virtualenv with its dependencies installed.

Usage: uv run scripts/setup_primer_project.py <project-name> [directory]
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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("project", help="Name of a mypy-primer project")
    parser.add_argument(
        "directory",
        nargs="?",
        help="Directory to clone into (default: project name)",
    )
    args = parser.parse_args()

    project = find_project(args.project)

    target_dir = Path(args.directory or project.name).resolve()

    # Clone (shallow if no pinned revision, same as primer)
    clone_cmd = [
        "git",
        "clone",
        "--recurse-submodules",
        project.location,
        str(target_dir),
    ]
    if not project.revision:
        clone_cmd += ["--depth", "1"]
    print(f"Cloning {project.location} into {target_dir}...")
    subprocess.run(clone_cmd, check=True)

    if project.revision:
        print(f"Checking out revision {project.revision}...")
        subprocess.run(
            ["git", "checkout", project.revision], cwd=target_dir, check=True
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

    # Run custom install command if the project defines one (matching primer's setup())
    if project.install_cmd:
        install_cmd = project.install_cmd.format(install=install_base)
        print(f"Running install command: {install_cmd}")
        subprocess.run(install_cmd, shell=True, cwd=target_dir, check=True)

    # Install listed dependencies (matching primer's setup())
    if project.deps:
        deps_cmd = f"{install_base} {' '.join(project.deps)}"
        print(f"Installing dependencies: {', '.join(project.deps)}")
        subprocess.run(deps_cmd, shell=True, cwd=target_dir, check=True)

    print(f"\nDone! Project set up at {target_dir}")
    print(f"Activate the venv with: source {venv_dir}/bin/activate")


if __name__ == "__main__":
    main()
