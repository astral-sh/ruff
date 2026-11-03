import subprocess
import sys
import tomllib
import venv as stdlib_venv
from pathlib import Path

import pytest

from benchmark import benchmark_environment
from benchmark.projects import Project
from benchmark.targets import write_cli_targets
from benchmark.tool import Mypy, Pyrefly, Pyright, Ty, Zuban, which_tool
from benchmark.venv import Venv


@pytest.mark.parametrize("name", ["ty", "pyrefly", "mypy", "pyright", "zuban"])
def test_cli_preserves_imported_stub_types(tmp_path: Path, name: str):
    files = {
        "pkg/__init__.py": "",
        "pkg/api.py": "from ._native import typed_value\n",
        "pkg/api.pyi": "def typed_value() -> int: ...\n",
        "pkg/consumer.py": ("from .api import typed_value\nbad: str = typed_value()\n"),
        "pkg/plain.py": 'def untyped():\n    number: int = "wrong"\n',
        "pkg/only.pyi": "value: MissingType\n",
        "pkg/excluded.py": "this is not valid Python!\n",
        "pkg/sub.pyi": "this is not valid Python!\n",
        "pkg/sub/__init__.pyi": "",
        "pkg/notebook.ipynb": "this is not a valid notebook!\n",
    }
    for filename, contents in files.items():
        path = tmp_path / filename
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents)
    subprocess.run(["git", "init", "-q"], cwd=tmp_path, check=True)
    subprocess.run(["git", "add", "pkg"], cwd=tmp_path, check=True)
    (tmp_path / "pkg/generated").mkdir()
    (tmp_path / "pkg/generated/untracked.py").write_text("this is not valid Python!\n")
    project = Project(
        name="stub-imports",
        repository="unused",
        revision="unused",
        install_arguments=[],
        python_version=f"{sys.version_info.major}.{sys.version_info.minor}",
        include=["pkg"],
        exclude=["pkg/excluded.py"],
    )
    venv = Venv(project_name=project.name, project_path=tmp_path)
    stdlib_venv.EnvBuilder().create(venv.path)
    project = write_cli_targets(project, tmp_path)
    tools = {
        "ty": Ty,
        "pyrefly": Pyrefly,
        "mypy": lambda: Mypy(warm=False, path=which_tool("mypy")),
        "pyright": Pyright,
        "zuban": Zuban,
    }
    tool = tools[name]()
    tool.write_config(project, venv)
    result = subprocess.run(
        tool.command(project, venv, single_threaded=False).command,
        cwd=tmp_path,
        capture_output=True,
        text=True,
        check=False,
        timeout=30,
    )
    output = result.stdout + result.stderr
    assert result.returncode == 1, output
    # Losing the stub's return type makes this assignment Any -> str in mypy.
    consumer_diagnostics = [
        line for line in output.splitlines() if "consumer.py:" in line
    ]
    assert any("assign" in line.lower() for line in consumer_diagnostics), output
    assert "plain.py" in output, output
    assert "only.pyi" in output, output
    assert "excluded.py" not in output, output
    assert "untracked.py" not in output, output
    assert "sub.pyi" not in output, output
    assert "api.py:" not in output, output
    assert "notebook.ipynb" not in output, output


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
    project = write_cli_targets(project, tmp_path)
    return project, venv


def test_ty_cli_checks_source_paths_with_spaces(zuban_project):
    project, venv = zuban_project
    root = venv.project_path
    with (venv.path / "pyvenv.cfg").open("a") as config:
        config.write(f"home = {Path(sys.executable).resolve().parent}\n")
    (root / "src").rename(root / "source root")
    (root / "source root/example/main.py").write_text(
        'def untyped():\n    number: int = "wrong"\n'
    )
    project = project._replace(
        include=["source root/example"], exclude=["source root/**/tests/**"]
    )
    subprocess.run(["git", "add", "--all"], cwd=root, check=True)
    project = write_cli_targets(project, root)
    tool = Ty()
    tool.write_config(project, venv)
    result = subprocess.run(
        tool.command(project, venv, single_threaded=False).command,
        cwd=root,
        capture_output=True,
        text=True,
        check=False,
        timeout=20,
    )
    assert result.returncode == 1, result.stderr
    assert "source root/example/main.py" in result.stdout
    assert "invalid-assignment" in result.stdout
    assert "excluded.py" not in result.stdout
    assert "outside.py" not in result.stdout


def test_benchmark_environment_isolates_user_config(tmp_path, monkeypatch):
    user_config = tmp_path / "user-config" / "ty"
    user_config.mkdir(parents=True)
    (user_config / "ty.toml").write_text('[rules]\nall = "ignore"\n')
    monkeypatch.setenv("XDG_CONFIG_HOME", str(user_config.parent))
    monkeypatch.setenv("APPDATA", str(user_config.parent))
    checkout = tmp_path / "project"
    checkout.mkdir()
    (checkout / "main.py").write_text('value: int = "wrong"\n')
    tool = Ty()
    project = Project(
        name="user-config",
        repository="",
        revision="",
        install_arguments=[],
        python_version="3.15",
        include=["main.py"],
    )
    venv = Venv(project_name=project.name, project_path=checkout)
    stdlib_venv.EnvBuilder().create(venv.path)
    tool.write_config(project, venv)
    command = tool.command(project, venv, single_threaded=False).command
    inherited = subprocess.run(
        command, cwd=checkout, capture_output=True, text=True, check=False
    )
    isolated = subprocess.run(
        command,
        cwd=checkout,
        env=benchmark_environment(checkout),
        capture_output=True,
        text=True,
        check=False,
    )
    assert inherited.returncode == 0, inherited.stderr
    assert isolated.returncode == 1, isolated.stderr
    assert "invalid-assignment" in isolated.stdout
    assert (user_config / "ty.toml").read_text() == '[rules]\nall = "ignore"\n'


def test_zuban_cli_prefers_stubs_beside_implementations(zuban_project):
    project, venv = zuban_project
    stub = venv.project_path / "src" / "example" / "main.pyi"
    stub.write_text("def untyped() -> None: ...\nvalue: MissingType\n")
    subprocess.run(["git", "add", "src"], cwd=venv.project_path, check=True)
    project = write_cli_targets(project, venv.project_path)
    tool = Zuban()
    tool.write_config(project, venv)
    config = tomllib.loads((venv.project_path / "zuban-benchmark.toml").read_text())
    assert config["tool"]["zuban"]["files"] == list(project.include)
    assert "src/example/main.py" in project.exclude
    result = subprocess.run(
        tool.command(project, venv, single_threaded=False).command,
        cwd=venv.project_path,
        capture_output=True,
        text=True,
        check=False,
        timeout=20,
    )
    assert result.returncode == 1, result.stderr
    assert "main.pyi" in result.stdout
    assert 'Name "MissingType" is not defined' in result.stdout
    assert "Found 1 error in 1 file" in result.stdout
