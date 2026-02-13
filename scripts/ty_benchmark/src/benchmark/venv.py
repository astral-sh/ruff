from __future__ import annotations

import logging
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True, kw_only=True, slots=True)
class Venv:
    project_name: str
    project_path: Path

    @property
    def path(self) -> Path:
        return self.project_path / "venv"

    @property
    def name(self) -> str:
        """The name of the virtual environment directory."""
        return self.path.name

    @property
    def python(self) -> Path:
        """Returns the path to the python executable"""
        return self.script("python")

    @property
    def bin(self) -> Path:
        bin_dir = "scripts" if sys.platform == "win32" else "bin"
        return self.path / bin_dir

    def script(self, name: str) -> Path:
        extension = ".exe" if sys.platform == "win32" else ""
        return self.bin / f"{name}{extension}"

    @staticmethod
    def create(*, project: str, parent: Path, python_version: str) -> Venv:
        """Creates a new, empty virtual environment."""

        command = [
            "uv",
            "venv",
            "--quiet",
            "--python",
            python_version,
            "venv",
        ]

        try:
            subprocess.run(
                command, cwd=parent, check=True, capture_output=True, text=True
            )
        except subprocess.CalledProcessError as e:
            msg = f"Failed to create venv for {project}:\n\n{e.stderr}"
            raise RuntimeError(msg) from e

        return Venv(project_name=project, project_path=parent)

    def install(self, pip_install_args: list[str]) -> None:
        """Installs the dependencies required to type check the project."""

        logging.debug(f"Installing dependencies: {', '.join(pip_install_args)}")

        command = [
            "uv",
            "pip",
            "install",
            "--python",
            self.python.as_posix(),
            "--quiet",
            # We pass `--exclude-newer` to ensure that type-checking of one of
            # our projects isn't unexpectedly broken by a change in the
            # annotations of one of that project's dependencies
            "--exclude-newer",
            "2025-12-13T00:00:00Z",
            "mypy",  # We need to install mypy into the virtual environment or it fails to load plugins.
            *pip_install_args,
        ]

        try:
            subprocess.run(
                command,
                cwd=self.project_path,
                check=True,
                capture_output=True,
                text=True,
            )
        except subprocess.CalledProcessError as e:
            msg = (
                f"Failed to install dependencies for {self.project_name}:\n\n{e.stderr}"
            )
            raise RuntimeError(msg) from e
