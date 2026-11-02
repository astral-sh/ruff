import subprocess
import sys
import tomllib
from pathlib import Path

import pytest

from benchmark.projects import Project
from benchmark.targets import write_cli_targets
from benchmark.tool import Zuban
from benchmark.venv import Venv


@pytest.fixture
def zuban_project(tmp_path: Path) -> tuple[Project, Venv]:
    package = tmp_path / "src" / "example"
    package.mkdir(parents=True)
    (package / "__init__.py").write_text("")
    (package / "main.py").write_text(
        "import sys\n"
        "from dependency import value\n"
        "def untyped():\n"
        "    number: int = value()\n"
        "if sys.version_info >= (3, 12):\n"
        "    newer_python_only: int = 'wrong'\n"
        "def untyped_return():\n"
        "    return 'text'\n"
        "any_return: int = untyped_return()\n"
    )
    tests = package / "tests"
    tests.mkdir()
    (tests / "excluded.py").write_text("this is not valid Python!\n")
    (tmp_path / "outside.py").write_text("this is not valid Python either!\n")

    venv = Venv(project_name="example", project_path=tmp_path)
    venv.bin.mkdir(parents=True)
    venv.python.touch()
    (venv.path / "pyvenv.cfg").write_text(
        "version = 3.11.0\ninclude-system-site-packages = false\n"
    )
    site_packages = (
        venv.path / "Lib" / "site-packages"
        if sys.platform == "win32"
        else venv.path / "lib" / "python3.11" / "site-packages"
    )
    dependency = site_packages / "dependency"
    dependency.mkdir(parents=True)
    (dependency / "py.typed").touch()
    (dependency / "__init__.py").write_text("def value() -> str: return 'text'\n")

    (tmp_path / "pyproject.toml").write_text(
        "# Preserve project metadata and other checker settings.\n"
        '[project]\nname = "example"\n'
        '[tool.mypy]\nignore_errors = true\nexclude = ["src"]\n'
        '[tool.zuban]\nignore_errors = true\npython_version = "3.14"\n'
    )
    (tmp_path / "mypy.ini").write_text("[mypy]\nignore_errors = True\nexclude = src\n")
    project = Project(
        name="example",
        repository="unused",
        revision="unused",
        install_arguments=[],
        python_version="3.11",
        include=["src/example"],
        exclude=["src/**/tests/**"],
    )
    subprocess.run(["git", "init", "-q"], cwd=tmp_path, check=True)
    subprocess.run(["git", "add", "src"], cwd=tmp_path, check=True)
    write_cli_targets(project, tmp_path)
    return project, venv


def test_zuban_cli_checks_implementations_beside_stubs(zuban_project):
    project, venv = zuban_project
    stub = venv.project_path / "src" / "example" / "main.pyi"
    stub.write_text("def untyped() -> None: ...\n")
    subprocess.run(["git", "add", "src"], cwd=venv.project_path, check=True)
    write_cli_targets(project, venv.project_path)
    tool = Zuban()
    tool.write_config(project, venv)
    config = tomllib.loads((venv.project_path / "zuban-benchmark.toml").read_text())
    assert "./src/example/main.py" in config["tool"]["zuban"]["files"]
    assert "./src/example/main.pyi" not in config["tool"]["zuban"]["files"]
    result = subprocess.run(
        tool.command(project, venv, single_threaded=False).command,
        cwd=venv.project_path,
        capture_output=True,
        text=True,
        check=False,
        timeout=20,
    )
    assert result.returncode == 1, result.stderr
    assert 'expression has type "str", variable has type "int"' in result.stdout
    assert "Found 1 error in 1 file" in result.stdout
