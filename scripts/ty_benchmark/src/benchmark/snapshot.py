from __future__ import annotations

import difflib
import logging
import re
import subprocess
from pathlib import Path
from typing import Mapping, NamedTuple

from benchmark import Command


def normalize_output(output: str, cwd: Path) -> str:
    """Normalize output by replacing absolute paths with relative placeholders."""

    # Replace absolute paths with <CWD>/relative/path.
    # This handles both the cwd itself and any paths within it.
    cwd_str = str(cwd.resolve())
    normalized = output.replace(cwd_str, "<CWD>")

    # Replace temp directory patterns (e.g., /var/folders/.../tmp123abc/).
    # Match common temp directory patterns across platforms.
    normalized = re.sub(
        r"/(?:var/folders|tmp)/[a-zA-Z0-9/_-]+/tmp[a-zA-Z0-9]+",
        "<TMPDIR>",
        normalized,
    )

    # Normalize unordered lists in error messages (e.g., '"name", "description"' vs '"description", "name"').
    # Sort quoted comma-separated items to make them deterministic.
    def sort_quoted_items(match):
        items = match.group(0)
        quoted_items = re.findall(r'"[^"]+"', items)
        if len(quoted_items) > 1:
            sorted_items = ", ".join(sorted(quoted_items))
            return sorted_items
        return items

    normalized = re.sub(r'"[^"]+"(?:, "[^"]+")+', sort_quoted_items, normalized)

    return normalized


class SnapshotRunner(NamedTuple):
    name: str
    """The benchmark to run."""

    commands: list[Command]
    """The commands to snapshot."""

    snapshot_dir: Path
    """Directory to store snapshots."""

    accept: bool
    """Whether to accept snapshot changes."""

    def run(self, *, cwd: Path, env: Mapping[str, str]):
        """Run commands and snapshot their output."""
        # Create snapshot directory if it doesn't exist.
        self.snapshot_dir.mkdir(parents=True, exist_ok=True)

        all_passed = True

        for command in self.commands:
            snapshot_file = self.snapshot_dir / f"{self.name}_{command.name}.txt"

            # Run the prepare command if provided.
            if command.prepare:
                logging.info(f"Running prepare: {command.prepare}")
                subprocess.run(
                    command.prepare,
                    shell=True,
                    cwd=cwd,
                    env=env,
                    capture_output=True,
                    check=True,
                )

            # Run the actual command and capture output.
            logging.info(f"Running {command.command}")
            result = subprocess.run(
                command.command,
                cwd=cwd,
                env=env,
                capture_output=True,
                text=True,
            )

            # Get the actual output and combine stdout and stderr for the snapshot.
            actual_output = normalize_output(result.stdout + result.stderr, cwd)

            # Check if snapshot exists.
            if snapshot_file.exists():
                expected_output = snapshot_file.read_text()

                if actual_output != expected_output:
                    all_passed = False
                    print(f"\n❌ Snapshot mismatch for {command.name}")
                    print(f"   Snapshot file: {snapshot_file}")
                    print("\n" + "=" * 80)
                    print("Diff:")
                    print("=" * 80)

                    # Generate and display a unified diff.
                    diff = difflib.unified_diff(
                        expected_output.splitlines(keepends=True),
                        actual_output.splitlines(keepends=True),
                        fromfile=f"{command.name} (expected)",
                        tofile=f"{command.name} (actual)",
                        lineterm="",
                    )

                    for line in diff:
                        # Color-code the diff output.
                        if line.startswith("+") and not line.startswith("+++"):
                            print(f"\033[32m{line}\033[0m", end="")
                        elif line.startswith("-") and not line.startswith("---"):
                            print(f"\033[31m{line}\033[0m", end="")
                        elif line.startswith("@@"):
                            print(f"\033[36m{line}\033[0m", end="")
                        else:
                            print(line, end="")

                    print("\n" + "=" * 80)

                    if self.accept:
                        print(f"✓ Accepting changes for {command.name}")
                        snapshot_file.write_text(actual_output)
                else:
                    print(f"✓ Snapshot matches for {command.name}")
            else:
                # No existing snapshot - create it.
                print(f"📸 Creating new snapshot for {command.name}")
                snapshot_file.write_text(actual_output)

        if not all_passed and not self.accept:
            print("\n⚠️  Some snapshots don't match. Run with --accept to update them.")
