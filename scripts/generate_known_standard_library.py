"""Vendored from scripts/mkstdlibs.py in PyCQA/isort.

Source:
    https://github.com/PyCQA/isort/blob/e321a670d0fefdea0e04ed9d8d696434cf49bdec/scripts/mkstdlibs.py

Only the generation of the file has been modified for use in this project.
"""
# flake8: noqa: WPS111
from __future__ import annotations

from pathlib import Path

from sphinx.ext.intersphinx import fetch_inventory

URL = "https://docs.python.org/{}/objects.inv"
PATH = Path("crates") / "ruff_python_stdlib" / "src" / "sys.rs"
VERSIONS: list[tuple[int, int]] = [
    (3, 7),
    (3, 8),
    (3, 9),
    (3, 10),
    (3, 11),
    (3, 12),
]


class FakeConfig:
    intersphinx_timeout = None
    tls_verify = True
    user_agent = ""


class FakeApp:
    srcdir = ""
    config = FakeConfig()


with PATH.open("w") as f:
    f.write(
        """\
//! This file is generated by `scripts/generate_known_standard_library.py`

pub fn is_known_standard_library(minor_version: u32, module: &str) -> bool {
    matches!((minor_version, module),
""",
    )

    modules_by_version = {}

    for major_version, minor_version in VERSIONS:
        url = URL.format(f"{major_version}.{minor_version}")
        invdata = fetch_inventory(FakeApp(), "", url)

        modules = {
            "_ast",
            "posixpath",
            "ntpath",
            "sre_constants",
            "sre_parse",
            "sre_compile",
            "sre",
        }

        for module in invdata["py:module"]:
            root, *_ = module.split(".")
            if root not in ["__future__", "__main__"]:
                modules.add(root)

        modules_by_version[minor_version] = modules

    # First, add a case for the modules that are in all versions.
    ubiquitous_modules = set.intersection(*modules_by_version.values())

    f.write("(_, ")
    for i, module in enumerate(sorted(ubiquitous_modules)):
        if i > 0:
            f.write(" | ")
        f.write(f'"{module}"')
    f.write(")")
    f.write("\n")

    # Next, add any version-specific modules.
    for _major_version, minor_version in VERSIONS:
        version_modules = set.difference(
            modules_by_version[minor_version],
            ubiquitous_modules,
        )

        f.write(" | ")
        f.write(f"({minor_version}, ")
        for i, module in enumerate(sorted(version_modules)):
            if i > 0:
                f.write(" | ")
            f.write(f'"{module}"')
        f.write(")")
        f.write("\n")

    f.write(
        """\
        )
}
    """,
    )
