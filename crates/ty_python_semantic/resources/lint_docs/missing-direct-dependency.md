## What it does

Checks for imports of installed distributions that the current project does not
declare as direct dependencies.

## Why is this bad?

A dependency can be installed because another package requires it. Importing that
dependency without declaring it makes your project rely on another package's
dependency list. If that package removes the dependency, your imports can fail.

Declare each distribution your package imports in `project.dependencies` or
`project.optional-dependencies` in `pyproject.toml`. Non-package files, such as
tests and development scripts, can also use dependencies declared in dependency
groups.

## Rule status

This rule is disabled by default. It requires uv workspace integration
(`TY_UV=1`) and an existing, synchronized environment. Running
[`uv check`](https://docs.astral.sh/uv/reference/cli/#uv-check) synchronizes the
environment automatically before invoking ty, unless `--no-sync` is passed.
The rule itself reads the dependency graph and module ownership returned by
`uv workspace metadata`; it does not install or update dependencies. uv 0.11.32
or later is required to report module ownership without synchronizing the environment.

The current workspace integration applies to directory checks. Explicit file
arguments and `--config-file` bypass uv workspace discovery.

Standard-library imports, imports guarded by `TYPE_CHECKING`, and imports whose
distribution cannot be identified unambiguously are not reported. This rule does
not support PEP 723 scripts.

Imports of [namespace packages](https://docs.python.org/3/reference/import.html#namespace-packages)
themselves, such as `import ns`, are not reported: the namespace can contain
modules from several distributions. Imports of their submodules, such as
`import ns.child`, are checked when the owning distribution is known. An
`__init__.pyi` stub does not change this distinction.

Native packages that ty can resolve only as namespace packages at runtime are
also skipped. For other native modules, ty can use stubs to resolve the import
and uv's ownership map to identify the distribution.

Some editable installations add the whole project directory to Python's import
path, making both package code and files such as `tests/test_app.py` importable.
If uv does not identify which modules belong to the distribution, ty allows
dependency-group imports throughout that directory, including in package code,
to avoid incorrectly flagging imports in tests and scripts.

## Examples

With `requests` as a direct dependency, `urllib3` may also be installed because
`requests` depends on it:

```python {data-mdtest="ignore"}
import requests
import urllib3  # error: [missing-direct-dependency]
```

Add `urllib3` to `project.dependencies` if your code imports it directly.
