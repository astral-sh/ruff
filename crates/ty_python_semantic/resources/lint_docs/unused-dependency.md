## What it does

Checks for dependencies declared in a project's `pyproject.toml` or a PEP 723 script that are never
imported.

For example, if a project declares `pillow` and `httpx` and its only import is
`from PIL import Image`, ty reports `httpx`. uv identifies `pillow` as the package that provides
`PIL`.

## Why is this bad?

A dependency that a project no longer needs adds installation time and can restrict which versions
of other packages can be installed.

## Rule status

This preview rule is disabled by default. To enable it for a project, run
`TY_UV=1 ty check --warn unused-dependency` with a synchronized uv environment (uv 0.13.2 or later).
Use `TY_UV=scripts` to enable uv integration for standalone scripts only.

For a workspace member, ty counts imports in all files selected by its source and ignore settings,
including unopened files. Imports in excluded files do not count, even if an included file imports
them. Checking just a file or subdirectory skips the containing member's declarations.

For a script, ty follows imports into its configured local source roots. Set `root = ["."]` under
`[tool.ty.environment]` in the script to include helpers in the same directory. Imports in stub
files and `TYPE_CHECKING` blocks count as use.

## Known limitations

ty checks runtime and optional dependencies. It skips dependency groups, packages without known
importable modules, and declarations whose alternative versions cannot all be checked. Unreadable
files, syntax errors, or a recognized dynamic import with an unknown target can hide an import, so
ty skips warnings for the affected member or script.

Some dependencies are used through commands or plugin discovery without being imported. Check for
these uses before removing a declaration.
