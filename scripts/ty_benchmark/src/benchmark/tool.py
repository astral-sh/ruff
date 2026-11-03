from __future__ import annotations

import abc
import glob
import json
import re
import shutil
import sys
from collections.abc import Generator
from contextlib import contextmanager
from pathlib import Path
from textwrap import dedent
from typing import TYPE_CHECKING, override

import tomlkit

from benchmark import Command
from benchmark.projects import Project
from benchmark.targets import PYRIGHT_CONFIG

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

    @contextmanager
    def configure_lsp(self, project: Project, venv: Venv) -> Generator[None]:
        self.write_config(project, venv)
        yield

    def lsp_initialization_options(
        self, project: Project, venv: Venv
    ) -> dict[str, str] | None:
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
            tomlkit.dumps(
                {
                    "src": {
                        "include": [
                            path.replace(" ", r"\ ") for path in project.include
                        ],
                        "exclude": [
                            path.replace(" ", r"\ ") for path in project.exclude
                        ],
                    },
                    "environment": {
                        "python-version": project.python_version,
                        "python": venv.path.as_posix(),
                    },
                }
            ),
        )

    @override
    def command(self, project: Project, venv: Venv, single_threaded: bool) -> Command:
        command = [
            str(self.path),
            "check",
            "--output-format=concise",
            "--no-progress",
        ]

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
            # An empty config path disables mypy's project and user config discovery.
            "--config-file=",
            "--follow-imports=normal",
            "--check-untyped-defs",
            *project.include,
        ]

        if project.exclude:
            command.extend(
                [
                    "--exclude",
                    r"\A(?:"
                    + "|".join(re.escape(path) for path in project.exclude)
                    + r")(?:[/\\]|\Z)",
                ]
            )

        prepare = None

        if not self.warm:
            cache_dir = ".mypy_cache_benchmark"
            command.extend(
                [
                    "--cache-dir",
                    cache_dir,
                ]
            )
            prepare = [
                sys.executable,
                "-c",
                f"import shutil; shutil.rmtree({cache_dir!r}, ignore_errors=True)",
            ]

        return Command(
            name="mypy (warm)" if self.warm else "mypy",
            command=command,
            prepare=prepare,
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
        command = [str(self.path)]

        if not single_threaded:
            command.append("--threads")

        command.extend(
            [
                "--level=warning",
                "--project",
                PYRIGHT_CONFIG,
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
            tomlkit.dumps(
                {
                    "project-includes": list(project.include),
                    "project-excludes": list(project.exclude),
                    "python-interpreter-path": venv.python.as_posix(),
                    "python-version": project.python_version,
                    "site-package-path": [venv.path.as_posix()],
                    "untyped-def-behavior": "check-and-infer-return-any",
                }
            ),
        )

    @override
    def command(self, project: Project, venv: Venv, single_threaded: bool) -> Command:
        command = [
            str(self.path),
            "check",
            "--output-format=min-text",
            "--config=pyrefly.toml",
            "--disable-project-excludes-heuristics=true",
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


class Zuban(Tool):
    def __init__(self, *, path: Path | None = None):
        self.path = path or which_tool("zuban")

    @override
    def name(self) -> str:
        return "zuban"

    def _settings(self, project: Project, venv: Venv) -> dict[str, object]:
        search_paths = set()
        for include in project.include:
            path = venv.project_path / include
            if path.is_file():
                path = path.parent
            while path != venv.project_path and (
                (path / "__init__.py").is_file() or (path / "__init__.pyi").is_file()
            ):
                path = path.parent
            search_paths.add(path)

        # Zuban matches exclusions relative to each source import root.
        excludes = list(project.exclude)
        for path in sorted(search_paths):
            relative_root = path.relative_to(venv.project_path).as_posix()
            if relative_root == ".":
                continue
            for pattern in project.exclude:
                if pattern == relative_root:
                    excludes.append("**")
                elif pattern.startswith(relative_root + "/"):
                    excludes.append(pattern[len(relative_root) + 1 :])

        return {
            "mode": "mypy",
            "python_version": project.python_version,
            "python_executable": venv.python.as_posix(),
            "mypy_path": [path.as_posix() for path in sorted(search_paths)],
            "files": list(project.include),
            "exclude": [
                r"\A"
                + glob.translate(
                    pattern, recursive=True, include_hidden=True, seps="/\\"
                ).removesuffix(r"\z")
                + r"(?:[/\\].*)?\z"
                for pattern in dict.fromkeys(excludes)
            ],
            "exclude_gitignore": False,
            "check_untyped_defs": True,
            "untyped_function_return_mode": "any",
        }

    @override
    def config(self, project: Project, venv: Venv) -> tuple[Path, str]:
        return (
            Path("zuban-benchmark.toml"),
            tomlkit.dumps({"tool": {"zuban": self._settings(project, venv)}}),
        )

    @override
    def command(self, project: Project, venv: Venv, single_threaded: bool) -> Command:
        return Command(
            name="Zuban",
            command=[
                str(self.path),
                "check",
                "--config-file",
                "zuban-benchmark.toml",
                "--no-pretty",
            ],
        )

    @override
    @contextmanager
    def configure_lsp(self, project: Project, venv: Venv) -> Generator[None]:
        path = venv.project_path / "pyproject.toml"
        original = path.read_bytes() if path.exists() else None
        document = (
            tomlkit.parse(original.decode())
            if original is not None
            else tomlkit.document()
        )
        tool = document.setdefault("tool", tomlkit.table())
        # An empty mypy table stops Zuban's search before mypy.ini/setup.cfg.
        # Its own settings are applied afterwards, matching the isolated CLI config.
        tool["mypy"] = tomlkit.table()
        tool["zuban"] = self._settings(project, venv)
        try:
            path.write_bytes(tomlkit.dumps(document).encode())
            yield
        finally:
            if original is None:
                path.unlink(missing_ok=True)
            else:
                path.write_bytes(original)

    @override
    def lsp_initialization_options(
        self, project: Project, venv: Venv
    ) -> dict[str, str]:
        return {
            "pythonExecutable": venv.python.as_posix(),
            "typeCheckingMode": "mypy",
            "diagnosticMode": "open-files-only",
        }

    @override
    def lsp_command(self, project: Project, venv: Venv) -> list[str]:
        return [str(self.path), "server"]


def npm_bin_path(name: str) -> Path:
    if sys.platform == "win32":
        return Path(f"./node_modules/.bin/{name}.cmd").resolve()

    else:
        return (Path("./node_modules/.bin") / name).resolve()
