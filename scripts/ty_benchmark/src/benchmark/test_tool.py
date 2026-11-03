import asyncio
import re
import subprocess
import sys
import tomllib
import venv as stdlib_venv
from pathlib import Path
from unittest.mock import AsyncMock, Mock

import pytest
from lsprotocol import types as lsp

from benchmark import Command, benchmark_environment
from benchmark.lsp_client import FileDiagnostics, LSPClient
from benchmark.preflight import validate_cli_targets
from benchmark.projects import IncrementalEdit, Project
from benchmark.targets import neutralize_file_directives, write_cli_targets
from benchmark.test_lsp_diagnostics import FetchDiagnostics
from benchmark.tool import Mypy, Pyrefly, Pyright, Ty, Zuban, which_tool
from benchmark.venv import Venv


@pytest.mark.parametrize("name", ["ty", "pyrefly", "mypy", "pyright", "zuban"])
@pytest.mark.parametrize("layout", ["", "lib", "src"])
def test_cli_preserves_imported_stub_types(tmp_path: Path, name: str, layout: str):
    files = {
        "pkg/__init__.py": "",
        "pkg/api.py": "from ._native import typed_value\n",
        "pkg/api.pyi": "def typed_value() -> int: ...\n",
        "pkg/consumer.py": (
            "from pkg.api import typed_value\nbad: str = typed_value()\n"
        ),
        "pkg/plain.py": '# mypy: no-check-untyped-defs\n# pyrefly: ignore-errors\ndef untyped():\n    number: int = "wrong"\n',
        "pkg/only.pyi": "value: MissingType\n",
        "pkg/excluded.py": "this is not valid Python!\n",
        "pkg/sub.pyi": "this is not valid Python!\n",
        "pkg/sub/__init__.pyi": "",
        "pkg/notebook.ipynb": "this is not a valid notebook!\n",
        "pkg/native.pyi": "value: int\n",
        "pkg/native/build.py": "this is not valid Python!\n",
    }
    for filename, contents in files.items():
        path = tmp_path / layout / filename
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents)
    subprocess.run(["git", "init", "-q"], cwd=tmp_path, check=True)
    subprocess.run(["git", "add", "."], cwd=tmp_path, check=True)
    (tmp_path / "pyproject.toml").write_text(
        '[tool.mypy]\nexclude = ["pkg"]\n'
        '[[tool.mypy.overrides]]\nmodule = ["pkg.*"]\n'
        "check_untyped_defs = false\n"
    )
    (tmp_path / layout / "pkg/generated").mkdir()
    (tmp_path / layout / "pkg/generated/untracked.py").write_text(
        "this is not valid Python!\n"
    )
    project = Project(
        name="stub-imports",
        repository="unused",
        revision="unused",
        install_arguments=[],
        python_version=f"{sys.version_info.major}.{sys.version_info.minor}",
        include=[(Path(layout) / "pkg").as_posix()],
        exclude=[(Path(layout) / "pkg/excluded.py").as_posix()],
        import_roots=[layout] if layout else [],
    )
    venv = Venv(project_name=project.name, project_path=tmp_path)
    stdlib_venv.EnvBuilder().create(venv.path)
    neutralize_file_directives(tmp_path)
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
        env=benchmark_environment(tmp_path),
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
    assert "build.py" not in output, output
    validate_cli_targets(
        tool, tool.command(project, venv, False), venv, benchmark_environment(tmp_path)
    )


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
        '[[tool.mypy.overrides]]\nmodule = ["example.*"]\n'
        "check_untyped_defs = false\n"
        '[tool.zuban]\nignore_errors = true\npython_version = "3.14"\n'
    )
    (tmp_path / "mypy.ini").write_text(
        "[mypy]\nignore_errors = True\nexclude = src\n"
        "[mypy-example.*]\ncheck_untyped_defs = False\n"
    )
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


@pytest.mark.parametrize("layout", ["", "lib", "src"])
def test_zuban_canonicalizes_symlinked_import_roots(zuban_project, layout):
    project, venv = zuban_project
    root = venv.project_path
    source_root = root / layout
    if layout != "src":
        source_root.mkdir(exist_ok=True)
        (root / "src/example").rename(source_root / "example")
    (root / "source-alias").symlink_to(source_root, target_is_directory=True)
    checkout_alias = root.with_name(root.name + "-alias")
    checkout_alias.symlink_to(root, target_is_directory=True)
    venv = Venv(project_name=project.name, project_path=checkout_alias)
    package = source_root / "example"
    (package / "settings.py").write_text("class Config: pass\n")
    (package / "worker.py").write_text(
        "from .settings import Config\ndef check(config: Config) -> None: pass\n"
    )
    (package / "main.py").write_text(
        "from example.settings import Config\nfrom .worker import check\n"
        'check(Config())\nnumber: int = "wrong"\n'
    )
    project = project._replace(
        include=[(Path(layout) / "example").as_posix()],
        exclude=[(Path(layout) / "example/tests/**").as_posix()],
        import_roots=[layout or ".", "source-alias"],
    )
    tool = Zuban()
    tool.write_config(project, venv)
    config = tomllib.loads((root / "zuban-benchmark.toml").read_text())["tool"]["zuban"]
    assert config["mypy_path"] == [source_root.resolve().as_posix()]
    assert any(
        re.search(pattern, "example/tests/excluded.py") for pattern in config["exclude"]
    )
    with tool.configure_lsp(project, venv):
        assert (
            tomllib.loads((root / "pyproject.toml").read_text())["tool"]["zuban"]
            == config
        )
    result = subprocess.run(
        tool.command(project, venv, single_threaded=False).command,
        cwd=checkout_alias,
        env=benchmark_environment(checkout_alias),
        capture_output=True,
        text=True,
        check=False,
        timeout=20,
    )
    assert result.returncode == 1, result.stdout + result.stderr
    assert 'expression has type "str", variable has type "int"' in result.stdout
    assert "Found 1 error in 1 file" in result.stdout


def test_lsp_uses_physical_checkout_paths(zuban_project, monkeypatch, capsys):
    project, venv = zuban_project
    root = venv.project_path.resolve()
    alias = root.with_name(root.name + "-alias")
    alias.symlink_to(root, target_is_directory=True)
    venv = Venv(project_name=project.name, project_path=alias)
    project = project._replace(
        edit=IncrementalEdit(
            edited_file="src/example/main.py",
            affected_files=["src/example/__init__.py"],
            replace_text="text",
            replacement="replacement",
        )
    )
    test = FetchDiagnostics(project, Zuban(), venv)
    start_io = AsyncMock()
    initialize = AsyncMock()
    opened = Mock()
    monkeypatch.setattr(test.client, "start_io", start_io)
    monkeypatch.setattr(test.client, "initialize_async", initialize)
    monkeypatch.setattr(test.client, "initialized", Mock())
    monkeypatch.setattr(test.client, "text_document_did_open", opened)
    asyncio.run(test.setup())
    assert test.cwd == root
    assert test.edited_file_path == root / "src/example/main.py"
    assert start_io.await_args is not None
    assert start_io.await_args.kwargs["cwd"] == root
    assert initialize.await_args is not None
    params = initialize.await_args.args[0]
    assert params.root_uri == root.as_uri()
    assert [folder.uri for folder in params.workspace_folders] == [root.as_uri()]
    assert [call.args[0].text_document.uri for call in opened.call_args_list] == [
        (root / "src/example/main.py").as_uri(),
        (root / "src/example/__init__.py").as_uri(),
    ]
    test.diagnostics = [
        FileDiagnostics(
            test.edited_file_path,
            [
                lsp.Diagnostic(
                    range=lsp.Range(
                        start=lsp.Position(line=0, character=0),
                        end=lsp.Position(line=0, character=1),
                    ),
                    message="test diagnostic",
                )
            ],
        )
    ]
    test.assert_output(verbose=True)
    assert "src/example/main.py:1:1" in capsys.readouterr().out


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
    monkeypatch.setenv("TY_CONFIG_FILE", str(user_config / "ty.toml"))
    monkeypatch.setenv("MYPY_NUM_WORKERS", "123")
    checkout = tmp_path / "project"
    checkout.mkdir()
    (checkout / "main.py").write_text('value: int = "wrong"\n')
    tool = Ty()
    project = Project(
        name="user-config",
        repository="",
        revision="",
        install_arguments=[],
        python_version="3.14",
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
    assert "TY_CONFIG_FILE" not in benchmark_environment(checkout)
    assert "MYPY_NUM_WORKERS" not in benchmark_environment(checkout)


def test_cli_preflight_rejects_zero_roots(tmp_path, monkeypatch):
    (tmp_path / "benchmark-files.txt").write_text("./main.py\n")
    (tmp_path / "main.py").write_text('value: int = "wrong"\n')
    tool = Ty()
    command = Command(name="ty", command=[str(tool.path), "check"])
    venv = Venv(project_name="zero-roots", project_path=tmp_path)
    monkeypatch.setattr(
        "benchmark.preflight.subprocess.run",
        lambda *args, **kwargs: subprocess.CompletedProcess(
            args[0], 0, "", "Indexed 0 files"
        ),
    )
    with pytest.raises(ValueError, match="discovered 0 roots; expected 1"):
        validate_cli_targets(tool, command, venv, benchmark_environment(tmp_path))


@pytest.mark.parametrize("name", ["ty", "pyrefly"])
def test_cli_does_not_respect_git_info_exclude(tmp_path, name):
    (tmp_path / "main.py").write_text('value: int = "wrong"\n')
    subprocess.run(["git", "init", "-q"], cwd=tmp_path, check=True)
    subprocess.run(["git", "add", "main.py"], cwd=tmp_path, check=True)
    (tmp_path / ".git/info/exclude").write_text("main.py\n")
    project = Project(
        name="ignore-files",
        repository="",
        revision="",
        install_arguments=[],
        python_version="3.14",
        include=["main.py"],
    )
    venv = Venv(project_name=project.name, project_path=tmp_path)
    stdlib_venv.EnvBuilder().create(venv.path)
    project = write_cli_targets(project, tmp_path)
    tool = Ty() if name == "ty" else Pyrefly()
    tool.write_config(project, venv)
    command = tool.command(project, venv, False)
    result = subprocess.run(
        command.command,
        cwd=tmp_path,
        env=benchmark_environment(tmp_path),
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 1, result.stdout + result.stderr
    assert "main.py" in result.stdout
    validate_cli_targets(tool, command, venv, benchmark_environment(tmp_path))


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


def test_zuban_lsp_checks_untyped_code_with_project_environment(zuban_project):
    project, venv = zuban_project
    tool = Zuban()
    path = venv.project_path / "src" / "example" / "main.py"

    async def fetch_diagnostics():
        client = LSPClient()
        try:
            async with asyncio.timeout(20):
                await client.start_io(
                    *tool.lsp_command(project, venv), cwd=venv.project_path
                )
                await client.initialize_async(
                    lsp.InitializeParams(
                        root_uri=venv.project_path.as_uri(),
                        initialization_options=tool.lsp_initialization_options(
                            project, venv
                        ),
                        capabilities=lsp.ClientCapabilities(
                            text_document=lsp.TextDocumentClientCapabilities(
                                diagnostic=lsp.DiagnosticClientCapabilities()
                            )
                        ),
                    )
                )
                client.initialized(lsp.InitializedParams())
                client.text_document_did_open(
                    lsp.DidOpenTextDocumentParams(
                        text_document=lsp.TextDocumentItem(
                            uri=path.as_uri(),
                            language_id="python",
                            version=1,
                            text=path.read_text(),
                        )
                    )
                )
                diagnostics = await client.text_document_diagnostics_async(path)
                await client.shutdown_async(None)
                return diagnostics
        finally:
            client.exit(None)
            await client.stop()

    with tool.configure_lsp(project, venv):
        diagnostics = asyncio.run(fetch_diagnostics())

    assert len(diagnostics) == 1, diagnostics
    assert (
        'expression has type "str", variable has type "int"' in diagnostics[0].message
    )


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
    with Zuban().configure_lsp(project._replace(exclude=[pattern]), venv):
        content = (venv.project_path / "pyproject.toml").read_text()
        regex = tomllib.loads(content)["tool"]["zuban"]["exclude"][0]
    assert re.search(regex, excluded)
    assert re.search(regex, excluded.replace("/", "\\"))
    assert not re.search(regex, included)
