# project

An example multi-package Python project used to test setting resolution and other complex
behaviors.

## Expected behavior

Running from the repo root should pick up and enforce the appropriate settings for each package:

```
∴ cargo run resources/test/project/
Found 7 error(s).
resources/test/project/examples/.dotfiles/script.py:1:8: F401 `os` imported but unused
resources/test/project/examples/.dotfiles/script.py:5:5: F841 Local variable `x` is assigned to but never used
resources/test/project/examples/docs/docs/file.py:1:1: I001 Import block is un-sorted or un-formatted
resources/test/project/examples/docs/docs/file.py:8:5: F841 Local variable `x` is assigned to but never used
resources/test/project/src/file.py:1:8: F401 `os` imported but unused
resources/test/project/src/file.py:5:5: F841 Local variable `x` is assigned to but never used
resources/test/project/src/import_file.py:1:1: I001 Import block is un-sorted or un-formatted
4 potentially fixable with the --fix option.
```

Running from the project directory itself should exhibit the same behavior:

```
∴ (cd resources/test/project/ && cargo run .)
Found 7 error(s).
examples/.dotfiles/script.py:1:8: F401 `os` imported but unused
examples/.dotfiles/script.py:5:5: F841 Local variable `x` is assigned to but never used
examples/docs/docs/file.py:1:1: I001 Import block is un-sorted or un-formatted
examples/docs/docs/file.py:8:5: F841 Local variable `x` is assigned to but never used
src/file.py:1:8: F401 `os` imported but unused
src/file.py:5:5: F841 Local variable `x` is assigned to but never used
src/import_file.py:1:1: I001 Import block is un-sorted or un-formatted
4 potentially fixable with the --fix option.
```

Running from the sub-package directory should exhibit the same behavior, but omit the top-level
files:

```
∴ (cd resources/test/project/examples/docs && cargo run .)
Found 2 error(s).
docs/file.py:1:1: I001 Import block is un-sorted or un-formatted
docs/file.py:8:5: F841 Local variable `x` is assigned to but never used
1 potentially fixable with the --fix option.
```

`--config` should force Ruff to use the specified `pyproject.toml` for all files, and resolve
file paths from the current working directory:

```
∴ (cargo run -- --config=resources/test/project/pyproject.toml resources/test/project/)
Found 11 error(s).
resources/test/project/examples/.dotfiles/script.py:1:8: F401 `os` imported but unused
resources/test/project/examples/.dotfiles/script.py:5:5: F841 Local variable `x` is assigned to but never used
resources/test/project/examples/docs/docs/concepts/file.py:1:8: F401 `os` imported but unused
resources/test/project/examples/docs/docs/concepts/file.py:5:5: F841 Local variable `x` is assigned to but never used
resources/test/project/examples/docs/docs/file.py:1:8: F401 `os` imported but unused
resources/test/project/examples/docs/docs/file.py:3:8: F401 `numpy` imported but unused
resources/test/project/examples/docs/docs/file.py:4:27: F401 `docs.concepts.file` imported but unused
resources/test/project/examples/docs/docs/file.py:8:5: F841 Local variable `x` is assigned to but never used
resources/test/project/src/file.py:1:8: F401 `os` imported but unused
resources/test/project/src/file.py:5:5: F841 Local variable `x` is assigned to but never used
resources/test/project/src/import_file.py:1:1: I001 Import block is un-sorted or un-formatted
7 potentially fixable with the --fix option.
```

Running from a parent directory should this "ignore" the `exclude` (hence, `concepts/file.py` gets
included in the output):

```
∴ (cd resources/test/project/examples && cargo run -- --config=docs/pyproject.toml .)
Found 4 error(s).
.dotfiles/script.py:5:5: F841 Local variable `x` is assigned to but never used
docs/docs/concepts/file.py:5:5: F841 Local variable `x` is assigned to but never used
docs/docs/file.py:1:1: I001 Import block is un-sorted or un-formatted
docs/docs/file.py:8:5: F841 Local variable `x` is assigned to but never used
1 potentially fixable with the --fix option.
```

Passing an excluded directory directly should report errors in the contained files:

```
∴ cargo run resources/test/project/examples/excluded/
Found 2 error(s).
resources/test/project/examples/excluded/script.py:1:8: F401 `os` imported but unused
resources/test/project/examples/excluded/script.py:5:5: F841 Local variable `x` is assigned to but never used
1 potentially fixable with the --fix option.
```
