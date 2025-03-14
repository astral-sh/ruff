from __future__ import annotations

import abc
import enum
import logging
import os
import shutil
import subprocess
import sys
from pathlib import Path

from benchmark import Command
from benchmark.projects import Project


class Benchmark(enum.Enum):
    """Enumeration of the benchmarks to run."""

    COLD = "cold"
    """Cold check of an entire project without a cache present."""

    WARM = "warm"
    """Re-checking the entire project without any changes"."""


def which_tool(name: str) -> Path:
    tool = shutil.which(name)

    assert tool is not None, (
        f"Tool {name} not found. Run the script with `uv run <script>`."
    )

    return Path(tool)


class Tool(abc.ABC):
    def command(
        self, benchmark: Benchmark, project: Project, venv: Venv
    ) -> Command | None:
        """Generate a command to benchmark a given tool."""
        match benchmark:
            case Benchmark.COLD:
                return self.cold_command(project, venv)
            case Benchmark.WARM:
                return self.warm_command(project, venv)
            case _:
                raise ValueError(f"Invalid benchmark: {benchmark}")

    @abc.abstractmethod
    def cold_command(self, project: Project, venv: Venv) -> Command: ...

    def warm_command(self, project: Project, venv: Venv) -> Command | None:
        return None


class Knot(Tool):
    path: Path
    name: str

    def __init__(self, *, path: Path | None = None):
        self.name = str(path) or "knot"
        self.path = path or (
            (Path(__file__) / "../../../../../target/release/red_knot").resolve()
        )

        assert self.path.is_file(), (
            f"Red Knot not found at '{self.path}'. Run `cargo build --release --bin red_knot`."
        )

    def cold_command(self, project: Project, venv: Venv) -> Command:
        command = [str(self.path), "check", "-v", *project.include]

        command.extend(["--python", str(venv.path)])

        return Command(
            name="knot",
            command=command,
        )


class Mypy(Tool):
    path: Path

    def __init__(self, *, path: Path | None = None):
        self.path = path or which_tool(
            "mypy",
        )

    def cold_command(self, project: Project, venv: Venv) -> Command:
        command = [
            *self._base_command(project, venv),
            "--no-incremental",
            "--cache-dir",
            os.devnull,
        ]

        return Command(
            name="mypy",
            command=command,
        )

    def warm_command(self, project: Project, venv: Venv) -> Command | None:
        command = [
            str(self.path),
            *(project.mypy_arguments or project.include),
            "--python-executable",
            str(venv.python),
        ]

        return Command(
            name="mypy",
            command=command,
        )

    def _base_command(self, project: Project, venv: Venv) -> list[str]:
        return [
            str(self.path),
            "--python-executable",
            str(venv.python),
            *(project.mypy_arguments or project.include),
        ]


class Pyright(Tool):
    path: Path

    def __init__(self, *, path: Path | None = None):
        self.path = path or which_tool("pyright")

    def cold_command(self, project: Project, venv: Venv) -> Command:
        command = [
            str(self.path),
            "--threads",
            "--venvpath",
            str(
                venv.path.parent
            ),  # This is not the path to the venv folder, but the folder that contains the venv...
            *(project.pyright_arguments or project.include),
        ]

        return Command(
            name="Pyright",
            command=command,
        )


class Venv:
    path: Path

    def __init__(self, path: Path):
        self.path = path

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
    def create(parent: Path) -> Venv:
        """Creates a new, empty virtual environment."""

        command = [
            "uv",
            "venv",
            "--quiet",
            "venv",
        ]

        try:
            subprocess.run(
                command, cwd=parent, check=True, capture_output=True, text=True
            )
        except subprocess.CalledProcessError as e:
            raise RuntimeError(f"Failed to create venv: {e.stderr}")

        root = parent / "venv"
        return Venv(root)

    def install(self, dependencies: list[str]) -> None:
        """Installs the dependencies required to type check the project."""

        logging.debug(f"Installing dependencies: {', '.join(dependencies)}")
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
            "2024-09-03T00:00:00Z",
            *dependencies,
        ]

        try:
            subprocess.run(
                command, cwd=self.path, check=True, capture_output=True, text=True
            )
        except subprocess.CalledProcessError as e:
            raise RuntimeError(f"Failed to install dependencies: {e.stderr}")
