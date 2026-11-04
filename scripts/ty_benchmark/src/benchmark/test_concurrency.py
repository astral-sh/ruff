import pytest

from benchmark import benchmark_environment, cli_benchmark_environment
from benchmark.concurrency import CLI_WORKERS
from benchmark.projects import Project
from benchmark.tool import Mypy, Pyrefly, Pyright
from benchmark.venv import Venv


@pytest.mark.parametrize("single_threaded", [False, True])
def test_cli_commands_use_per_checker_counts(tmp_path, monkeypatch, single_threaded):
    monkeypatch.setattr("os.process_cpu_count", lambda: 64)
    monkeypatch.setitem(CLI_WORKERS, "mypy", 2)
    monkeypatch.setitem(CLI_WORKERS, "pyrefly", 4)
    monkeypatch.setitem(CLI_WORKERS, "pyright", 8)
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
    commands = {
        "mypy": Mypy(warm=False, path=executable)
        .command(project, venv, single_threaded)
        .command,
        "pyrefly": Pyrefly(path=executable)
        .command(project, venv, single_threaded)
        .command,
        "pyright": Pyright(path=executable)
        .command(project, venv, single_threaded)
        .command,
    }
    mypy = commands["mypy"]
    assert mypy[mypy.index("--num-workers") + 1] == ("0" if single_threaded else "2")
    pyrefly = commands["pyrefly"]
    assert pyrefly[pyrefly.index("--threads") + 1] == ("1" if single_threaded else "4")
    pyright = commands["pyright"]
    if single_threaded:
        assert "--threads" not in pyright
    else:
        assert pyright[pyright.index("--threads") + 1] == "8"


@pytest.mark.parametrize("single_threaded", [False, True])
def test_cli_environment_preserves_language_server_policy(
    tmp_path, monkeypatch, single_threaded
):
    monkeypatch.setattr("os.process_cpu_count", lambda: 64)
    monkeypatch.setenv("TY_MAX_PARALLELISM", "123")
    monkeypatch.setenv("RAYON_NUM_THREADS", "456")
    monkeypatch.setitem(CLI_WORKERS, "ty", 2)
    monkeypatch.setitem(CLI_WORKERS, "zuban", 4)

    cli_env = cli_benchmark_environment(tmp_path, single_threaded=single_threaded)
    assert cli_env["TY_MAX_PARALLELISM"] == ("1" if single_threaded else "2")
    assert cli_env["RAYON_NUM_THREADS"] == ("1" if single_threaded else "4")

    lsp_env = benchmark_environment(tmp_path, single_threaded=single_threaded)
    assert lsp_env["TY_MAX_PARALLELISM"] == ("1" if single_threaded else "64")
    assert lsp_env["RAYON_NUM_THREADS"] == ("1" if single_threaded else "64")
    assert {
        key: value
        for key, value in cli_env.items()
        if key not in ("TY_MAX_PARALLELISM", "RAYON_NUM_THREADS")
    } == {
        key: value
        for key, value in lsp_env.items()
        if key not in ("TY_MAX_PARALLELISM", "RAYON_NUM_THREADS")
    }


@pytest.mark.parametrize("single_threaded", [False, True])
def test_cli_runner_uses_cli_policy(tmp_path, monkeypatch, single_threaded):
    from benchmark import run

    project = Project(
        name="example",
        repository="",
        revision="",
        install_arguments=[],
        python_version="3.12",
        include=["example"],
    )
    monkeypatch.setattr("os.process_cpu_count", lambda: 64)
    monkeypatch.setitem(CLI_WORKERS, "ty", 2)
    monkeypatch.setitem(CLI_WORKERS, "zuban", 4)
    monkeypatch.setitem(CLI_WORKERS, "mypy", 8)
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
    monkeypatch.setattr(run, "neutralize_file_directives", lambda cwd: None)
    monkeypatch.setattr(run, "write_cli_targets", lambda project, cwd: project)
    monkeypatch.setattr("benchmark.tool.which_tool", lambda *args: tmp_path / "mypy")
    preflights = []
    monkeypatch.setattr(
        run,
        "validate_cli_targets",
        lambda tool, command, venv, env: preflights.append((command, env)),
    )
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

    assert len(preflights) == len(measured) == 1
    commands, env = measured[0]
    assert preflights == [(commands[0], env)]
    command = commands[0].command
    assert command[command.index("--num-workers") + 1] == (
        "0" if single_threaded else "8"
    )
    assert env["TY_MAX_PARALLELISM"] == ("1" if single_threaded else "2")
    assert env["RAYON_NUM_THREADS"] == ("1" if single_threaded else "4")
