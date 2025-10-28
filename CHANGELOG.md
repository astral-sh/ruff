# Changelog

## 0.14.2

Released on 2025-10-23.

### Preview features

- \[`flake8-gettext`\] Resolve qualified names and built-in bindings (`INT001`, `INT002`, `INT003`) ([#19045](https://github.com/astral-sh/ruff/pull/19045))

### Bug fixes

- Avoid reusing nested, interpolated quotes before Python 3.12 ([#20930](https://github.com/astral-sh/ruff/pull/20930))
- Catch syntax errors in nested interpolations before Python 3.12 ([#20949](https://github.com/astral-sh/ruff/pull/20949))
- \[`fastapi`\] Handle ellipsis defaults in `FAST002` autofix ([#20810](https://github.com/astral-sh/ruff/pull/20810))
- \[`flake8-simplify`\] Skip `SIM911` when unknown arguments are present ([#20697](https://github.com/astral-sh/ruff/pull/20697))
- \[`pyupgrade`\] Always parenthesize assignment expressions in fix for `f-string` (`UP032`) ([#21003](https://github.com/astral-sh/ruff/pull/21003))
- \[`pyupgrade`\] Fix `UP032` conversion for decimal ints with underscores ([#21022](https://github.com/astral-sh/ruff/pull/21022))
- \[`fastapi`\] Skip autofix for keyword and `__debug__` path params (`FAST003`) ([#20960](https://github.com/astral-sh/ruff/pull/20960))

### Rule changes

- \[`flake8-bugbear`\] Skip `B905` and `B912` for fewer than two iterables and no starred arguments ([#20998](https://github.com/astral-sh/ruff/pull/20998))
- \[`ruff`\] Use `DiagnosticTag` for more `pyflakes` and `pandas` rules ([#20801](https://github.com/astral-sh/ruff/pull/20801))

### CLI

- Improve JSON output from `ruff rule` ([#20168](https://github.com/astral-sh/ruff/pull/20168))

### Documentation

- Add source to testimonial ([#20971](https://github.com/astral-sh/ruff/pull/20971))
- Document when a rule was added ([#21035](https://github.com/astral-sh/ruff/pull/21035))

### Other changes

- [syntax-errors] Name is parameter and global ([#20426](https://github.com/astral-sh/ruff/pull/20426))
- [syntax-errors] Alternative `match` patterns bind different names ([#20682](https://github.com/astral-sh/ruff/pull/20682))

### Contributors

- [@hengky-kurniawan-1](https://github.com/hengky-kurniawan-1)
- [@ShalokShalom](https://github.com/ShalokShalom)
- [@robsdedude](https://github.com/robsdedude)
- [@LoicRiegel](https://github.com/LoicRiegel)
- [@TaKO8Ki](https://github.com/TaKO8Ki)
- [@dylwil3](https://github.com/dylwil3)
- [@11happy](https://github.com/11happy)
- [@ntBre](https://github.com/ntBre)

## 0.14.1

Released on 2025-10-16.

### Preview features

- [formatter] Remove parentheses around multiple exception types on Python 3.14+ ([#20768](https://github.com/astral-sh/ruff/pull/20768))
- \[`flake8-bugbear`\] Omit annotation in preview fix for `B006` ([#20877](https://github.com/astral-sh/ruff/pull/20877))
- \[`flake8-logging-format`\] Avoid dropping implicitly concatenated pieces in the `G004` fix ([#20793](https://github.com/astral-sh/ruff/pull/20793))
- \[`pydoclint`\] Implement `docstring-extraneous-parameter` (`DOC102`) ([#20376](https://github.com/astral-sh/ruff/pull/20376))
- \[`pyupgrade`\] Extend `UP019` to detect `typing_extensions.Text` (`UP019`) ([#20825](https://github.com/astral-sh/ruff/pull/20825))
- \[`pyupgrade`\] Fix false negative for `TypeVar` with default argument in `non-pep695-generic-class` (`UP046`) ([#20660](https://github.com/astral-sh/ruff/pull/20660))

### Bug fixes

- Fix false negatives in `Truthiness::from_expr` for lambdas, generators, and f-strings ([#20704](https://github.com/astral-sh/ruff/pull/20704))
- Fix syntax error false positives for escapes and quotes in f-strings ([#20867](https://github.com/astral-sh/ruff/pull/20867))
- Fix syntax error false positives on parenthesized context managers ([#20846](https://github.com/astral-sh/ruff/pull/20846))
- \[`fastapi`\] Fix false positives for path parameters that FastAPI doesn't recognize (`FAST003`) ([#20687](https://github.com/astral-sh/ruff/pull/20687))
- \[`flake8-pyi`\] Fix operator precedence by adding parentheses when needed (`PYI061`) ([#20508](https://github.com/astral-sh/ruff/pull/20508))
- \[`ruff`\] Suppress diagnostic for f-string interpolations with debug text (`RUF010`) ([#20525](https://github.com/astral-sh/ruff/pull/20525))

### Rule changes

- \[`airflow`\] Add warning to `airflow.datasets.DatasetEvent` usage (`AIR301`) ([#20551](https://github.com/astral-sh/ruff/pull/20551))
- \[`flake8-bugbear`\] Mark `B905` and `B912` fixes as unsafe ([#20695](https://github.com/astral-sh/ruff/pull/20695))
- Use `DiagnosticTag` for more rules - changes display in editors ([#20758](https://github.com/astral-sh/ruff/pull/20758),[#20734](https://github.com/astral-sh/ruff/pull/20734))

### Documentation

- Update Python compatibility from 3.13 to 3.14 in README.md ([#20852](https://github.com/astral-sh/ruff/pull/20852))
- Update `lint.flake8-type-checking.quoted-annotations` docs ([#20765](https://github.com/astral-sh/ruff/pull/20765))
- Update setup instructions for Zed 0.208.0+ ([#20902](https://github.com/astral-sh/ruff/pull/20902))
- \[`flake8-datetimez`\] Clarify docs for several rules ([#20778](https://github.com/astral-sh/ruff/pull/20778))
- Fix typo in `RUF015` description ([#20873](https://github.com/astral-sh/ruff/pull/20873))

### Other changes

- Reduce binary size ([#20863](https://github.com/astral-sh/ruff/pull/20863))
- Improved error recovery for unclosed strings (including f- and t-strings) ([#20848](https://github.com/astral-sh/ruff/pull/20848))

### Contributors

- [@ntBre](https://github.com/ntBre)
- [@Paillat-dev](https://github.com/Paillat-dev)
- [@terror](https://github.com/terror)
- [@pieterh-oai](https://github.com/pieterh-oai)
- [@MichaReiser](https://github.com/MichaReiser)
- [@TaKO8Ki](https://github.com/TaKO8Ki)
- [@ageorgou](https://github.com/ageorgou)
- [@danparizher](https://github.com/danparizher)
- [@mgaitan](https://github.com/mgaitan)
- [@augustelalande](https://github.com/augustelalande)
- [@dylwil3](https://github.com/dylwil3)
- [@Lee-W](https://github.com/Lee-W)
- [@injust](https://github.com/injust)
- [@CarrotManMatt](https://github.com/CarrotManMatt)

## 0.14.0

Released on 2025-10-07.

### Breaking changes

- Update default and latest Python versions for 3.14 ([#20725](https://github.com/astral-sh/ruff/pull/20725))

### Preview features

- \[`flake8-bugbear`\] Include certain guaranteed-mutable expressions: tuples, generators, and assignment expressions (`B006`) ([#20024](https://github.com/astral-sh/ruff/pull/20024))
- \[`refurb`\] Add fixes for `FURB101` and `FURB103` ([#20520](https://github.com/astral-sh/ruff/pull/20520))
- \[`ruff`\] Extend `FA102` with listed PEP 585-compatible APIs ([#20659](https://github.com/astral-sh/ruff/pull/20659))

### Bug fixes

- \[`flake8-annotations`\] Fix return type annotations to handle shadowed builtin symbols (`ANN201`, `ANN202`, `ANN204`, `ANN205`, `ANN206`) ([#20612](https://github.com/astral-sh/ruff/pull/20612))
- \[`flynt`\] Fix f-string quoting for mixed quote joiners (`FLY002`) ([#20662](https://github.com/astral-sh/ruff/pull/20662))
- \[`isort`\] Fix inserting required imports before future imports (`I002`) ([#20676](https://github.com/astral-sh/ruff/pull/20676))
- \[`ruff`\] Handle argfile expansion errors gracefully ([#20691](https://github.com/astral-sh/ruff/pull/20691))
- \[`ruff`\] Skip `RUF051` if `else`/`elif` block is present ([#20705](https://github.com/astral-sh/ruff/pull/20705))
- \[`ruff`\] Improve handling of intermixed comments inside from-imports ([#20561](https://github.com/astral-sh/ruff/pull/20561))

### Documentation

- \[`flake8-comprehensions`\] Clarify fix safety documentation (`C413`) ([#20640](https://github.com/astral-sh/ruff/pull/20640))

### Contributors

- [@danparizher](https://github.com/danparizher)
- [@terror](https://github.com/terror)
- [@TaKO8Ki](https://github.com/TaKO8Ki)
- [@ntBre](https://github.com/ntBre)
- [@njhearp](https://github.com/njhearp)
- [@amyreese](https://github.com/amyreese)
- [@IDrokin117](https://github.com/IDrokin117)
- [@chirizxc](https://github.com/chirizxc)

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
