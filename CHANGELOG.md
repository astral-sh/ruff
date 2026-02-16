# Changelog

## 0.15.1

Released on 2026-02-12.

### Preview features

- \[`airflow`\] Add ruff rules to catch deprecated Airflow imports for Airflow 3.1 (`AIR321`) ([#22376](https://github.com/astral-sh/ruff/pull/22376))
- \[`airflow`\] Third positional parameter not named `ti_key` should be flagged for `BaseOperatorLink.get_link` (`AIR303`) ([#22828](https://github.com/astral-sh/ruff/pull/22828))
- \[`flake8-gettext`\] Fix false negatives for plural argument of `ngettext` (`INT001`, `INT002`, `INT003`) ([#21078](https://github.com/astral-sh/ruff/pull/21078))
- \[`pyflakes`\] Fix infinite loop in preview fix for `unused-import` (`F401`) ([#23038](https://github.com/astral-sh/ruff/pull/23038))
- \[`pygrep-hooks`\] Detect non-existent mock methods in standalone expressions (`PGH005`) ([#22830](https://github.com/astral-sh/ruff/pull/22830))
- \[`pylint`\] Allow dunder submodules and improve diagnostic range (`PLC2701`) ([#22804](https://github.com/astral-sh/ruff/pull/22804))
- \[`pyupgrade`\] Improve diagnostic range for tuples (`UP024`) ([#23013](https://github.com/astral-sh/ruff/pull/23013))
- \[`refurb`\] Check subscripts in tuple do not use lambda parameters in `reimplemented-operator` (`FURB118`) ([#23079](https://github.com/astral-sh/ruff/pull/23079))
- \[`ruff`\] Detect mutable defaults in `field` calls (`RUF008`) ([#23046](https://github.com/astral-sh/ruff/pull/23046))
- \[`ruff`\] Ignore std `cmath.inf` (`RUF069`) ([#23120](https://github.com/astral-sh/ruff/pull/23120))
- \[`ruff`\] New rule `float-equality-comparison` (`RUF069`) ([#20585](https://github.com/astral-sh/ruff/pull/20585))
- Don't format unlabeled Markdown code blocks ([#23106](https://github.com/astral-sh/ruff/pull/23106))
- Markdown formatting support in LSP ([#23063](https://github.com/astral-sh/ruff/pull/23063))
- Support Quarto Markdown language markers ([#22947](https://github.com/astral-sh/ruff/pull/22947))
- Support formatting `pycon` Markdown code blocks ([#23112](https://github.com/astral-sh/ruff/pull/23112))
- Use extension mapping to select Markdown code block language ([#22934](https://github.com/astral-sh/ruff/pull/22934))

### Bug fixes

- Avoid false positive for undefined variables in `FAST001` ([#23224](https://github.com/astral-sh/ruff/pull/23224))
- Avoid introducing syntax errors for `FAST003` autofix ([#23227](https://github.com/astral-sh/ruff/pull/23227))
- Avoid suggesting `InitVar` for `__post_init__` that references PEP 695 type parameters ([#23226](https://github.com/astral-sh/ruff/pull/23226))
- Deduplicate type variables in generic functions ([#23225](https://github.com/astral-sh/ruff/pull/23225))
- Fix exception handler parenthesis removal for Python 3.14+ ([#23126](https://github.com/astral-sh/ruff/pull/23126))
- Fix f-string middle panic when parsing t-strings ([#23232](https://github.com/astral-sh/ruff/pull/23232))
- Wrap `RUF020` target for multiline fixes ([#23210](https://github.com/astral-sh/ruff/pull/23210))
- Wrap `UP007` target for multiline fixes ([#23208](https://github.com/astral-sh/ruff/pull/23208))
- Fix missing diagnostics for last range suppression in file ([#23242](https://github.com/astral-sh/ruff/pull/23242))
- \[`pyupgrade`\] Fix syntax error on string with newline escape and comment (`UP037`) ([#22968](https://github.com/astral-sh/ruff/pull/22968))

### Rule changes

- Use `ruff` instead of `Ruff` as the program name in GitHub output format ([#23240](https://github.com/astral-sh/ruff/pull/23240))
- \[`PT006`\] Fix syntax error when unpacking nested tuples in `parametrize` fixes (#22441) ([#22464](https://github.com/astral-sh/ruff/pull/22464))
- \[`airflow`\] Catch deprecated attribute access from context key for Airflow 3.0 (`AIR301`) ([#22850](https://github.com/astral-sh/ruff/pull/22850))
- \[`airflow`\] Capture deprecated arguments and a decorator (`AIR301`) ([#23170](https://github.com/astral-sh/ruff/pull/23170))
- \[`flake8-boolean-trap`\] Add `multiprocessing.Value` to excluded functions for `FBT003` ([#23010](https://github.com/astral-sh/ruff/pull/23010))
- \[`flake8-bugbear`\] Add a secondary annotation showing the previous occurrence (`B033`) ([#22634](https://github.com/astral-sh/ruff/pull/22634))
- \[`flake8-type-checking`\] Add sub-diagnostic showing the runtime use of an annotation (`TC004`) ([#23091](https://github.com/astral-sh/ruff/pull/23091))
- \[`isort`\] Support configurable import section heading comments ([#23151](https://github.com/astral-sh/ruff/pull/23151))
- \[`ruff`\] Improve the diagnostic for `RUF012` ([#23202](https://github.com/astral-sh/ruff/pull/23202))

### Formatter

- Suppress diagnostic output for `format --check --silent` ([#17736](https://github.com/astral-sh/ruff/pull/17736))

### Documentation

- Add tabbed shell completion documentation ([#23169](https://github.com/astral-sh/ruff/pull/23169))
- Explain how to enable Markdown formatting for pre-commit hook ([#23077](https://github.com/astral-sh/ruff/pull/23077))
- Fixed import in `runtime-evaluated-decorators` example ([#23187](https://github.com/astral-sh/ruff/pull/23187))
- Update ruff server contributing guide ([#23060](https://github.com/astral-sh/ruff/pull/23060))

### Other changes

- Exclude WASM artifacts from GitHub releases ([#23221](https://github.com/astral-sh/ruff/pull/23221))

### Contributors

- [@mkniewallner](https://github.com/mkniewallner)
- [@bxff](https://github.com/bxff)
- [@dylwil3](https://github.com/dylwil3)
- [@Avasam](https://github.com/Avasam)
- [@amyreese](https://github.com/amyreese)
- [@charliermarsh](https://github.com/charliermarsh)
- [@Alex-ley-scrub](https://github.com/Alex-ley-scrub)
- [@Kalmaegi](https://github.com/Kalmaegi)
- [@danparizher](https://github.com/danparizher)
- [@AiyionPrime](https://github.com/AiyionPrime)
- [@eureka928](https://github.com/eureka928)
- [@11happy](https://github.com/11happy)
- [@Jkhall81](https://github.com/Jkhall81)
- [@chirizxc](https://github.com/chirizxc)
- [@leandrobbraga](https://github.com/leandrobbraga)
- [@tvatter](https://github.com/tvatter)
- [@anishgirianish](https://github.com/anishgirianish)
- [@shaanmajid](https://github.com/shaanmajid)
- [@ntBre](https://github.com/ntBre)
- [@sjyangkevin](https://github.com/sjyangkevin)

## 0.15.0

Released on 2026-02-03.

Check out the [blog post](https://astral.sh/blog/ruff-v0.15.0) for a migration
guide and overview of the changes!

### Breaking changes

- Ruff now formats your code according to the 2026 style guide. See the formatter section below or in the blog post for a detailed list of changes.

- The linter now supports block suppression comments. For example, to suppress `N803` for all parameters in this function:

    ```python
    # ruff: disable[N803]
    def foo(
        legacyArg1,
        legacyArg2,
        legacyArg3,
        legacyArg4,
    ): ...
    # ruff: enable[N803]
    ```

    See the [documentation](https://docs.astral.sh/ruff/linter/#block-level) for more details.

- The `ruff:alpine` Docker image is now based on Alpine 3.23 (up from 3.21).

- The `ruff:debian` and `ruff:debian-slim` Docker images are now based on Debian 13 "Trixie" instead of Debian 12 "Bookworm."

- Binaries for the `ppc64` (64-bit big-endian PowerPC) architecture are no longer included in our releases. It should still be possible to build Ruff manually for this platform, if needed.

- Ruff now resolves all `extend`ed configuration files before falling back on a default Python version.

### Stabilization

The following rules have been stabilized and are no longer in preview:

- [`blocking-http-call-httpx-in-async-function`](https://docs.astral.sh/ruff/rules/blocking-http-call-httpx-in-async-function)
    (`ASYNC212`)
- [`blocking-path-method-in-async-function`](https://docs.astral.sh/ruff/rules/blocking-path-method-in-async-function)
    (`ASYNC240`)
- [`blocking-input-in-async-function`](https://docs.astral.sh/ruff/rules/blocking-input-in-async-function)
    (`ASYNC250`)
- [`map-without-explicit-strict`](https://docs.astral.sh/ruff/rules/map-without-explicit-strict)
    (`B912`)
- [`if-exp-instead-of-or-operator`](https://docs.astral.sh/ruff/rules/if-exp-instead-of-or-operator)
    (`FURB110`)
- [`single-item-membership-test`](https://docs.astral.sh/ruff/rules/single-item-membership-test)
    (`FURB171`)
- [`missing-maxsplit-arg`](https://docs.astral.sh/ruff/rules/missing-maxsplit-arg) (`PLC0207`)
- [`unnecessary-lambda`](https://docs.astral.sh/ruff/rules/unnecessary-lambda) (`PLW0108`)
- [`unnecessary-empty-iterable-within-deque-call`](https://docs.astral.sh/ruff/rules/unnecessary-empty-iterable-within-deque-call)
    (`RUF037`)
- [`in-empty-collection`](https://docs.astral.sh/ruff/rules/in-empty-collection) (`RUF060`)
- [`legacy-form-pytest-raises`](https://docs.astral.sh/ruff/rules/legacy-form-pytest-raises)
    (`RUF061`)
- [`non-octal-permissions`](https://docs.astral.sh/ruff/rules/non-octal-permissions) (`RUF064`)
- [`invalid-rule-code`](https://docs.astral.sh/ruff/rules/invalid-rule-code) (`RUF102`)
- [`invalid-suppression-comment`](https://docs.astral.sh/ruff/rules/invalid-suppression-comment)
    (`RUF103`)
- [`unmatched-suppression-comment`](https://docs.astral.sh/ruff/rules/unmatched-suppression-comment)
    (`RUF104`)
- [`replace-str-enum`](https://docs.astral.sh/ruff/rules/replace-str-enum) (`UP042`)

The following behaviors have been stabilized:

- The `--output-format` flag is now respected when running Ruff in `--watch` mode, and the `full` output format is now used by default, matching the regular CLI output.
- [`builtin-attribute-shadowing`](https://docs.astral.sh/ruff/rules/builtin-attribute-shadowing/) (`A003`) now detects the use of shadowed built-in names in additional contexts like decorators, default arguments, and other attribute definitions.
- [`duplicate-union-member`](https://docs.astral.sh/ruff/rules/duplicate-union-member/) (`PYI016`) now considers `typing.Optional` when searching for duplicate union members.
- [`split-static-string`](https://docs.astral.sh/ruff/rules/split-static-string/) (`SIM905`) now offers an autofix when the `maxsplit` argument is provided, even without a `sep` argument.
- [`dict-get-with-none-default`](https://docs.astral.sh/ruff/rules/dict-get-with-none-default/) (`SIM910`) now applies to more types of key expressions.
- [`super-call-with-parameters`](https://docs.astral.sh/ruff/rules/super-call-with-parameters/) (`UP008`) now has a safe fix when it will not delete comments.
- [`unnecessary-default-type-args`](https://docs.astral.sh/ruff/rules/unnecessary-default-type-args/) (`UP043`) now applies to stub (`.pyi`) files on Python versions before 3.13.

### Formatter

This release introduces the new 2026 style guide, with the following changes:

- Lambda parameters are now kept on the same line and lambda bodies will be parenthesized to let
    them break across multiple lines ([#21385](https://github.com/astral-sh/ruff/pull/21385))
- Parentheses around tuples of exceptions in `except` clauses will now be removed on Python 3.14 and
    later ([#20768](https://github.com/astral-sh/ruff/pull/20768))
- A single empty line is now permitted at the beginning of function bodies ([#21110](https://github.com/astral-sh/ruff/pull/21110))
- Parentheses are avoided for long `as` captures in `match` statements ([#21176](https://github.com/astral-sh/ruff/pull/21176))
- Extra spaces between escaped quotes and ending triple quotes can now be omitted ([#17216](https://github.com/astral-sh/ruff/pull/17216))
- Blank lines are now enforced before classes with decorators in stub files ([#18888](https://github.com/astral-sh/ruff/pull/18888))

### Preview features

- Apply formatting to Markdown code blocks ([#22470](https://github.com/astral-sh/ruff/pull/22470), [#22990](https://github.com/astral-sh/ruff/pull/22990), [#22996](https://github.com/astral-sh/ruff/pull/22996))

    See the [documentation](https://docs.astral.sh/ruff/formatter/#markdown-code-formatting) for more details.

### Bug fixes

- Fix suppression indentation matching ([#22903](https://github.com/astral-sh/ruff/pull/22903))

### Rule changes

- Customize where the `fix_title` sub-diagnostic appears ([#23044](https://github.com/astral-sh/ruff/pull/23044))
- \[`FastAPI`\] Add sub-diagnostic explaining why a fix was unavailable (`FAST002`) ([#22565](https://github.com/astral-sh/ruff/pull/22565))
- \[`flake8-annotations`\] Don't suggest `NoReturn` for functions raising `NotImplementedError` (`ANN201`, `ANN202`, `ANN205`, `ANN206`) ([#21311](https://github.com/astral-sh/ruff/pull/21311))
- \[`pyupgrade`\] Make fix unsafe if it deletes comments (`UP017`) ([#22873](https://github.com/astral-sh/ruff/pull/22873))
- \[`pyupgrade`\] Make fix unsafe if it deletes comments (`UP020`) ([#22872](https://github.com/astral-sh/ruff/pull/22872))
- \[`pyupgrade`\] Make fix unsafe if it deletes comments (`UP033`) ([#22871](https://github.com/astral-sh/ruff/pull/22871))
- \[`refurb`\] Do not add `abc.ABC` if already present (`FURB180`) ([#22234](https://github.com/astral-sh/ruff/pull/22234))
- \[`refurb`\] Make fix unsafe if it deletes comments (`FURB110`) ([#22768](https://github.com/astral-sh/ruff/pull/22768))
- \[`ruff`\] Add sub-diagnostics with permissions (`RUF064`) ([#22972](https://github.com/astral-sh/ruff/pull/22972))

### Server

- Identify notebooks by LSP `didOpen` instead of `.ipynb` file extension ([#22810](https://github.com/astral-sh/ruff/pull/22810))

### CLI

- Add `--color` CLI option to force colored output ([#22806](https://github.com/astral-sh/ruff/pull/22806))

### Documentation

- Document `-` stdin convention in CLI help text ([#22817](https://github.com/astral-sh/ruff/pull/22817))
- \[`refurb`\] Change example to `re.search` with `^` anchor (`FURB167`) ([#22984](https://github.com/astral-sh/ruff/pull/22984))
- Fix link to Sphinx code block directives ([#23041](https://github.com/astral-sh/ruff/pull/23041))
- \[`pydocstyle`\] Clarify which quote styles are allowed (`D300`) ([#22825](https://github.com/astral-sh/ruff/pull/22825))
- \[`flake8-bugbear`\] Improve docs for `no-explicit-stacklevel` (`B028`) ([#22538](https://github.com/astral-sh/ruff/pull/22538))

### Other changes

- Update MSRV to 1.91 ([#22874](https://github.com/astral-sh/ruff/pull/22874))

### Contributors

- [@danparizher](https://github.com/danparizher)
- [@chirizxc](https://github.com/chirizxc)
- [@amyreese](https://github.com/amyreese)
- [@Jkhall81](https://github.com/Jkhall81)
- [@cwkang1998](https://github.com/cwkang1998)
- [@manzt](https://github.com/manzt)
- [@11happy](https://github.com/11happy)
- [@hugovk](https://github.com/hugovk)
- [@caiquejjx](https://github.com/caiquejjx)
- [@ntBre](https://github.com/ntBre)
- [@akawd](https://github.com/akawd)
- [@konstin](https://github.com/konstin)

## 0.14.x

See [changelogs/0.14.x](./changelogs/0.14.x.md)

## 0.13.x

See [changelogs/0.13.x](./changelogs/0.13.x.md)

## 0.12.x

See [changelogs/0.12.x](./changelogs/0.12.x.md)

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
