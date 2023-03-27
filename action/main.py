import os
import shlex
import sys
from pathlib import Path
from subprocess import PIPE, STDOUT, run

ACTION_PATH = Path(os.environ["GITHUB_ACTION_PATH"])
ENV_PATH = ACTION_PATH / ".ruff-env"
ENV_BIN = ENV_PATH / ("Scripts" if sys.platform == "win32" else "bin")
OPTIONS = os.getenv("INPUT_OPTIONS", default="")
SRC = os.getenv("INPUT_SRC", default="")
VERSION = os.getenv("INPUT_VERSION", default="")

run([sys.executable, "-m", "venv", str(ENV_PATH)], check=True)

version_specifier=""
# TODO: some form of validation for user input VERSION
if VERSION != "":
    version_specifier = f"=={VERSION}"

req = f"ruff{version_specifier}"
pip_proc = run(
    [str(ENV_BIN / "python"), "-m", "pip", "install", req],
    stdout=PIPE,
    stderr=STDOUT,
    encoding="utf-8",
)
if pip_proc.returncode:
    print(pip_proc.stdout)
    print("::error::Failed to install Ruff.", flush=True)
    sys.exit(pip_proc.returncode)


base_cmd = [str(ENV_BIN / "ruff")]

proc = run([*base_cmd, *shlex.split(OPTIONS), *shlex.split(SRC)])

sys.exit(proc.returncode)
