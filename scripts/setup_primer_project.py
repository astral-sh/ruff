#!/usr/bin/env -S uv run --script
#
# /// script
# requires-python = ">=3.11"
# dependencies = ["mypy-primer"]
#
# [tool.uv]
# # The only direct dependency of this script is mypy-primer,
# # and mypy-primer is a git dependency, so it is unaffected
# # by the `exclude-newer` setting:
# #
# # > The --exclude-newer option is only applied to packages
# # > that are read from a registry (as opposed to, e.g., Git dependencies).
# # -- https://docs.astral.sh/uv/concepts/resolution/#reproducible-resolutions
# #
# # That's probably desirable: we usually want the latest
# # version of mypy-primer anyway. But it's still worth setting
# # `exclude-newer` here for any transitive dependencies of
# # mypy-primer.
# exclude-newer = "7 days"
#
# [tool.uv.sources]
# mypy-primer = { git = "https://github.com/hauntsaninja/mypy_primer" }
# ///

"""Clone a mypy-primer project and set up a virtualenv with its dependencies installed.

Usage: uv run --no-project scripts/setup_primer_project.py <project-name> [directory] [options]
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


if __name__ == "__main__":
    main()
