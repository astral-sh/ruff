## What it does

Checks for declared dependencies that are not imported by a project or PEP 723 script.

The package name need not match its import name. For example, importing `PIL` counts as using the
`pillow` dependency.

## Why is this bad?

An unused dependency adds installation time and can impose unnecessary version constraints. Remove
the declaration if the dependency is no longer needed.

## Rule status

This rule is in preview and disabled by default. Enable it with `--warn unused-dependency` or set
`unused-dependency = "warn"` in the `rules` table of your ty configuration.

The rule requires uv metadata about dependencies and the modules they provide. Enable uv workspace
integration with `TY_UV=1`, or script integration with `TY_UV=scripts`. Project checks use an
existing, synchronized environment; ty synchronizes PEP 723 script environments automatically.

The rule checks `project.dependencies`, `project.optional-dependencies`, and PEP 723 script
dependencies reported by uv. It skips dependency groups and packages with no known importable
modules in the environment. Conditional dependencies are checked when uv identifies an installed
package that provides their modules.

Project diagnostics identify the package in `pyproject.toml`; script diagnostics point to the PEP
723 metadata block.

## Known limitations

Project checks require a complete scan of the project directory. Checking an individual file or
subdirectory, or configuring `src.include` or a non-empty `src.exclude`, does not report unused
project dependencies. Script checks include imports in local modules reached from the script, using
the script's own environment and declarations.

Imports in nested scopes, stub files, and `TYPE_CHECKING` blocks count as use. Calls to
`importlib.import_module` and `__import__` count when given a single literal module name. If ty
recognizes a call to either function but cannot analyze its arguments, it does not report unused
dependencies for that project or script.

Some dependencies are used without being imported in Python code, for example as command-line tools
or through plugin discovery. A warning does not mean the dependency can be removed. Check for these
uses before removing it, or disable the rule for projects that rely on them.

## Example

For a project with these dependencies:

```toml
[project]
dependencies = ["requests", "flask"]
```

If the project imports `requests` but never imports `flask`, ty reports an unused `flask` dependency
in `pyproject.toml`. Remove it if the project no longer uses Flask.
