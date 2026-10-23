# Project mdtests

These tests check Python files and project diagnostics in an in-memory project. They use the
assertion comments and inline snapshots described in [`ty_test/README.md`](../../ty_test/README.md).
To check project diagnostics, use a named `pyproject.toml` code block.

Each test starts with a fresh project at `/project` and must include `pyproject.toml`. Relative file
paths are resolved from the project root. Files outside the project are available for import
resolution but are not checked.

Unnamed TOML blocks configure ty and specify a `packages` table. Each entry maps a distribution name
to the modules it provides. The uv metadata fixture treats every listed distribution as a direct
dependency. Write its Python files under `/venv/<path-to-site-packages>/`; the runner expands that
placeholder for the current platform. The fixtures do not run uv or install packages.

Run the suite with `cargo test -p ty_project --test mdtest`. To select one fixture, append its name,
for example `-- unused_dependency.md`. `MDTEST_TEST_FILTER` selects a section within a fixture.
