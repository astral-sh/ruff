## What it does

Checks for declared dependencies that are not imported by a project or PEP 723 script.

The package name need not match its import name. For example, importing `PIL` counts as using the
`pillow` dependency.

## Why is this bad?

An unused dependency adds installation time and can impose unnecessary version constraints. Remove
the declaration if the dependency is no longer needed.

## Rule status

This rule is disabled by default. Enable it with `--warn unused-dependency` or set
`unused-dependency = "warn"` in the `rules` table of your ty configuration.

It requires uv dependency metadata, including module ownership. Enable uv workspace integration with
`TY_UV=1`, or script integration with `TY_UV=scripts`. Project checks use an existing, synchronized
environment; ty synchronizes PEP 723 script environments automatically.

The rule checks `project.dependencies`, `project.optional-dependencies`, and PEP 723 script
dependencies. Dependency groups, conditional requirements, and distributions with no known
importable modules are not checked.

## Known limitations

Project checks require a complete scan of the project directory. Checking an individual file or
subdirectory, or configuring `src.include` or a nonempty `src.exclude`, does not report unused
project dependencies. Script checks include imports in local modules reached from the script, using
the script's own environment and declarations.

Imports in nested scopes, stub files, and `TYPE_CHECKING` blocks count as use. Literal calls to
`importlib.import_module` and `__import__` also count. If a recognized dynamic import has an unknown
module name, the rule does not report unused dependencies for that project or script.

Some dependencies are used without imports, for example through command-line tools or plugin
discovery. Their absence from the import inventory does not prove they can be removed. Review these
uses before removing a dependency, or disable the rule for projects that rely on them.

## Example

For a project with these dependencies:

```toml
[project]
dependencies = ["requests", "flask"]
```

If the project imports `requests` but never imports `flask`, ty reports the `flask` declaration.
Remove it if the project no longer uses Flask.
