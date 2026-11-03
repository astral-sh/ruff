"""The expected source files and exclusions for native CLI discovery."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from benchmark.projects import Project

FILE_LIST = "benchmark-files.txt"
PYRIGHT_CONFIG = "pyrightconfig.benchmark.json"


def write_cli_targets(project: Project, checkout: Path) -> Project:
    """Record the expected files and configure native discovery of that scope."""
    entries = subprocess.check_output(
        [
            "git",
            "ls-files",
            "--stage",
            "-z",
            "--",
            *(project.include or ["."]),
            *(f":(exclude,glob){pattern}" for pattern in project.exclude),
        ],
        cwd=checkout,
    )
    # Git's regular-file modes exclude symlinks and submodules, including on Windows.
    files = {
        PurePosixPath(os.fsdecode(entry.split(b"\t", 1)[1]))
        for entry in entries.split(b"\0")
        if entry.startswith((b"100644 ", b"100755 "))
        and entry.endswith((b".py", b".pyi"))
    }
    directories = {parent for path in files for parent in path.parents}
    # mypy accepts one definition per module. Prefer a populated directory to a
    # same-named file and a stub to its adjacent implementation. Explicitly
    # targeting an implementation overrides its stub for mypy's imports, losing
    # types that the other checkers still resolve from the stub.
    targets = sorted(
        f"./{path}"
        for path in files
        if path.with_suffix("") not in directories
        and (path.suffix == ".pyi" or path.with_suffix(".pyi") not in files)
    )
    if not targets:
        raise ValueError(f"No Python source files selected for {project.name}")
    for target in targets:
        # Native include/exclude formats use globs and the audit is line-based.
        # Refuse paths those formats cannot represent literally.
        if any(char in target for char in "\r\n\\*?[]{}"):
            raise ValueError(f"Unsupported benchmark source path: {target!r}")
        if not (checkout / target).is_file():
            raise FileNotFoundError(f"Missing benchmark source file: {target}")

    manifest = "".join(f"{target}\n" for target in targets).encode("utf-8")
    (checkout / FILE_LIST).write_bytes(manifest)
    selected = {PurePosixPath(path) for path in targets}
    selected_parents = {parent for path in selected for parent in path.parents}
    unwanted = {
        PurePosixPath(os.fsdecode(entry.split(b"\t", 1)[1]))
        for entry in entries.split(b"\0")
        if entry.startswith(b"120000 ")
    }
    for include in project.include or ["."]:
        start = checkout / include
        if start.is_file():
            if PurePosixPath(include) not in selected:
                unwanted.add(PurePosixPath(include))
            continue
        for directory, directories, filenames in os.walk(start, followlinks=False):
            base = Path(directory)
            for name in directories + filenames:
                path = base / name
                relative = PurePosixPath(path.relative_to(checkout).as_posix())
                if path.is_symlink() or (
                    path.is_file()
                    and path.suffix in (".py", ".pyi", ".ipynb")
                    and relative not in selected
                ):
                    unwanted.add(relative)

    # Collapse excluded Python subtrees, without pre-pruning asset-only directories
    # that the checkers would normally visit during their timed discovery.
    exclusions = set()
    for path in unwanted:
        while path.parent not in selected_parents and path.parent != PurePosixPath("."):
            path = path.parent
        exclusions.add(path.as_posix())
    for exclusion in exclusions:
        if any(char in exclusion for char in "\r\n\\*?[]{}"):
            raise ValueError(f"Unsupported benchmark exclusion: {exclusion!r}")
    native_project = project._replace(
        include=list(project.include) or ["."], exclude=sorted(exclusions)
    )
    (checkout / PYRIGHT_CONFIG).write_text(
        json.dumps(
            {
                "extends": "./pyrightconfig.json",
                "include": list(native_project.include),
                "exclude": list(native_project.exclude),
                "analyzeUnannotatedFunctions": True,
            }
        ),
        encoding="utf-8",
    )
    digest = hashlib.sha256(manifest).hexdigest()
    print(
        f"{project.name}: {len(targets)} CLI source files (sha256 {digest})", flush=True
    )
    return native_project
