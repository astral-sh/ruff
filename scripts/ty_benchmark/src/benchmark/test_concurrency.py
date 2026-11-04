import pytest

from benchmark import benchmark_environment, cli_benchmark_environment
from benchmark.projects import Project
from benchmark.tool import Mypy, Pyrefly, Pyright, Ty, Zuban
from benchmark.venv import Venv


@pytest.mark.parametrize("single_threaded", [False, True])
def test_cli_uses_native_concurrency_defaults(tmp_path, monkeypatch, single_threaded):
    monkeypatch.setattr("os.process_cpu_count", lambda: 64)
    project = Project(
        name="example",
        repository="",
        revision="",
        install_arguments=[],
        python_version="3.12",
        include=["example"],
    )
    venv = Venv(project_name=project.name, project_path=tmp_path)
    executable = tmp_path / "checker"
    executable.touch()
    tools = [
        Ty(path=executable),
        Pyrefly(path=executable),
        Mypy(warm=False, path=executable),
        Pyright(path=executable),
        Zuban(path=executable),
    ]
    for tool in tools:
        command = tool.command(project, venv, single_threaded)
        if single_threaded and isinstance(tool, Mypy):
            assert command.command[command.command.index("--num-workers") + 1] == "0"
        else:
            assert "--num-workers" not in command.command
        if single_threaded and isinstance(tool, Pyrefly):
            assert command.command[command.command.index("--threads") + 1] == "1"
        else:
            assert "--threads" not in command.command
        if isinstance(tool, Mypy):
            assert command.prepare is not None
            assert ".mypy_cache_benchmark" in command.prepare[-1]
            assert "--parser" not in command.command


@pytest.mark.parametrize("single_threaded", [False, True])
def test_cli_defaults_preserve_lsp_pool_policy(tmp_path, monkeypatch, single_threaded):
    monkeypatch.setattr("os.process_cpu_count", lambda: 64)
    monkeypatch.setenv("TY_MAX_PARALLELISM", "123")
    monkeypatch.setenv("RAYON_NUM_THREADS", "456")
    monkeypatch.setenv("RAYON_RS_NUM_CPUS", "789")
    monkeypatch.setenv("PYTHON_CPU_COUNT", "32")
    monkeypatch.setenv("MYPY_NUM_WORKERS", "99")
    cli_env = cli_benchmark_environment(tmp_path, single_threaded=single_threaded)
    assert "RAYON_RS_NUM_CPUS" not in cli_env
    assert "PYTHON_CPU_COUNT" not in cli_env
    assert "MYPY_NUM_WORKERS" not in cli_env
    if single_threaded:
        assert cli_env["TY_MAX_PARALLELISM"] == cli_env["RAYON_NUM_THREADS"] == "1"
    else:
        assert "TY_MAX_PARALLELISM" not in cli_env
        assert "RAYON_NUM_THREADS" not in cli_env

    lsp_env = benchmark_environment(tmp_path, single_threaded=single_threaded)
    assert lsp_env["TY_MAX_PARALLELISM"] == ("1" if single_threaded else "64")
    assert lsp_env["RAYON_NUM_THREADS"] == ("1" if single_threaded else "64")
    assert lsp_env["RAYON_RS_NUM_CPUS"] == "789"
    assert lsp_env["PYTHON_CPU_COUNT"] == "32"
    overrides = {
        "TY_MAX_PARALLELISM",
        "RAYON_NUM_THREADS",
        "RAYON_RS_NUM_CPUS",
        "PYTHON_CPU_COUNT",
    }
    assert {key: value for key, value in cli_env.items() if key not in overrides} == {
        key: value for key, value in lsp_env.items() if key not in overrides
    }


@pytest.mark.parametrize("single_threaded", [False, True])
def test_cli_runner_uses_default_concurrency_environment(
    tmp_path, monkeypatch, single_threaded
):
    from benchmark import run

    project = Project(
        name="example",
        repository="",
        revision="",
        install_arguments=[],
        python_version="3.12",
        include=["example"],
    )
    monkeypatch.setattr(run, "ALL_PROJECTS", [project])
    monkeypatch.setattr(Project, "clone", lambda self, cwd: None)
    monkeypatch.setattr(
        Venv,
        "create",
        staticmethod(
            lambda **kwargs: Venv(
                project_name=kwargs["project"], project_path=kwargs["parent"]
            )
        ),
    )
    monkeypatch.setattr(Venv, "install", lambda self, args: None)
    monkeypatch.setattr("benchmark.tool.which_tool", lambda *args: tmp_path / "mypy")
    measured = []
    monkeypatch.setattr(
        run.Hyperfine,
        "run",
        lambda self, *, cwd, env: measured.append((self.commands, env)),
    )
    monkeypatch.setattr(
        "sys.argv",
        ["benchmark", "--tool", "mypy"]
        + (["--single-threaded"] if single_threaded else []),
    )

    run.main()

    assert len(measured) == 1
    commands, env = measured[0]
    command = commands[0].command
    if single_threaded:
        assert command[command.index("--num-workers") + 1] == "0"
        assert env["TY_MAX_PARALLELISM"] == env["RAYON_NUM_THREADS"] == "1"
    else:
        assert "--num-workers" not in command
        assert "TY_MAX_PARALLELISM" not in env
        assert "RAYON_NUM_THREADS" not in env
