import os
import re
import shlex
import sys
from pathlib import Path
from subprocess import run

ACTION_PATH = Path(os.environ["GITHUB_ACTION_PATH"])
OPTIONS = os.getenv("INPUT_OPTIONS", default="")
SRC = os.getenv("INPUT_SRC", default="")
VERSION = os.getenv("INPUT_VERSION", default="")

version_specifier=""
# TODO: some form of validation for user input VERSION
if VERSION != "":
    if not re.match('v?\d\.\d{1,3}\.\d{1,3}$', VERSION):
        print("VERSION does not match expected pattern")
        sys.exit(1)
    version_specifier = f"=={VERSION}"

req = f"ruff{version_specifier}"

proc = run(["pipx", "run", req, *shlex.split(OPTIONS), *shlex.split(SRC)])

sys.exit(proc.returncode)
