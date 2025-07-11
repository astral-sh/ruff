# Changelog

## 0.12.3

### Preview features

- \[`flake8-bugbear`\] Support non-context-manager calls in `B017` ([#19063](https://github.com/astral-sh/ruff/pull/19063))
- \[`flake8-use-pathlib`\] Add autofixes for `PTH100`, `PTH106`, `PTH107`, `PTH108`, `PTH110`, `PTH111`, `PTH112`, `PTH113`, `PTH114`, `PTH115`, `PTH117`, `PTH119`, `PTH120` ([#19213](https://github.com/astral-sh/ruff/pull/19213))
- \[`flake8-use-pathlib`\] Add autofixes for `PTH203`, `PTH204`, `PTH205` ([#18922](https://github.com/astral-sh/ruff/pull/18922))

### Bug fixes

- \[`flake8-return`\] Fix false-positive for variables used inside nested functions in `RET504` ([#18433](https://github.com/astral-sh/ruff/pull/18433))
- Treat form feed as valid whitespace before a line continuation ([#19220](https://github.com/astral-sh/ruff/pull/19220))
- \[`flake8-type-checking`\] Fix syntax error introduced by fix (`TC008`) ([#19150](https://github.com/astral-sh/ruff/pull/19150))
- \[`pyupgrade`\] Keyword arguments in `super` should suppress the `UP008` fix ([#19131](https://github.com/astral-sh/ruff/pull/19131))

### Documentation

- \[`flake8-pyi`\] Make example error out-of-the-box (`PYI007`, `PYI008`) ([#19103](https://github.com/astral-sh/ruff/pull/19103))
- \[`flake8-simplify`\] Make example error out-of-the-box (`SIM116`) ([#19111](https://github.com/astral-sh/ruff/pull/19111))
- \[`flake8-type-checking`\] Make example error out-of-the-box (`TC001`) ([#19151](https://github.com/astral-sh/ruff/pull/19151))
- \[`flake8-use-pathlib`\] Make example error out-of-the-box (`PTH210`) ([#19189](https://github.com/astral-sh/ruff/pull/19189))
- \[`pycodestyle`\] Make example error out-of-the-box (`E272`) ([#19191](https://github.com/astral-sh/ruff/pull/19191))
- \[`pycodestyle`\] Make example not raise unnecessary `SyntaxError` (`E114`) ([#19190](https://github.com/astral-sh/ruff/pull/19190))
- \[`pydoclint`\] Make example error out-of-the-box (`DOC501`) ([#19218](https://github.com/astral-sh/ruff/pull/19218))
- \[`pylint`, `pyupgrade`\] Fix syntax errors in examples (`PLW1501`, `UP028`) ([#19127](https://github.com/astral-sh/ruff/pull/19127))
- \[`pylint`\] Update `missing-maxsplit-arg` docs and error to suggest proper usage (`PLC0207`) ([#18949](https://github.com/astral-sh/ruff/pull/18949))
- \[`flake8-bandit`\] Make example error out-of-the-box (`S412`) ([#19241](https://github.com/astral-sh/ruff/pull/19241))

## 0.12.2

### Preview features

- \[`flake8-pyi`\] Expand `Optional[A]` to `A | None` (`PYI016`) ([#18572](https://github.com/astral-sh/ruff/pull/18572))
- \[`pyupgrade`\] Mark `UP008` fix safe if no comments are in range ([#18683](https://github.com/astral-sh/ruff/pull/18683))

### Bug fixes

- \[`flake8-comprehensions`\] Fix `C420` to prepend whitespace when needed ([#18616](https://github.com/astral-sh/ruff/pull/18616))
- \[`perflint`\] Fix `PERF403` panic on attribute or subscription loop variable ([#19042](https://github.com/astral-sh/ruff/pull/19042))
- \[`pydocstyle`\] Fix `D413` infinite loop for parenthesized docstring ([#18930](https://github.com/astral-sh/ruff/pull/18930))
- \[`pylint`\] Fix `PLW0108` autofix introducing a syntax error when the lambda's body contains an assignment expression ([#18678](https://github.com/astral-sh/ruff/pull/18678))
- \[`refurb`\] Fix false positive on empty tuples (`FURB168`) ([#19058](https://github.com/astral-sh/ruff/pull/19058))
- \[`ruff`\] Allow more `field` calls from `attrs` (`RUF009`) ([#19021](https://github.com/astral-sh/ruff/pull/19021))
- \[`ruff`\] Fix syntax error introduced for an empty string followed by a u-prefixed string (`UP025`) ([#18899](https://github.com/astral-sh/ruff/pull/18899))

### Rule changes

- \[`flake8-executable`\] Allow `uvx` in shebang line (`EXE003`) ([#18967](https://github.com/astral-sh/ruff/pull/18967))
- \[`pandas`\] Avoid flagging `PD002` if `pandas` is not imported ([#18963](https://github.com/astral-sh/ruff/pull/18963))
- \[`pyupgrade`\] Avoid PEP-604 unions with `typing.NamedTuple` (`UP007`, `UP045`) ([#18682](https://github.com/astral-sh/ruff/pull/18682))

### Documentation

- Document link between `import-outside-top-level (PLC0415)` and `lint.flake8-tidy-imports.banned-module-level-imports` ([#18733](https://github.com/astral-sh/ruff/pull/18733))
- Fix description of the `format.skip-magic-trailing-comma` example ([#19095](https://github.com/astral-sh/ruff/pull/19095))
- \[`airflow`\] Make `AIR302` example error out-of-the-box ([#18988](https://github.com/astral-sh/ruff/pull/18988))
- \[`airflow`\] Make `AIR312` example error out-of-the-box ([#18989](https://github.com/astral-sh/ruff/pull/18989))
- \[`flake8-annotations`\] Make `ANN401` example error out-of-the-box ([#18974](https://github.com/astral-sh/ruff/pull/18974))
- \[`flake8-async`\] Make `ASYNC100` example error out-of-the-box ([#18993](https://github.com/astral-sh/ruff/pull/18993))
- \[`flake8-async`\] Make `ASYNC105` example error out-of-the-box ([#19002](https://github.com/astral-sh/ruff/pull/19002))
- \[`flake8-async`\] Make `ASYNC110` example error out-of-the-box ([#18975](https://github.com/astral-sh/ruff/pull/18975))
- \[`flake8-async`\] Make `ASYNC210` example error out-of-the-box ([#18977](https://github.com/astral-sh/ruff/pull/18977))
- \[`flake8-async`\] Make `ASYNC220`, `ASYNC221`, and `ASYNC222` examples error out-of-the-box ([#18978](https://github.com/astral-sh/ruff/pull/18978))
- \[`flake8-async`\] Make `ASYNC251` example error out-of-the-box ([#18990](https://github.com/astral-sh/ruff/pull/18990))
- \[`flake8-bandit`\] Make `S201` example error out-of-the-box ([#19017](https://github.com/astral-sh/ruff/pull/19017))
- \[`flake8-bandit`\] Make `S604` and `S609` examples error out-of-the-box ([#19049](https://github.com/astral-sh/ruff/pull/19049))
- \[`flake8-bugbear`\] Make `B028` example error out-of-the-box ([#19054](https://github.com/astral-sh/ruff/pull/19054))
- \[`flake8-bugbear`\] Make `B911` example error out-of-the-box ([#19051](https://github.com/astral-sh/ruff/pull/19051))
- \[`flake8-datetimez`\] Make `DTZ011` example error out-of-the-box ([#19055](https://github.com/astral-sh/ruff/pull/19055))
- \[`flake8-datetimez`\] Make `DTZ901` example error out-of-the-box ([#19056](https://github.com/astral-sh/ruff/pull/19056))
- \[`flake8-pyi`\] Make `PYI032` example error out-of-the-box ([#19061](https://github.com/astral-sh/ruff/pull/19061))
- \[`flake8-pyi`\] Make example error out-of-the-box (`PYI014`, `PYI015`) ([#19097](https://github.com/astral-sh/ruff/pull/19097))
- \[`flake8-pyi`\] Make example error out-of-the-box (`PYI042`) ([#19101](https://github.com/astral-sh/ruff/pull/19101))
- \[`flake8-pyi`\] Make example error out-of-the-box (`PYI059`) ([#19080](https://github.com/astral-sh/ruff/pull/19080))
- \[`flake8-pyi`\] Make example error out-of-the-box (`PYI062`) ([#19079](https://github.com/astral-sh/ruff/pull/19079))
- \[`flake8-pytest-style`\] Make example error out-of-the-box (`PT023`) ([#19104](https://github.com/astral-sh/ruff/pull/19104))
- \[`flake8-pytest-style`\] Make example error out-of-the-box (`PT030`) ([#19105](https://github.com/astral-sh/ruff/pull/19105))
- \[`flake8-quotes`\] Make example error out-of-the-box (`Q003`) ([#19106](https://github.com/astral-sh/ruff/pull/19106))
- \[`flake8-simplify`\] Make example error out-of-the-box (`SIM110`) ([#19113](https://github.com/astral-sh/ruff/pull/19113))
- \[`flake8-simplify`\] Make example error out-of-the-box (`SIM113`) ([#19109](https://github.com/astral-sh/ruff/pull/19109))
- \[`flake8-simplify`\] Make example error out-of-the-box (`SIM401`) ([#19110](https://github.com/astral-sh/ruff/pull/19110))
- \[`pyflakes`\] Fix backslash in docs (`F621`) ([#19098](https://github.com/astral-sh/ruff/pull/19098))
- \[`pylint`\] Fix `PLC0415` example ([#18970](https://github.com/astral-sh/ruff/pull/18970))

## 0.12.1

### Preview features

- \[`flake8-errmsg`\] Extend `EM101` to support byte strings ([#18867](https://github.com/astral-sh/ruff/pull/18867))
- \[`flake8-use-pathlib`\] Add autofix for `PTH202` ([#18763](https://github.com/astral-sh/ruff/pull/18763))
- \[`pygrep-hooks`\] Add `AsyncMock` methods to `invalid-mock-access` (`PGH005`) ([#18547](https://github.com/astral-sh/ruff/pull/18547))
- \[`pylint`\] Ignore `__init__.py` files in (`PLC0414`) ([#18400](https://github.com/astral-sh/ruff/pull/18400))
- \[`ruff`\] Trigger `RUF037` for empty string and byte strings ([#18862](https://github.com/astral-sh/ruff/pull/18862))
- [formatter] Fix missing blank lines before decorated classes in `.pyi` files ([#18888](https://github.com/astral-sh/ruff/pull/18888))

### Bug fixes

- Avoid generating diagnostics with per-file ignores ([#18801](https://github.com/astral-sh/ruff/pull/18801))
- Handle parenthesized arguments in `remove_argument` ([#18805](https://github.com/astral-sh/ruff/pull/18805))
- \[`flake8-logging`\] Avoid false positive for `exc_info=True` outside `logger.exception` (`LOG014`) ([#18737](https://github.com/astral-sh/ruff/pull/18737))
- \[`flake8-pytest-style`\] Enforce `pytest` import for decorators ([#18779](https://github.com/astral-sh/ruff/pull/18779))
- \[`flake8-pytest-style`\] Mark autofix for `PT001` and `PT023` as unsafe if there's comments in the decorator ([#18792](https://github.com/astral-sh/ruff/pull/18792))
- \[`flake8-pytest-style`\] `PT001`/`PT023` fix makes syntax error on parenthesized decorator ([#18782](https://github.com/astral-sh/ruff/pull/18782))
- \[`flake8-raise`\] Make fix unsafe if it deletes comments (`RSE102`) ([#18788](https://github.com/astral-sh/ruff/pull/18788))
- \[`flake8-simplify`\] Fix `SIM911` autofix creating a syntax error ([#18793](https://github.com/astral-sh/ruff/pull/18793))
- \[`flake8-simplify`\] Fix false negatives for shadowed bindings (`SIM910`, `SIM911`) ([#18794](https://github.com/astral-sh/ruff/pull/18794))
- \[`flake8-simplify`\] Preserve original behavior for `except ()` and bare `except` (`SIM105`) ([#18213](https://github.com/astral-sh/ruff/pull/18213))
- \[`flake8-pyi`\] Fix `PYI041`'s fix causing `TypeError` with `None | None | ...` ([#18637](https://github.com/astral-sh/ruff/pull/18637))
- \[`perflint`\] Fix `PERF101` autofix creating a syntax error and mark autofix as unsafe if there are comments in the `list` call expr ([#18803](https://github.com/astral-sh/ruff/pull/18803))
- \[`perflint`\] Fix false negative in `PERF401` ([#18866](https://github.com/astral-sh/ruff/pull/18866))
- \[`pylint`\] Avoid flattening nested `min`/`max` when outer call has single argument (`PLW3301`) ([#16885](https://github.com/astral-sh/ruff/pull/16885))
- \[`pylint`\] Fix `PLC2801` autofix creating a syntax error ([#18857](https://github.com/astral-sh/ruff/pull/18857))
- \[`pylint`\] Mark `PLE0241` autofix as unsafe if there's comments in the base classes ([#18832](https://github.com/astral-sh/ruff/pull/18832))
- \[`pylint`\] Suppress `PLE2510`/`PLE2512`/`PLE2513`/`PLE2514`/`PLE2515` autofix if the text contains an odd number of backslashes ([#18856](https://github.com/astral-sh/ruff/pull/18856))
- \[`refurb`\] Detect more exotic float literals in `FURB164` ([#18925](https://github.com/astral-sh/ruff/pull/18925))
- \[`refurb`\] Fix `FURB163` autofix creating a syntax error for `yield` expressions ([#18756](https://github.com/astral-sh/ruff/pull/18756))
- \[`refurb`\] Mark `FURB129` autofix as unsafe if there's comments in the `readlines` call ([#18858](https://github.com/astral-sh/ruff/pull/18858))
- \[`ruff`\] Fix false positives and negatives in `RUF010` ([#18690](https://github.com/astral-sh/ruff/pull/18690))
- Fix casing of `analyze.direction` variant names ([#18892](https://github.com/astral-sh/ruff/pull/18892))

### Rule changes

- Fix f-string interpolation escaping in generated fixes ([#18882](https://github.com/astral-sh/ruff/pull/18882))
- \[`flake8-return`\] Mark `RET501` fix unsafe if comments are inside ([#18780](https://github.com/astral-sh/ruff/pull/18780))
- \[`flake8-async`\] Fix detection for large integer sleep durations in `ASYNC116` rule ([#18767](https://github.com/astral-sh/ruff/pull/18767))
- \[`flake8-async`\] Mark autofix for `ASYNC115` as unsafe if the call expression contains comments ([#18753](https://github.com/astral-sh/ruff/pull/18753))
- \[`flake8-bugbear`\] Mark autofix for `B004` as unsafe if the `hasattr` call expr contains comments ([#18755](https://github.com/astral-sh/ruff/pull/18755))
- \[`flake8-comprehension`\] Mark autofix for `C420` as unsafe if there's comments inside the dict comprehension ([#18768](https://github.com/astral-sh/ruff/pull/18768))
- \[`flake8-comprehensions`\] Handle template strings for comprehension fixes ([#18710](https://github.com/astral-sh/ruff/pull/18710))
- \[`flake8-future-annotations`\] Add autofix (`FA100`) ([#18903](https://github.com/astral-sh/ruff/pull/18903))
- \[`pyflakes`\] Mark `F504`/`F522`/`F523` autofix as unsafe if there's a call with side effect ([#18839](https://github.com/astral-sh/ruff/pull/18839))
- \[`pylint`\] Allow fix with comments and document performance implications (`PLW3301`) ([#18936](https://github.com/astral-sh/ruff/pull/18936))
- \[`pylint`\] Detect more exotic `NaN` literals in `PLW0177` ([#18630](https://github.com/astral-sh/ruff/pull/18630))
- \[`pylint`\] Fix `PLC1802` autofix creating a syntax error and mark autofix as unsafe if there's comments in the `len` call ([#18836](https://github.com/astral-sh/ruff/pull/18836))
- \[`pyupgrade`\] Extend version detection to include `sys.version_info.major` (`UP036`) ([#18633](https://github.com/astral-sh/ruff/pull/18633))
- \[`ruff`\] Add lint rule `RUF064` for calling `chmod` with non-octal integers ([#18541](https://github.com/astral-sh/ruff/pull/18541))
- \[`ruff`\] Added `cls.__dict__.get('__annotations__')` check (`RUF063`) ([#18233](https://github.com/astral-sh/ruff/pull/18233))
- \[`ruff`\] Frozen `dataclass` default should be valid (`RUF009`) ([#18735](https://github.com/astral-sh/ruff/pull/18735))

### Server

- Consider virtual path for various server actions ([#18910](https://github.com/astral-sh/ruff/pull/18910))

### Documentation

- Add fix safety sections ([#18940](https://github.com/astral-sh/ruff/pull/18940),[#18841](https://github.com/astral-sh/ruff/pull/18841),[#18802](https://github.com/astral-sh/ruff/pull/18802),[#18837](https://github.com/astral-sh/ruff/pull/18837),[#18800](https://github.com/astral-sh/ruff/pull/18800),[#18415](https://github.com/astral-sh/ruff/pull/18415),[#18853](https://github.com/astral-sh/ruff/pull/18853),[#18842](https://github.com/astral-sh/ruff/pull/18842))
- Use updated pre-commit id ([#18718](https://github.com/astral-sh/ruff/pull/18718))
- \[`perflint`\] Small docs improvement to `PERF401` ([#18786](https://github.com/astral-sh/ruff/pull/18786))
- \[`pyupgrade`\]: Use `super()`, not `__super__` in error messages (`UP008`) ([#18743](https://github.com/astral-sh/ruff/pull/18743))
- \[`flake8-pie`\] Small docs fix to `PIE794` ([#18829](https://github.com/astral-sh/ruff/pull/18829))
- \[`flake8-pyi`\] Correct `collections-named-tuple` example to use PascalCase assignment ([#16884](https://github.com/astral-sh/ruff/pull/16884))
- \[`flake8-pie`\] Add note on type checking benefits to `unnecessary-dict-kwargs` (`PIE804`) ([#18666](https://github.com/astral-sh/ruff/pull/18666))
- \[`pycodestyle`\] Clarify PEP 8 relationship to `whitespace-around-operator` rules ([#18870](https://github.com/astral-sh/ruff/pull/18870))

### Other changes

- Disallow newlines in format specifiers of single quoted f- or t-strings ([#18708](https://github.com/astral-sh/ruff/pull/18708))
- \[`flake8-logging`\] Add fix safety section to `LOG002` ([#18840](https://github.com/astral-sh/ruff/pull/18840))
- \[`pyupgrade`\] Add fix safety section to `UP010` ([#18838](https://github.com/astral-sh/ruff/pull/18838))

## 0.12.0

Check out the [blog post](https://astral.sh/blog/ruff-v0.12.0) for a migration
guide and overview of the changes!

### Breaking changes

- **Detection of more syntax errors**

    Ruff now detects version-related syntax errors, such as the use of the `match`
    statement on Python versions before 3.10, and syntax errors emitted by
    CPython's compiler, such as irrefutable `match` patterns before the final
    `case` arm.

- **New default Python version handling for syntax errors**

    Ruff will default to the *latest* supported Python version (3.13) when
    checking for the version-related syntax errors mentioned above to prevent
    false positives in projects without a Python version configured. The default
    in all other cases, like applying lint rules, is unchanged and remains at the
    minimum supported Python version (3.9).

- **Updated f-string formatting**

    Ruff now formats multi-line f-strings with format specifiers to avoid adding a
    line break after the format specifier. This addresses a change to the Python
    grammar in version 3.13.4 that made such a line break a syntax error.

- **`rust-toolchain.toml` is no longer included in source distributions**

    The `rust-toolchain.toml` is used to specify a higher Rust version than Ruff's
    minimum supported Rust version (MSRV) for development and building release
    artifacts. However, when present in source distributions, it would also cause
    downstream package maintainers to pull in the same Rust toolchain, even if
    their available toolchain was MSRV-compatible.

### Removed Rules

The following rules have been removed:

- [`suspicious-xmle-tree-usage`](https://docs.astral.sh/ruff/rules/suspicious-xmle-tree-usage/)
    (`S320`)

### Deprecated Rules

The following rules have been deprecated:

- [`pandas-df-variable-name`](https://docs.astral.sh/ruff/rules/pandas-df-variable-name/)

### Stabilization

The following rules have been stabilized and are no longer in preview:

- [`for-loop-writes`](https://docs.astral.sh/ruff/rules/for-loop-writes) (`FURB122`)
- [`check-and-remove-from-set`](https://docs.astral.sh/ruff/rules/check-and-remove-from-set) (`FURB132`)
- [`verbose-decimal-constructor`](https://docs.astral.sh/ruff/rules/verbose-decimal-constructor) (`FURB157`)
- [`fromisoformat-replace-z`](https://docs.astral.sh/ruff/rules/fromisoformat-replace-z) (`FURB162`)
- [`int-on-sliced-str`](https://docs.astral.sh/ruff/rules/int-on-sliced-str) (`FURB166`)
- [`exc-info-outside-except-handler`](https://docs.astral.sh/ruff/rules/exc-info-outside-except-handler) (`LOG014`)
- [`import-outside-top-level`](https://docs.astral.sh/ruff/rules/import-outside-top-level) (`PLC0415`)
- [`unnecessary-dict-index-lookup`](https://docs.astral.sh/ruff/rules/unnecessary-dict-index-lookup) (`PLR1733`)
- [`nan-comparison`](https://docs.astral.sh/ruff/rules/nan-comparison) (`PLW0177`)
- [`eq-without-hash`](https://docs.astral.sh/ruff/rules/eq-without-hash) (`PLW1641`)
- [`pytest-parameter-with-default-argument`](https://docs.astral.sh/ruff/rules/pytest-parameter-with-default-argument) (`PT028`)
- [`pytest-warns-too-broad`](https://docs.astral.sh/ruff/rules/pytest-warns-too-broad) (`PT030`)
- [`pytest-warns-with-multiple-statements`](https://docs.astral.sh/ruff/rules/pytest-warns-with-multiple-statements) (`PT031`)
- [`invalid-formatter-suppression-comment`](https://docs.astral.sh/ruff/rules/invalid-formatter-suppression-comment) (`RUF028`)
- [`dataclass-enum`](https://docs.astral.sh/ruff/rules/dataclass-enum) (`RUF049`)
- [`class-with-mixed-type-vars`](https://docs.astral.sh/ruff/rules/class-with-mixed-type-vars) (`RUF053`)
- [`unnecessary-round`](https://docs.astral.sh/ruff/rules/unnecessary-round) (`RUF057`)
- [`starmap-zip`](https://docs.astral.sh/ruff/rules/starmap-zip) (`RUF058`)
- [`non-pep604-annotation-optional`] (`UP045`)
- [`non-pep695-generic-class`](https://docs.astral.sh/ruff/rules/non-pep695-generic-class) (`UP046`)
- [`non-pep695-generic-function`](https://docs.astral.sh/ruff/rules/non-pep695-generic-function) (`UP047`)
- [`private-type-parameter`](https://docs.astral.sh/ruff/rules/private-type-parameter) (`UP049`)

The following behaviors have been stabilized:

- [`collection-literal-concatenation`] (`RUF005`) now recognizes slices, in
    addition to list literals and variables.
- The fix for [`readlines-in-for`] (`FURB129`) is now marked as always safe.
- [`if-else-block-instead-of-if-exp`] (`SIM108`) will now further simplify
    expressions to use `or` instead of an `if` expression, where possible.
- [`unused-noqa`] (`RUF100`) now checks for file-level `noqa` comments as well
    as inline comments.
- [`subprocess-without-shell-equals-true`] (`S603`) now accepts literal strings,
    as well as lists and tuples of literal strings, as trusted input.
- [`boolean-type-hint-positional-argument`] (`FBT001`) now applies to types that
    include `bool`, like `bool | int` or `typing.Optional[bool]`, in addition to
    plain `bool` annotations.
- [`non-pep604-annotation-union`] (`UP007`) has now been split into two rules.
    `UP007` now applies only to `typing.Union`, while
    [`non-pep604-annotation-optional`] (`UP045`) checks for use of
    `typing.Optional`. `UP045` has also been stabilized in this release, but you
    may need to update existing `include`, `ignore`, or `noqa` settings to
    accommodate this change.

### Preview features

- \[`ruff`\] Check for non-context-manager use of `pytest.raises`, `pytest.warns`, and `pytest.deprecated_call` (`RUF061`) ([#17368](https://github.com/astral-sh/ruff/pull/17368))
- [syntax-errors] Raise unsupported syntax error for template strings prior to Python 3.14 ([#18664](https://github.com/astral-sh/ruff/pull/18664))

### Bug fixes

- Add syntax error when conversion flag does not immediately follow exclamation mark ([#18706](https://github.com/astral-sh/ruff/pull/18706))
- Add trailing space around `readlines` ([#18542](https://github.com/astral-sh/ruff/pull/18542))
- Fix `\r` and `\r\n` handling in t- and f-string debug texts ([#18673](https://github.com/astral-sh/ruff/pull/18673))
- Hug closing `}` when f-string expression has a format specifier ([#18704](https://github.com/astral-sh/ruff/pull/18704))
- \[`flake8-pyi`\] Avoid syntax error in the case of starred and keyword arguments (`PYI059`) ([#18611](https://github.com/astral-sh/ruff/pull/18611))
- \[`flake8-return`\] Fix `RET504` autofix generating a syntax error ([#18428](https://github.com/astral-sh/ruff/pull/18428))
- \[`pep8-naming`\] Suppress fix for `N804` and `N805` if the recommended name is already used ([#18472](https://github.com/astral-sh/ruff/pull/18472))
- \[`pycodestyle`\] Avoid causing a syntax error in expressions spanning multiple lines (`E731`) ([#18479](https://github.com/astral-sh/ruff/pull/18479))
- \[`pyupgrade`\] Suppress `UP008` if `super` is shadowed ([#18688](https://github.com/astral-sh/ruff/pull/18688))
- \[`refurb`\] Parenthesize lambda and ternary expressions (`FURB122`, `FURB142`) ([#18592](https://github.com/astral-sh/ruff/pull/18592))
- \[`ruff`\] Handle extra arguments to `deque` (`RUF037`) ([#18614](https://github.com/astral-sh/ruff/pull/18614))
- \[`ruff`\] Preserve parentheses around `deque` in fix for `unnecessary-empty-iterable-within-deque-call` (`RUF037`) ([#18598](https://github.com/astral-sh/ruff/pull/18598))
- \[`ruff`\] Validate arguments before offering a fix (`RUF056`) ([#18631](https://github.com/astral-sh/ruff/pull/18631))
- \[`ruff`\] Skip fix for `RUF059` if dummy name is already bound ([#18509](https://github.com/astral-sh/ruff/pull/18509))
- \[`pylint`\] Fix `PLW0128` to check assignment targets in square brackets and after asterisks ([#18665](https://github.com/astral-sh/ruff/pull/18665))

### Rule changes

- Fix false positive on mutations in `return` statements (`B909`) ([#18408](https://github.com/astral-sh/ruff/pull/18408))
- Treat `ty:` comments as pragma comments ([#18532](https://github.com/astral-sh/ruff/pull/18532))
- \[`flake8-pyi`\] Apply `custom-typevar-for-self` to string annotations (`PYI019`) ([#18311](https://github.com/astral-sh/ruff/pull/18311))
- \[`pyupgrade`\] Don't offer a fix for `Optional[None]` (`UP007`, `UP045)` ([#18545](https://github.com/astral-sh/ruff/pull/18545))
- \[`pyupgrade`\] Fix `super(__class__, self)` detection (`UP008`) ([#18478](https://github.com/astral-sh/ruff/pull/18478))
- \[`refurb`\] Make the fix for `FURB163` unsafe for `log2`, `log10`, `*args`, and deleted comments ([#18645](https://github.com/astral-sh/ruff/pull/18645))

### Server

- Support cancellation requests ([#18627](https://github.com/astral-sh/ruff/pull/18627))

### Documentation

- Drop confusing second `*` from glob pattern example for `per-file-target-version` ([#18709](https://github.com/astral-sh/ruff/pull/18709))
- Update Neovim configuration examples ([#18491](https://github.com/astral-sh/ruff/pull/18491))
- \[`pylint`\] De-emphasize `__hash__ = Parent.__hash__` (`PLW1641`) ([#18613](https://github.com/astral-sh/ruff/pull/18613))
- \[`refurb`\] Add a note about float literal handling (`FURB157`) ([#18615](https://github.com/astral-sh/ruff/pull/18615))

## 0.11.x

See [changelogs/0.11.x](./changelogs/0.11.x.md)

## 0.10.x

See [changelogs/0.10.x](./changelogs/0.10.x.md)

## 0.9.x

See [changelogs/0.9.x](./changelogs/0.9.x.md)

## 0.8.x

See [changelogs/0.8.x](./changelogs/0.8.x.md)

## 0.7.x

See [changelogs/0.7.x](./changelogs/0.7.x.md)

## 0.6.x

See [changelogs/0.6.x](./changelogs/0.6.x.md)

## 0.5.x

See [changelogs/0.5.x](./changelogs/0.5.x.md)

## 0.4.x

See [changelogs/0.4.x](./changelogs/0.4.x.md)

## 0.3.x

See [changelogs/0.3.x](./changelogs/0.3.x.md)

## 0.2.x

See [changelogs/0.2.x](./changelogs/0.2.x.md)

## 0.1.x

See [changelogs/0.1.x](./changelogs/0.1.x.md)

[`boolean-type-hint-positional-argument`]: https://docs.astral.sh/ruff/rules/boolean-type-hint-positional-argument
[`collection-literal-concatenation`]: https://docs.astral.sh/ruff/rules/collection-literal-concatenation
[`if-else-block-instead-of-if-exp`]: https://docs.astral.sh/ruff/rules/if-else-block-instead-of-if-exp
[`non-pep604-annotation-optional`]: https://docs.astral.sh/ruff/rules/non-pep604-annotation-optional
[`non-pep604-annotation-union`]: https://docs.astral.sh/ruff/rules/non-pep604-annotation-union
[`readlines-in-for`]: https://docs.astral.sh/ruff/rules/readlines-in-for
[`subprocess-without-shell-equals-true`]: https://docs.astral.sh/ruff/rules/subprocess-without-shell-equals-true
[`unused-noqa`]: https://docs.astral.sh/ruff/rules/unused-noqa
