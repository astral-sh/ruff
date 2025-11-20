from __future__ import annotations

import abc
import json
import os
import shutil
import sys
from pathlib import Path
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
    def command(self, project: Project, venv: Venv, single_threaded: bool) -> Command:
        """Generate a command to benchmark a given tool."""


class Ty(Tool):
    path: Path
    name: str

    def __init__(self, *, path: Path | None = None):
        self.name = str(path) if path else "ty"
        self.path = (
            path or (Path(__file__) / "../../../../../target/release/ty")
        ).resolve()

        assert self.path.is_file(), (
            f"ty not found at '{self.path}'. Run `cargo build --release --bin ty`."
        )

    @override
    def command(self, project: Project, venv: Venv, single_threaded: bool) -> Command:
        command = [
            str(self.path),
            "check",
            "--output-format=concise",
            "--no-progress",
            "--python-version",
            project.python_version,
            *project.include,
        ]

        for exclude in project.exclude:
            command.extend(["--exclude", exclude])

        command.extend(["--python", str(venv.path)])

        return Command(name=self.name, command=command)


class Mypy(Tool):
    path: Path | None
    warm: bool

    def __init__(self, *, warm: bool, path: Path | None = None):
        self.path = path
        self.warm = warm

    @override
    def command(self, project: Project, venv: Venv, single_threaded: bool) -> Command:
        path = self.path or which_tool("mypy", venv.bin)
        command = [
            str(path),
            "--python-executable",
            str(venv.python),
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


class Pyright(Tool):
    path: Path

    def __init__(self, *, path: Path | None = None):
        if path:
            self.path = path
        else:
            if sys.platform == "win32":
                self.path = Path("./node_modules/.bin/pyright.exe").resolve()
            else:
                self.path = Path("./node_modules/.bin/pyright").resolve()

            if not self.path.exists():
                print(
                    "Pyright executable not found. Did you ran `npm install` in the `ty_benchmark` directory?"
                )

    def command(self, project: Project, venv: Venv, single_threaded: bool) -> Command:
        command = [str(self.path), "--skipunannotated"]

        (venv.project_path / "pyrightconfig.json").write_text(
            json.dumps(
                {
                    "exclude": [str(path) for path in project.exclude],
                    # Set the `venv` config for pyright. Pyright only respects the `--venvpath`
                    # CLI option when `venv` is set in the configuration... 🤷‍♂️
                    "venv": venv.name,
                }
            )
        )

        if not single_threaded:
            command.append("--threads")

        command.extend(
            [
                "--venvpath",
                str(
                    venv.path.parent
                ),  # This is not the path to the venv folder, but the folder that contains the venv...
                "--pythonversion",
                project.python_version,
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


class Pyrefly(Tool):
    path: Path

    def __init__(self, *, path: Path | None = None):
        self.path = path or which_tool("pyrefly")

    @override
    def command(self, project: Project, venv: Venv, single_threaded: bool) -> Command:
        command = [
            str(self.path),
            "check",
            "--python-interpreter-path",
            str(venv.python),
            "--python-version",
            project.python_version,
            "--output-format=min-text",
            "--site-package-path",
            str(
                venv.path.parent
            ),  # This is not the path to the venv folder, but the folder that contains the venv...
            "--untyped-def-behavior=check-and-infer-return-any",
            "--ignore=missing-source-for-stubs",  # not supported by ty
        ]

        for exclude in project.exclude:
            command.extend(["--project-excludes", exclude])

        if single_threaded:
            command.extend(["--threads", "1"])

        command.extend(project.include)

        return Command(
            name="Pyrefly",
            command=command,
        )
