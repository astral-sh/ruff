import re
import subprocess
import sys
import tomllib
from pathlib import Path

import pytest

from benchmark.projects import Project
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
    return (
        Project(
            name="example",
            repository="unused",
            revision="unused",
            install_arguments=[],
            python_version="3.11",
            include=["src/example"],
            exclude=["src/**/tests/**"],
        ),
        venv,
    )


def test_zuban_checks_scope_untyped_code_and_project_dependencies(zuban_project):
    project, venv = zuban_project
    tool = Zuban()
    config = venv.project_path / "pyproject.toml"
    original = config.read_bytes()
    tool.write_config(project, venv)
    result = subprocess.run(
        tool.command(project, venv, single_threaded=False).command,
        cwd=venv.project_path,
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 1, result.stderr
    assert 'expression has type "str", variable has type "int"' in result.stdout
    assert "Found 1 error in 1 file" in result.stdout
    assert "excluded.py" not in result.stdout
    assert "outside.py" not in result.stdout
    assert "newer_python_only" not in result.stdout
    assert config.read_bytes() == original


@pytest.mark.parametrize(
    "pyproject_newline", [None, b"\n", b"\r\n"], ids=["missing", "lf", "crlf"]
)
def test_zuban_lsp_config_matches_cli_and_restores_project(
    zuban_project, pyproject_newline
):
    project, venv = zuban_project
    tool = Zuban()
    path = venv.project_path / "pyproject.toml"
    if pyproject_newline is None:
        path.unlink()
    else:
        path.write_bytes(
            path.read_bytes().replace(b"\r\n", b"\n").replace(b"\n", pyproject_newline)
        )
    original = path.read_bytes() if path.exists() else None
    mypy_ini = (venv.project_path / "mypy.ini").read_bytes()
    tool.write_config(project, venv)
    cli = subprocess.run(
        tool.command(project, venv, single_threaded=False).command,
        cwd=venv.project_path,
        capture_output=True,
        text=True,
        check=False,
    )
    with (
        pytest.raises(RuntimeError, match="benchmark failed"),
        tool.configure_lsp(project, venv),
    ):
        discovered = subprocess.run(
            [
                str(tool.path),
                "check",
                "--mode",
                "mypy",
                "--no-pretty",
                *project.include,
            ],
            cwd=venv.project_path,
            capture_output=True,
            text=True,
            check=False,
        )
        assert discovered.returncode == cli.returncode == 1
        assert discovered.stdout == cli.stdout
        raise RuntimeError("benchmark failed")
    assert (path.read_bytes() if path.exists() else None) == original
    assert (venv.project_path / "mypy.ini").read_bytes() == mypy_ini


@pytest.mark.parametrize(
    ("pattern", "excluded", "included"),
    [
        ("numpy/tests/**", "numpy/tests/test_array.py", "numpy/_core/array.py"),
        (
            "numpy/**/tests/**",
            "numpy/_core/tests/test_array.py",
            "numpy/_core/array.py",
        ),
        ("pandas/tests", "pandas/tests/test_frame.py", "pandas/core/frame.py"),
        (
            "torch/_inductor/codegen/triton.py",
            "torch/_inductor/codegen/triton.py",
            "torch/_inductor/codegen/tritonxpy",
        ),
    ],
)
def test_zuban_exclusions_follow_project_globs(
    zuban_project, pattern, excluded, included
):
    project, venv = zuban_project
    _, content = Zuban().config(project._replace(exclude=[pattern]), venv)
    regex = tomllib.loads(content)["tool"]["zuban"]["exclude"][0]
    assert re.search(regex, excluded)
    assert re.search(regex, excluded.replace("/", "\\"))
    assert not re.search(regex, included)
