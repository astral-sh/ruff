from __future__ import annotations

import abc
import json
import os
import shutil
import sys
from pathlib import Path
from textwrap import dedent
from typing import TYPE_CHECKING, override

from benchmark import Command
from benchmark.projects import Project

if TYPE_CHECKING:
    from benchmark.venv import Venv


def which_tool(name: str, path: Path | None = None) -> Path:
    tool = shutil.which(name, path=path)

    assert tool is not None, (
        f"Tool {name} not found. Run the script with `uv run <script>`."
    )

    return Path(tool)


class Tool(abc.ABC):
    @abc.abstractmethod
    def name(self) -> str: ...

    def write_config(self, project: Project, venv: Venv) -> None:
        """Write the tool's configuration file."""

        if config := self.config(project, venv):
            config_name, config_text = config
            config_path = venv.project_path / config_name
            config_path.write_text(dedent(config_text))

    def config(self, project: Project, venv: Venv) -> tuple[Path, str] | None:
        """Returns the path to the tool's configuration file with the configuration
        content or `None` if the tool requires no configuration file.

        We write a configuration over using CLI arguments because
        most LSPs don't accept per CLI.
        """
        return None

    @abc.abstractmethod
    def command(self, project: Project, venv: Venv, single_threaded: bool) -> Command:
        """Generate a command to benchmark a given tool."""

    @abc.abstractmethod
    def lsp_command(self, project: Project, venv: Venv) -> list[str] | None:
        """Generate command to start LSP server, or None if not supported."""


class Ty(Tool):
    path: Path
    _name: str

    def __init__(self, *, path: Path | None = None):
        self._name = str(path) if path else "ty"
        executable = "ty.exe" if sys.platform == "win32" else "ty"
        self.path = (
            path or (Path(__file__) / "../../../../../target/release" / executable)
        ).resolve()

        assert self.path.is_file(), (
            f"ty not found at '{self.path}'. Run `cargo build --release --bin ty`."
        )

    @override
    def name(self) -> str:
        return self._name

    @override
    def config(self, project: Project, venv: Venv):
        return (
            Path("ty.toml"),
            f"""
            [src]
            include = [{", ".join([f'"{include}"' for include in project.include])}]
            exclude = [{", ".join([f'"{exclude}"' for exclude in project.exclude])}]

            [environment]
            python-version = "{project.python_version}"
            python = "{venv.path.as_posix()}"
            """,
        )

    @override
    def command(self, project: Project, venv: Venv, single_threaded: bool) -> Command:
        command = [
            str(self.path),
            "check",
            "--output-format=concise",
            "--no-progress",
        ]

        for exclude in project.exclude:
            command.extend(["--exclude", exclude])

        return Command(name=self._name, command=command)

    @override
    def lsp_command(self, project: Project, venv: Venv) -> list[str] | None:
        return [str(self.path), "server"]


class Mypy(Tool):
    path: Path | None
    warm: bool

    def __init__(self, *, warm: bool, path: Path | None = None):
        self.path = path
        self.warm = warm

    @override
    def name(self) -> str:
        return "mypy"

    @override
    def command(self, project: Project, venv: Venv, single_threaded: bool) -> Command:
        path = self.path or which_tool("mypy", venv.bin)
        command = [
            str(path),
            "--python-executable",
            str(venv.python.as_posix()),
            "--python-version",
            project.python_version,
            "--no-pretty",
            *project.include,
            "--check-untyped-defs",
        ]

        for exclude in project.exclude:
            # Mypy uses regex...
            # This is far from perfect, but not terrible.
            command.extend(
                [
                    "--exclude",
                    exclude.replace(".", r"\.")
                    .replace("**", ".*")
                    .replace("*", r"\w.*"),
                ]
            )

        if not self.warm:
            command.extend(
                [
                    "--no-incremental",
                    "--cache-dir",
                    os.devnull,
                ]
            )

        return Command(
            name="mypy (warm)" if self.warm else "mypy",
            command=command,
        )

    @override
    def lsp_command(self, project: Project, venv: Venv) -> list[str] | None:
        # Mypy doesn't have official LSP support.
        return None


class Pyright(Tool):
    path: Path
    lsp_path: Path

    def __init__(self, *, path: Path | None = None):
        if path:
            self.path = path
            # Assume langserver is in the same directory.
            if sys.platform == "win32":
                self.lsp_path = path.with_name("pyright-langserver.cmd")
            else:
                self.lsp_path = path.with_name("pyright-langserver")
        else:
            self.path = npm_bin_path("pyright")
            self.lsp_path = npm_bin_path("pyright-langserver")

            if not self.path.exists():
                print(
                    "Pyright executable not found. Did you run `npm ci` in the `ty_benchmark` directory?"
                )

    @override
    def name(self) -> str:
        return "pyright"

    @override
    def config(self, project: Project, venv: Venv):
        return (
            Path("pyrightconfig.json"),
            json.dumps(
                {
                    "exclude": [str(path) for path in project.exclude],
                    # Set the `venv` config for pyright. Pyright only respects the `--venvpath`
                    # CLI option when `venv` is set in the configuration... 🤷‍♂️
                    "venv": venv.name,
                    # This is not the path to the venv folder, but the folder that contains the venv...
                    "venvPath": str(venv.path.parent.as_posix()),
                    "pythonVersion": project.python_version,
                }
            ),
        )

    def command(self, project: Project, venv: Venv, single_threaded: bool) -> Command:
        command = [str(self.path), "--skipunannotated"]

        if not single_threaded:
            command.append("--threads")

        command.extend(
            [
                "--level=warning",
                "--project",
                "pyrightconfig.json",
                *project.include,
            ]
        )

        return Command(
            name="Pyright",
            command=command,
        )

    @override
    def lsp_command(self, project: Project, venv: Venv) -> list[str] | None:
        # Pyright LSP server is a separate executable.
        return [str(self.lsp_path), "--stdio"]


class Pyrefly(Tool):
    path: Path

    def __init__(self, *, path: Path | None = None):
        self.path = path or which_tool("pyrefly")

    @override
    def name(self) -> str:
        return "pyrefly"

    @override
    def config(self, project: Project, venv: Venv):
        return (
            Path("pyrefly.toml"),
            f"""
            project-includes = [{", ".join([f'"{include}"' for include in project.include])}]
            project-excludes = [{", ".join([f'"{exclude}"' for exclude in project.exclude])}]
            python-interpreter-path = "{venv.python.as_posix()}"
            python-version = "{project.python_version}"
            site-package-path = ["{venv.path.as_posix()}"]
            ignore-missing-source = true
            untyped-def-behavior="check-and-infer-return-any"
            """,
        )

    @override
    def command(self, project: Project, venv: Venv, single_threaded: bool) -> Command:
        command = [
            str(self.path),
            "check",
            "--output-format=min-text",
        ]

        if single_threaded:
            command.extend(["--threads", "1"])

        return Command(
            name="Pyrefly",
            command=command,
        )

    @override
    def lsp_command(self, project: Project, venv: Venv) -> list[str] | None:
        # Pyrefly LSP server.
        # Turn-off pyrefly's indexing mode as it results in significant load after opening the first file,
        # skewing benchmark results and we don't use any of the features that require indexing.
        return [str(self.path), "lsp", "--indexing-mode", "none"]


def npm_bin_path(name: str) -> Path:
    if sys.platform == "win32":
        return Path(f"./node_modules/.bin/{name}.cmd").resolve()

    else:
        return (Path("./node_modules/.bin") / name).resolve()
