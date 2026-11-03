"""Untimed guards against measuring an empty or incorrectly scoped CLI check."""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
from collections.abc import Mapping
from pathlib import Path

from benchmark import Command
from benchmark.targets import FILE_LIST
from benchmark.tool import Mypy, Pyrefly, Pyright, Tool, Ty, Zuban
from benchmark.venv import Venv


def validate_cli_targets(
    tool: Tool, command: Command, venv: Venv, env: Mapping[str, str]
) -> None:
    """Check roots outside Hyperfine; diagnostics may exit 1, fatal errors may not."""
    cwd = venv.project_path
    expected = sorted(
        str((cwd / path).resolve())
        for path in (cwd / FILE_LIST).read_text(encoding="utf-8").splitlines()
    )
    if not expected:
        raise ValueError("Cannot benchmark an empty root manifest")
    args = command.command.copy()
    if isinstance(tool, Ty):
        args.append("-vv")
    elif isinstance(tool, Pyrefly):
        args.append("-v")
    elif isinstance(tool, Pyright):
        args.append("--verbose")
    elif isinstance(tool, Mypy):
        # Use the installed checker's native finder without running type analysis.
        code = (
            "import json,sys; from pathlib import Path; "
            "from mypy.main import process_options; "
            "sources,_=process_options(json.loads(sys.argv[1])); "
            "print(json.dumps([(str(Path(s.path).resolve()),s.module) for s in sources]))"
        )
        interpreter = venv.python
        if tool.path is not None:
            with tool.path.open("rb") as script:
                shebang = script.readline().decode("utf-8").strip()
            if shebang.startswith("#!/"):
                interpreter = Path(shebang.removeprefix("#!"))
        args = [str(interpreter), "-c", code, json.dumps(args[1:])]
    elif not isinstance(tool, Zuban):
        raise TypeError(f"No root preflight available for {type(tool).__name__}")
    result = subprocess.run(
        args, cwd=cwd, env=env, capture_output=True, text=True, check=False, timeout=600
    )
    output = result.stdout + result.stderr
    digest = hashlib.sha256(json.dumps(command.command).encode()).hexdigest()[:8]
    prefix = cwd / f"benchmark-preflight-{type(tool).__name__.lower()}-{digest}"
    prefix.with_suffix(".txt").write_text(output, encoding="utf-8")
    if result.returncode not in (0, 1):
        raise ValueError(
            f"{tool.name()} preflight exited {result.returncode}; see {prefix}.txt"
        )
    actual_paths = None
    if isinstance(tool, Ty):
        actual_paths = sorted(
            str(Path(path).resolve())
            for path in re.findall(
                r"Checking file '(.*)'\s*$", result.stderr, re.MULTILINE
            )
        )
        actual = len(actual_paths)
    elif isinstance(tool, Mypy):
        sources = json.loads(result.stdout)
        actual_paths = sorted(path for path, _ in sources)
        if len({module for _, module in sources}) != len(sources):
            raise ValueError(f"{tool.name()} discovered duplicate module identities")
        actual = len(actual_paths)
    else:
        if isinstance(tool, Pyrefly):
            pattern = r"Checking (\d+) files"
        elif isinstance(tool, Pyright):
            pattern = r"Found (\d+) source files"
        else:
            pattern = r"(?:checked |no issues found in )(\d+) source file"
        counts = re.findall(pattern, output)
        actual = int(counts[-1]) if counts else 0
    if actual != len(expected) or (
        actual_paths is not None and actual_paths != expected
    ):
        raise ValueError(
            f"{tool.name()} discovered {actual} roots; expected {len(expected)}. "
            f"See {prefix}.txt"
        )
    prefix.with_suffix(".json").write_text(
        json.dumps(
            {
                "expected_roots": len(expected),
                "actual_roots": actual,
                "paths_verified": actual_paths is not None,
                "command": command.command,
                "preflight_command": args,
                "environment": {
                    key: env[key]
                    for key in ("TY_MAX_PARALLELISM", "RAYON_NUM_THREADS")
                    if key in env
                },
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
