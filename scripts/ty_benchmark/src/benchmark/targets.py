"""The expected source files and exclusions for native CLI discovery."""

from __future__ import annotations

import hashlib
import io
import json
import os
import subprocess
import tokenize
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING, TypedDict

if TYPE_CHECKING:
    from benchmark.projects import Project

FILE_LIST = "benchmark-files.txt"
PYRIGHT_CONFIG = "pyrightconfig.benchmark.json"


class FileDirective(TypedDict):
    line: int
    directive: str


class SourceOverride(TypedDict):
    path: str
    directives: list[FileDirective]
    before_sha256: str
    after_sha256: str


def neutralize_file_directives(checkout: Path) -> None:
    """Remove whole-file checker overrides, preserving line-level suppressions."""
    entries = subprocess.check_output(
        ["git", "ls-files", "--stage", "-z"], cwd=checkout
    )
    changes: list[SourceOverride] = []
    for entry in entries.split(b"\0"):
        if not entry.startswith((b"100644 ", b"100755 ")) or not entry.endswith(
            (b".py", b".pyi")
        ):
            continue
        name = os.fsdecode(entry.split(b"\t", 1)[1])
        path = checkout / name
        original = path.read_bytes()
        if not any(
            marker in original
            for marker in (b"mypy:", b"pyright:", b"pyrefly:", b"type: ignore")
        ):
            continue
        encoding, _ = tokenize.detect_encoding(io.BytesIO(original).readline)
        lines = original.decode(encoding).splitlines(keepends=True)
        overrides: list[FileDirective] = []
        before_code = True
        try:
            for token in tokenize.tokenize(io.BytesIO(original).readline):
                if token.type == tokenize.COMMENT:
                    comment = token.string.removeprefix("#").lstrip()
                    whole_file = comment.startswith("mypy:")
                    if comment.startswith("pyright:"):
                        whole_file = (
                            not comment.removeprefix("pyright:")
                            .lstrip()
                            .startswith("ignore")
                        )
                    if comment.startswith("pyrefly:"):
                        whole_file = (
                            comment.removeprefix("pyrefly:")
                            .lstrip()
                            .startswith("ignore-errors")
                        )
                    if before_code and comment.startswith("type: ignore"):
                        whole_file = True
                    if whole_file:
                        row, column = token.start
                        line = lines[row - 1]
                        lines[row - 1] = (
                            line[:column]
                            + "# benchmark: "
                            + comment
                            + line[token.end[1] :]
                        )
                        overrides.append({"line": row, "directive": token.string})
                elif token.type not in (
                    tokenize.ENCODING,
                    tokenize.NL,
                    tokenize.NEWLINE,
                    tokenize.ENDMARKER,
                ):
                    before_code = False
        except tokenize.TokenError, IndentationError:
            # Reproduction inputs outside the root set can intentionally be invalid.
            continue
        # mypy also scans raw column-zero lines, including text inside literals.
        for row, line in enumerate(lines):
            if line.startswith("# mypy: "):
                lines[row] = "# benchmark: " + line.removeprefix("# ")
                overrides.append({"line": row + 1, "directive": line.rstrip()})
        if overrides:
            normalized = "".join(lines).encode(encoding)
            path.write_bytes(normalized)
            changes.append(
                {
                    "path": name,
                    "directives": overrides,
                    "before_sha256": hashlib.sha256(original).hexdigest(),
                    "after_sha256": hashlib.sha256(normalized).hexdigest(),
                }
            )
    report = checkout / "benchmark-source-overrides.json"
    if changes or not report.exists():
        previous = (
            json.loads(report.read_text(encoding="utf-8"))["files"]
            if report.exists()
            else []
        )
        by_path = {item["path"]: item for item in previous}
        for change in changes:
            if old := by_path.get(change["path"]):
                change["before_sha256"] = old["before_sha256"]
                change["directives"] = [*old["directives"], *change["directives"]]
            by_path[change["path"]] = change
        report.write_text(
            json.dumps({"files": [by_path[path] for path in sorted(by_path)]}, indent=2)
            + "\n",
            encoding="utf-8",
        )


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
    packages = {
        path.parent for path in files if path.name in ("__init__.py", "__init__.pyi")
    }
    shadowed_directories = {
        path.with_suffix("")
        for path in files
        if path.with_suffix("") in directories and path.with_suffix("") not in packages
    }
    # mypy accepts one definition per module. A package wins over a same-named
    # file, while a module wins over a non-package directory of build inputs.
    # Prefer a stub to its adjacent implementation. Explicitly
    # targeting an implementation overrides its stub for mypy's imports, losing
    # types that the other checkers still resolve from the stub.
    targets = sorted(
        f"./{path}"
        for path in files
        if path.with_suffix("") not in packages
        and not any(directory in path.parents for directory in shadowed_directories)
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
    implementation_lines = sum(
        (checkout / path).read_bytes().count(b"\n")
        for path in files
        if path.suffix == ".py"
    )
    selected_implementation_lines = sum(
        (checkout / path).read_bytes().count(b"\n")
        for path in selected
        if path.suffix == ".py"
    )
    (checkout / "benchmark-workload.json").write_text(
        json.dumps(
            {
                "roots": len(targets),
                "stubs": sum(path.suffix == ".pyi" for path in selected),
                "eligible_implementation_lines": implementation_lines,
                "selected_implementation_lines": selected_implementation_lines,
                "omitted_implementation_line_fraction": 1
                - selected_implementation_lines / implementation_lines
                if implementation_lines
                else 0,
                "manifest_sha256": digest,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"{project.name}: {len(targets)} CLI roots (sha256 {digest})", flush=True)
    return native_project
