# Changelog

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

    Ruff will default to the _latest_ supported Python version (3.13) when
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
- [`non-pep604-annotation-optional`](https://docs.astral.sh/ruff/rules/non-pep604-annotation-optional) (`UP045`)
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

[`boolean-type-hint-positional-argument`]: https://docs.astral.sh/ruff/rules/boolean-type-hint-positional-argument
[`collection-literal-concatenation`]: https://docs.astral.sh/ruff/rules/collection-literal-concatenation
[`if-else-block-instead-of-if-exp`]: https://docs.astral.sh/ruff/rules/if-else-block-instead-of-if-exp
[`non-pep604-annotation-optional`]: https://docs.astral.sh/ruff/rules/non-pep604-annotation-optional
[`non-pep604-annotation-union`]: https://docs.astral.sh/ruff/rules/non-pep604-annotation-union
[`readlines-in-for`]: https://docs.astral.sh/ruff/rules/readlines-in-for
[`subprocess-without-shell-equals-true`]: https://docs.astral.sh/ruff/rules/subprocess-without-shell-equals-true
[`unused-noqa`]: https://docs.astral.sh/ruff/rules/unused-noqa

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
