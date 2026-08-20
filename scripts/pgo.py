"""
Creates Profile-Guided-Optimized binaries for ruff.
Ensure `cargo-pgo` is installed and configured to run this code.
"""

import json
import os
import subprocess
from pathlib import Path

PROJECTS_JSON = "./scripts/pgo_profile.json"
CLONE_DIR = Path("clones")
TRIPLE = "x86_64-unknown-linux-gnu"

env = os.environ.copy()
env["LLVM_PROFILE_FILE"] = f"{os.getcwd()}/target/pgo-profiles/ruff_%m_%p.profraw"


def run_command(cmd, env, cwd=None, check=False):
    print(f">>> {cmd}")
    subprocess.run(
        cmd,
        shell=True,
        check=check,
        cwd=cwd,
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def main():
    CLONE_DIR.mkdir(exist_ok=True)

    with open(PROJECTS_JSON, "r") as f:
        projects = json.load(f)

    run_command("cargo pgo clean", env=env)
    run_command("cargo pgo instrument build --keep-profiles -- -q", env=env)

    for project in projects:
        name = project["name"]
        url = project["url"]
        branch = project["branch"]
        dest = CLONE_DIR / name

        print(f">> collecting data on {name}.")

        if not dest.exists():
            run_command(
                f"git clone --depth 1 --quiet --branch {branch} {url} {dest}", env=env
            )

        run_command(
            f"../../target/{TRIPLE}/release/ruff check -n -e --diff .",
            env=env,
            cwd=dest,
        )
        run_command(
            f"../../target/{TRIPLE}/release/ruff format -n --check .", env=env, cwd=dest
        )

    run_command("cargo pgo optimize", env=env)


if __name__ == "__main__":
    main()
