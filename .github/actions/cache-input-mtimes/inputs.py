# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Restore timestamps for matching tracked files and unchanged directory entries."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
import subprocess
import time
from pathlib import Path


def tracked_paths(root):
    metadata = json.loads(
        subprocess.check_output(
            [
                "cargo",
                "metadata",
                "--no-deps",
                "--format-version",
                "1",
                "--offline",
                "--locked",
            ],
            cwd=root,
        )
    )
    packages = [
        str(Path(package["manifest_path"]).parent.relative_to(root))
        for package in metadata["packages"]
        if any("custom-build" in target["kind"] for target in package["targets"])
        and Path(package["manifest_path"]).is_relative_to(root)
    ]
    if not packages:
        return []
    output = subprocess.check_output(
        ["git", "ls-files", "-z", "--", *packages], cwd=root
    )
    return sorted({os.fsdecode(name) for name in output.split(b"\0") if name})


def safe_path(root, name):
    relative = Path(name)
    if relative.is_absolute() or ".." in relative.parts:
        raise ValueError(f"Invalid cached input path: {name}")
    path = root / relative
    if not path.resolve().is_relative_to(root):
        return None
    return path


def identity(path):
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode):
        return None
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    return [digest, stat.S_IMODE(info.st_mode)]


def directory_entries(path):
    return sorted(
        (entry.name, stat.S_IFMT(entry.lstat().st_mode)) for entry in path.iterdir()
    )


def scan(root):
    files = {}
    directories = set()
    for name in tracked_paths(root):
        path = safe_path(root, name)
        if path is None or not path.exists():
            continue
        value = identity(path)
        if value is None:
            continue
        files[name] = {"identity": value, "mtime_ns": path.stat().st_mtime_ns}
        for parent in path.parents:
            if parent == root:
                break
            directories.add(parent)
    return {
        "version": 1,
        "files": files,
        "directories": {
            str(path.relative_to(root)): {
                "entries": directory_entries(path),
                "mtime_ns": path.stat().st_mtime_ns,
            }
            for path in sorted(directories)
        },
    }


def restore(root, saved):
    if saved["version"] != 1:
        raise ValueError("Unsupported input timestamp manifest")
    current = scan(root)
    restored = changed = 0
    now = time.time_ns()
    for name, record in current["files"].items():
        path = safe_path(root, name)
        previous = saved["files"].get(name)
        if previous and record["identity"] == previous["identity"]:
            mtime = previous["mtime_ns"]
            restored += 1
        else:
            # Give changed tracked files the current timestamp, including edits
            # made by tools that preserve their input's timestamp.
            mtime = now
            changed += 1
        os.utime(path, ns=(path.stat().st_atime_ns, mtime), follow_symlinks=False)
    restored_directories = 0
    for name, record in current["directories"].items():
        path = safe_path(root, name)
        previous = saved["directories"].get(name)
        if previous and record["entries"] == [
            tuple(entry) for entry in previous["entries"]
        ]:
            mtime = previous["mtime_ns"]
            restored_directories += 1
        else:
            mtime = now
        os.utime(path, ns=(path.stat().st_atime_ns, mtime), follow_symlinks=False)
    return {
        "restored_files": restored,
        "changed_files": changed,
        "restored_directories": restored_directories,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("operation", choices=["save", "restore"])
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    root = args.root.resolve()
    start = time.monotonic()
    if args.operation == "save":
        data = scan(root)
        args.manifest.parent.mkdir(parents=True, exist_ok=True)
        args.manifest.write_text(json.dumps(data, separators=(",", ":")) + "\n")
        result = {
            "saved_files": len(data["files"]),
            "manifest_bytes": args.manifest.stat().st_size,
        }
    elif args.manifest.exists():
        result = restore(root, json.loads(args.manifest.read_text()))
    else:
        result = {"no_manifest": True}
    print(
        json.dumps(
            result | {"operation": args.operation, "seconds": time.monotonic() - start}
        )
    )


if __name__ == "__main__":
    main()
