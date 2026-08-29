## What it does

Checks for imports from installable packages that the current project does not declare as direct
dependencies.

The name used in dependency declarations can differ from the import name: for example, the `pillow`
package is imported as `PIL`.

## Why is this bad?

A dependency can be installed because another package requires it. Importing that dependency without
declaring it makes your project rely on another package's dependency list. If that package removes
the dependency, your imports can fail.

Declare the packages that provide your imports in `project.dependencies` or
`project.optional-dependencies` in `pyproject.toml`. Non-package files, such as tests and
development scripts, can also use dependencies declared in dependency groups.

See uv's [guide to managing dependencies](https://docs.astral.sh/uv/concepts/projects/dependencies/)
for how to add these declarations.

## Rule status

This rule is disabled by default. It requires uv workspace integration (`TY_UV=1`) and an existing,
synchronized environment. Running [`uv check`](https://docs.astral.sh/uv/reference/cli/#uv-check)
synchronizes the environment automatically before invoking ty, unless `--no-sync` is passed. The
rule itself reads the dependency graph and module ownership returned by `uv workspace metadata`; it
does not install or update dependencies. uv 0.12.3 or later is required.

## Known limitations

The current workspace integration applies to directory checks. Explicit file arguments and
`--config-file` bypass uv workspace discovery.

Imports guarded by `TYPE_CHECKING` are not reported because they are not executed at runtime. They
can use development-only dependencies, such as type stub packages, without requiring those packages
as runtime dependencies.

Standard-library imports and imports whose owning package cannot be identified unambiguously are
also not reported. This rule does not support PEP 723 scripts.

Imports of [namespace packages](https://docs.python.org/3/reference/import.html#namespace-packages)
themselves, such as `import ns`, are not reported: the namespace can contain modules from several
installable packages. Imports of their submodules, such as `import ns.child`, are checked when the
owning package is known. An `__init__.pyi` stub does not change this distinction.

Native packages that ty can resolve only as namespace packages at runtime are also skipped. For
other native modules, ty can use stubs to resolve the import and uv's ownership map to identify
which package to declare.

Some editable installations add the whole project directory to Python's import path, making both
package code and files such as `tests/test_app.py` importable. If uv does not identify which modules
belong to the installable package, ty allows dependency-group imports throughout that directory,
including in package code, to avoid incorrectly flagging imports in tests and scripts.

## Examples

With `requests` as a direct dependency, `urllib3` may also be installed because `requests` depends
on it:

```python {data-mdtest="ignore"}
import requests
import urllib3  # error: [missing-direct-dependency]
```

Add `urllib3` to `project.dependencies` if your code imports it directly.
