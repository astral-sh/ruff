# Changelog

All notable changes to this project will be documented in this file.

## Unreleased

### New rules

* verbose-raise (TRY201) from tryceratops (#2073)

## v0.0.230

(released 2023-01-22)

### New rules

* prefer-type-error (TRY004) from tryceratops (#2066)

## Updated rules

* implicit-namespace-package (INP001): Fixed caching behavior and `__init__.py` detection. (#2077, #2079)
* import-alias-is-not-conventional (ICN001): Check `from` imports (#2070, #2072)
* non-lowercase-variable-in-function (N806): Don't mark `TypeVar` & `NewType` assignments as errors (#2085)
* prefer-type-error (TRY004): Implement autofix. (#2084)

## Other changes

* Improved performance of `--select ALL` for large codebases. (#1990)
* All rules of Pylint can now be selected via `PL`
  (previously only the individual categories (`PLC`, `PLE`, `PLR` and
   `PLW`) could be selected).
* flake8-to-ruff now supports the `tool.isort.src_paths` setting in
  `pyproject.toml` (#2082)

## v0.0.229

(released 2023-01-21)

### New rules

* empty-type-checking-block (TYP005) from flake8-type-checking (#2048)
* mixed-spaces-and-tabs (E101) from pycodestyle (#2038)
* printf-string-formatting (UP031) from pyupgrade (#1803)
* shebang rules from flake8-executable (#2023)
  * shebang-python (EXE003)
  * shebang-whitespace (EXCE004)
  * shebang-newline (EXE005)
* try-consider-else (TRY300) from tryceratops (#2055)

### Updated rules

* assert-used (S101): Improve range (#2052)
* builtin-* (A001, A002, A003): Add `builtins-ignorelist` setting (#2061)
* nested-if-statements (SIM102): Only report once for deeply nested if statements (#2050)
* unpack-instead-of-concatenating-to-collection-literal (RUF005): Avoid removing comments (#2057)
* unused-import (F401): Favor false-negatives over false-positives (#2065)

## Older releases

See the [GitHub release page](https://github.com/charliermarsh/ruff/releases)
for older releases.
