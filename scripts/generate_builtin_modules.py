"""Script to generate `crates/ruff_python_stdlib/src/builtin_modules.rs`.

This script requires the following executables to be callable via a subprocess:
- `python3.7`
- `python3.8`
- `python3.9`
- `python3.10`
- `python3.11`
- `python3.12`
- `python3.13`
"""

from __future__ import annotations

import builtins
import subprocess
import textwrap
from functools import partial
from pathlib import Path

MODULE_CRATE = "ruff_python_stdlib"
MODULE_PATH = Path("crates") / MODULE_CRATE / "src" / "sys" / "builtin_modules.rs"

type Version = tuple[int, int]

PYTHON_VERSIONS: list[Version] = [
    (3, 7),
    (3, 8),
    (3, 9),
    (3, 10),
    (3, 11),
    (3, 12),
    (3, 13),
]


def builtin_modules_on_version(major_version: int, minor_version: int) -> set[str]:
    executable = f"python{major_version}.{minor_version}"
    try:
        proc = subprocess.run(
            [executable, "-c", "import sys; print(sys.builtin_module_names)"],
            check=True,
            text=True,
            capture_output=True,
        )
    except subprocess.CalledProcessError as e:
        print(e.stdout)
        print(e.stderr)
        raise
    return set(eval(proc.stdout))


def generate_module(
    script_destination: Path, crate_name: str, python_versions: list[Version]
) -> None:
    with script_destination.open("w") as f:
        print = partial(builtins.print, file=f)

        print(
            textwrap.dedent(
                """\
                //! This file is generated by `scripts/generate_builtin_modules.py`

                /// Return `true` if `module` is a [builtin module] on the given
                /// Python 3 version.
                ///
                /// "Builtin modules" are modules that are compiled directly into the
                /// Python interpreter. These can never be shadowed by first-party
                /// modules; the normal rules of module resolution do not apply to these
                /// modules.
                ///
                /// [builtin module]: https://docs.python.org/3/library/sys.html#sys.builtin_module_names
                #[expect(clippy::unnested_or_patterns)]
                pub fn is_builtin_module(minor_version: u8, module: &str) -> bool {
                    matches!((minor_version, module),
                """,
            )
        )

        modules_by_version = {
            minor_version: builtin_modules_on_version(major_version, minor_version)
            for major_version, minor_version in python_versions
        }

        # First, add a case for the modules that are in all versions.
        ubiquitous_modules = set.intersection(*modules_by_version.values())

        print("(_, ")
        for i, module in enumerate(sorted(ubiquitous_modules)):
            if i > 0:
                print(" | ", end="")
            print(f'"{module}"')
        print(")")

        # Next, add any version-specific modules.
        for _major_version, minor_version in python_versions:
            version_modules = set.difference(
                modules_by_version[minor_version],
                ubiquitous_modules,
            )

            print(" | ")
            print(f"({minor_version}, ")
            for i, module in enumerate(sorted(version_modules)):
                if i > 0:
                    print(" | ", end="")
                print(f'"{module}"')
            print(")")

        print(")}")

    subprocess.run(["cargo", "fmt", "--package", crate_name], check=True)


if __name__ == "__main__":
    generate_module(MODULE_PATH, MODULE_CRATE, PYTHON_VERSIONS)
