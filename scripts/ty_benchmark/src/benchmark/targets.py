"""The source files passed to every CLI benchmark."""

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


def write_cli_targets(project: Project, checkout: Path) -> None:
    """Write one set of explicitly selected files in the formats the checkers accept."""
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
    # same-named file and an implementation to its adjacent stub. Leave both on
    # disk so each checker can resolve imports normally.
    targets = sorted(
        f"./{path}"
        for path in files
        if path.with_suffix("") not in directories
        and (path.suffix == ".py" or path.with_suffix(".py") not in files)
    )
    if not targets:
        raise ValueError(f"No Python source files selected for {project.name}")
    for target in targets:
        # Pyrefly and Pyright interpret paths as globs. Response files use one
        # argument per line. Refuse paths those formats cannot represent literally.
        if any(char in target for char in "\r\n\\*?[]{}"):
            raise ValueError(f"Unsupported benchmark source path: {target!r}")
        if not (checkout / target).is_file():
            raise FileNotFoundError(f"Missing benchmark source file: {target}")

    manifest = "".join(f"{target}\n" for target in targets).encode("utf-8")
    (checkout / FILE_LIST).write_bytes(manifest)
    # Pyright's stdin file list splits on spaces. JSON preserves each pathname.
    (checkout / PYRIGHT_CONFIG).write_text(
        json.dumps(
            {
                "extends": "./pyrightconfig.json",
                "include": targets,
                "exclude": [],
                "analyzeUnannotatedFunctions": True,
            }
        ),
        encoding="utf-8",
    )
    digest = hashlib.sha256(manifest).hexdigest()
    print(
        f"{project.name}: {len(targets)} CLI source files (sha256 {digest})", flush=True
    )
