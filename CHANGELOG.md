# Changelog

## 0.14.13

Released on 2026-01-15.

This is a follow-up release to 0.14.12. Because of an issue publishing the WASM packages, there is no GitHub release or Git tag for 0.14.12, although the package was published to PyPI. The contents of the 0.14.13 release are identical to 0.14.12.

## 0.14.12

Released on 2026-01-15.

### Preview features

- \[`flake8-blind-except`\] Allow more logging methods (`BLE001`) ([#22057](https://github.com/astral-sh/ruff/pull/22057))
- \[`ruff`\] Respect `lint.pydocstyle.property-decorators` in `RUF066` ([#22515](https://github.com/astral-sh/ruff/pull/22515))

### Bug fixes

- Fix configuration path in `--show-settings` ([#22478](https://github.com/astral-sh/ruff/pull/22478))
- Respect `fmt: skip` for multiple statements on the same logical line ([#22119](https://github.com/astral-sh/ruff/pull/22119))

### Rule changes

- \[`pydocstyle`\] Update Rust crate imperative to v1.0.7 (`D401`) ([#22519](https://github.com/astral-sh/ruff/pull/22519))
- \[`isort`\] Insert imports in alphabetical order (`I002`) ([#22493](https://github.com/astral-sh/ruff/pull/22493))

### Documentation

- Add llms.txt support for documentation ([#22463](https://github.com/astral-sh/ruff/pull/22463))
- Use prek in documentation and CI ([#22505](https://github.com/astral-sh/ruff/pull/22505))
- \[`flake8-pytest-style`\] Add `check` parameter example to `PT017` docs ([#22546](https://github.com/astral-sh/ruff/pull/22546))
- \[`ruff`\] Make example error out-of-the-box (`RUF103`) ([#22558](https://github.com/astral-sh/ruff/pull/22558))
- \[`ruff`\] document `RUF100` trailing comment fix behavior ([#22479](https://github.com/astral-sh/ruff/pull/22479))

### Other changes

- wasm: Require explicit logging initialization ([#22587](https://github.com/astral-sh/ruff/pull/22587))

### Contributors

- [@terror](https://github.com/terror)
- [@harupy](https://github.com/harupy)
- [@Jkhall81](https://github.com/Jkhall81)
- [@dhruvmanila](https://github.com/dhruvmanila)
- [@lubaskinc0de](https://github.com/lubaskinc0de)
- [@zanieb](https://github.com/zanieb)
- [@MeGaGiGaGon](https://github.com/MeGaGiGaGon)
- [@charliermarsh](https://github.com/charliermarsh)
- [@renovate](https://github.com/renovate)
- [@dylwil3](https://github.com/dylwil3)
- [@MichaReiser](https://github.com/MichaReiser)
- [@11happy](https://github.com/11happy)

## 0.14.11

Released on 2026-01-08.

### Preview features

- Consolidate diagnostics for matched disable/enable suppression comments ([#22099](https://github.com/astral-sh/ruff/pull/22099))
- Report diagnostics for invalid/unmatched range suppression comments ([#21908](https://github.com/astral-sh/ruff/pull/21908))
- \[`airflow`\] Passing positional argument into `airflow.lineage.hook.HookLineageCollector.create_asset` is not allowed (`AIR303`) ([#22046](https://github.com/astral-sh/ruff/pull/22046))
- \[`refurb`\] Mark `FURB192` fix as always unsafe ([#22210](https://github.com/astral-sh/ruff/pull/22210))
- \[`ruff`\] Add `non-empty-init-module` (`RUF067`) ([#22143](https://github.com/astral-sh/ruff/pull/22143))

### Bug fixes

- Fix GitHub format for multi-line diagnostics ([#22108](https://github.com/astral-sh/ruff/pull/22108))
- \[`flake8-unused-arguments`\] Mark `**kwargs` in `TypeVar` as used (`ARG001`) ([#22214](https://github.com/astral-sh/ruff/pull/22214))

### Rule changes

- Add `help:` subdiagnostics for several Ruff rules that can sometimes appear to disagree with `ty` ([#22331](https://github.com/astral-sh/ruff/pull/22331))
- \[`pylint`\] Demote `PLW1510` fix to display-only ([#22318](https://github.com/astral-sh/ruff/pull/22318))
- \[`pylint`\] Ignore identical members (`PLR1714`) ([#22220](https://github.com/astral-sh/ruff/pull/22220))
- \[`pylint`\] Improve diagnostic range for `PLC0206` ([#22312](https://github.com/astral-sh/ruff/pull/22312))
- \[`ruff`\] Improve fix title for `RUF102` invalid rule code ([#22100](https://github.com/astral-sh/ruff/pull/22100))
- \[`flake8-simplify`\]: Avoid unnecessary builtins import for `SIM105` ([#22358](https://github.com/astral-sh/ruff/pull/22358))

### Configuration

- Allow Python 3.15 as valid `target-version` value in preview ([#22419](https://github.com/astral-sh/ruff/pull/22419))
- Check `required-version` before parsing rules ([#22410](https://github.com/astral-sh/ruff/pull/22410))
- Include configured `src` directories when resolving graphs ([#22451](https://github.com/astral-sh/ruff/pull/22451))

### Documentation

- Update `T201` suggestion to not use root logger to satisfy `LOG015` ([#22059](https://github.com/astral-sh/ruff/pull/22059))
- Fix `iter` example in unsafe fixes doc ([#22118](https://github.com/astral-sh/ruff/pull/22118))
- \[`flake8_print`\] better suggestion for `basicConfig` in `T201` docs ([#22101](https://github.com/astral-sh/ruff/pull/22101))
- \[`pylint`\] Restore the fix safety docs for `PLW0133` ([#22211](https://github.com/astral-sh/ruff/pull/22211))
- Fix Jupyter notebook discovery info for editors ([#22447](https://github.com/astral-sh/ruff/pull/22447))

### Contributors

- [@charliermarsh](https://github.com/charliermarsh)
- [@ntBre](https://github.com/ntBre)
- [@cenviity](https://github.com/cenviity)
- [@njhearp](https://github.com/njhearp)
- [@cbachhuber](https://github.com/cbachhuber)
- [@jelle-openai](https://github.com/jelle-openai)
- [@AlexWaygood](https://github.com/AlexWaygood)
- [@ValdonVitija](https://github.com/ValdonVitija)
- [@BurntSushi](https://github.com/BurntSushi)
- [@Jkhall81](https://github.com/Jkhall81)
- [@PeterJCLaw](https://github.com/PeterJCLaw)
- [@harupy](https://github.com/harupy)
- [@amyreese](https://github.com/amyreese)
- [@sjyangkevin](https://github.com/sjyangkevin)
- [@woodruffw](https://github.com/woodruffw)

## 0.14.10

Released on 2025-12-18.

### Preview features

- [formatter] Fluent formatting of method chains ([#21369](https://github.com/astral-sh/ruff/pull/21369))
- [formatter] Keep lambda parameters on one line and parenthesize the body if it expands ([#21385](https://github.com/astral-sh/ruff/pull/21385))
- \[`flake8-implicit-str-concat`\] New rule to prevent implicit string concatenation in collections (`ISC004`) ([#21972](https://github.com/astral-sh/ruff/pull/21972))
- \[`flake8-use-pathlib`\] Make fixes unsafe when types change in compound statements (`PTH104`, `PTH105`, `PTH109`, `PTH115`) ([#22009](https://github.com/astral-sh/ruff/pull/22009))
- \[`refurb`\] Extend support for `Path.open` (`FURB101`, `FURB103`) ([#21080](https://github.com/astral-sh/ruff/pull/21080))

### Bug fixes

- \[`pyupgrade`\] Fix parsing named Unicode escape sequences (`UP032`) ([#21901](https://github.com/astral-sh/ruff/pull/21901))

### Rule changes

- \[`eradicate`\] Ignore `ruff:disable` and `ruff:enable` comments in `ERA001` ([#22038](https://github.com/astral-sh/ruff/pull/22038))
- \[`flake8-pytest-style`\] Allow `match` and `check` keyword arguments without an expected exception type (`PT010`) ([#21964](https://github.com/astral-sh/ruff/pull/21964))
- [syntax-errors] Annotated name cannot be global ([#20868](https://github.com/astral-sh/ruff/pull/20868))

### Documentation

- Add `uv` and `ty` to the Ruff README ([#21996](https://github.com/astral-sh/ruff/pull/21996))
- Document known lambda formatting deviations from Black ([#21954](https://github.com/astral-sh/ruff/pull/21954))
- Update `setup.md` ([#22024](https://github.com/astral-sh/ruff/pull/22024))
- \[`flake8-bandit`\] Fix broken link (`S704`) ([#22039](https://github.com/astral-sh/ruff/pull/22039))

### Other changes

- Fix playground Share button showing "Copied!" before clipboard copy completes ([#21942](https://github.com/astral-sh/ruff/pull/21942))

### Contributors

- [@dylwil3](https://github.com/dylwil3)
- [@charliecloudberry](https://github.com/charliecloudberry)
- [@charliermarsh](https://github.com/charliermarsh)
- [@chirizxc](https://github.com/chirizxc)
- [@ntBre](https://github.com/ntBre)
- [@zanieb](https://github.com/zanieb)
- [@amyreese](https://github.com/amyreese)
- [@hauntsaninja](https://github.com/hauntsaninja)
- [@11happy](https://github.com/11happy)
- [@mahiro72](https://github.com/mahiro72)
- [@MichaReiser](https://github.com/MichaReiser)
- [@phongddo](https://github.com/phongddo)
- [@PeterJCLaw](https://github.com/PeterJCLaw)

## 0.14.9

Released on 2025-12-11.

### Preview features

- \[`ruff`\] New `RUF100` diagnostics for unused range suppressions ([#21783](https://github.com/astral-sh/ruff/pull/21783))
- \[`pylint`\] Detect subclasses of builtin exceptions (`PLW0133`) ([#21382](https://github.com/astral-sh/ruff/pull/21382))

### Bug fixes

- Fix comment placement in lambda parameters ([#21868](https://github.com/astral-sh/ruff/pull/21868))
- Skip over trivia tokens after re-lexing ([#21895](https://github.com/astral-sh/ruff/pull/21895))
- \[`flake8-bandit`\] Fix false positive when using non-standard `CSafeLoader` path (S506). ([#21830](https://github.com/astral-sh/ruff/pull/21830))
- \[`flake8-bugbear`\] Accept immutable slice default arguments (`B008`) ([#21823](https://github.com/astral-sh/ruff/pull/21823))

### Rule changes

- \[`pydocstyle`\] Suppress `D417` for parameters with `Unpack` annotations ([#21816](https://github.com/astral-sh/ruff/pull/21816))

### Performance

- Use `memchr` for computing line indexes ([#21838](https://github.com/astral-sh/ruff/pull/21838))

### Documentation

- Document `*.pyw` is included by default in preview ([#21885](https://github.com/astral-sh/ruff/pull/21885))
- Document range suppressions, reorganize suppression docs ([#21884](https://github.com/astral-sh/ruff/pull/21884))
- Update mkdocs-material to 9.7.0 (Insiders now free) ([#21797](https://github.com/astral-sh/ruff/pull/21797))

### Contributors

- [@Avasam](https://github.com/Avasam)
- [@MichaReiser](https://github.com/MichaReiser)
- [@charliermarsh](https://github.com/charliermarsh)
- [@amyreese](https://github.com/amyreese)
- [@phongddo](https://github.com/phongddo)
- [@prakhar1144](https://github.com/prakhar1144)
- [@mahiro72](https://github.com/mahiro72)
- [@ntBre](https://github.com/ntBre)
- [@LoicRiegel](https://github.com/LoicRiegel)

## 0.14.8

Released on 2025-12-04.

### Preview features

- \[`flake8-bugbear`\] Catch `yield` expressions within other statements (`B901`) ([#21200](https://github.com/astral-sh/ruff/pull/21200))
- \[`flake8-use-pathlib`\] Mark fixes unsafe for return type changes (`PTH104`, `PTH105`, `PTH109`, `PTH115`) ([#21440](https://github.com/astral-sh/ruff/pull/21440))

### Bug fixes

- Fix syntax error false positives for `await` outside functions ([#21763](https://github.com/astral-sh/ruff/pull/21763))
- \[`flake8-simplify`\] Fix truthiness assumption for non-iterable arguments in tuple/list/set calls (`SIM222`, `SIM223`) ([#21479](https://github.com/astral-sh/ruff/pull/21479))

### Documentation

- Suggest using `--output-file` option in GitLab integration ([#21706](https://github.com/astral-sh/ruff/pull/21706))

### Other changes

- [syntax-error] Default type parameter followed by non-default type parameter ([#21657](https://github.com/astral-sh/ruff/pull/21657))

### Contributors

- [@kieran-ryan](https://github.com/kieran-ryan)
- [@11happy](https://github.com/11happy)
- [@danparizher](https://github.com/danparizher)
- [@ntBre](https://github.com/ntBre)

## 0.14.7

Released on 2025-11-28.

### Preview features

- \[`flake8-bandit`\] Handle string literal bindings in suspicious-url-open-usage (`S310`) ([#21469](https://github.com/astral-sh/ruff/pull/21469))
- \[`pylint`\] Fix `PLR1708` false positives on nested functions ([#21177](https://github.com/astral-sh/ruff/pull/21177))
- \[`pylint`\] Fix suppression for empty dict without tuple key annotation (`PLE1141`) ([#21290](https://github.com/astral-sh/ruff/pull/21290))
- \[`ruff`\] Add rule `RUF066` to detect unnecessary class properties ([#21535](https://github.com/astral-sh/ruff/pull/21535))
- \[`ruff`\] Catch more dummy variable uses (`RUF052`) ([#19799](https://github.com/astral-sh/ruff/pull/19799))

### Bug fixes

- [server] Set severity for non-rule diagnostics ([#21559](https://github.com/astral-sh/ruff/pull/21559))
- \[`flake8-implicit-str-concat`\] Avoid invalid fix in (`ISC003`) ([#21517](https://github.com/astral-sh/ruff/pull/21517))
- \[`parser`\] Fix panic when parsing IPython escape command expressions ([#21480](https://github.com/astral-sh/ruff/pull/21480))

### CLI

- Show partial fixability indicator in statistics output ([#21513](https://github.com/astral-sh/ruff/pull/21513))

### Contributors

- [@mikeleppane](https://github.com/mikeleppane)
- [@senekor](https://github.com/senekor)
- [@ShaharNaveh](https://github.com/ShaharNaveh)
- [@JumboBear](https://github.com/JumboBear)
- [@prakhar1144](https://github.com/prakhar1144)
- [@tsvikas](https://github.com/tsvikas)
- [@danparizher](https://github.com/danparizher)
- [@chirizxc](https://github.com/chirizxc)
- [@AlexWaygood](https://github.com/AlexWaygood)
- [@MichaReiser](https://github.com/MichaReiser)

## 0.14.6

Released on 2025-11-21.

### Preview features

- \[`flake8-bandit`\] Support new PySNMP API paths (`S508`, `S509`) ([#21374](https://github.com/astral-sh/ruff/pull/21374))

### Bug fixes

- Adjust own-line comment placement between branches ([#21185](https://github.com/astral-sh/ruff/pull/21185))
- Avoid syntax error when formatting attribute expressions with outer parentheses, parenthesized value, and trailing comment on value ([#20418](https://github.com/astral-sh/ruff/pull/20418))
- Fix panic when formatting comments in unary expressions ([#21501](https://github.com/astral-sh/ruff/pull/21501))
- Respect `fmt: skip` for compound statements on a single line ([#20633](https://github.com/astral-sh/ruff/pull/20633))
- \[`refurb`\] Fix `FURB103` autofix ([#21454](https://github.com/astral-sh/ruff/pull/21454))
- \[`ruff`\] Fix false positive for complex conversion specifiers in `logging-eager-conversion` (`RUF065`) ([#21464](https://github.com/astral-sh/ruff/pull/21464))

### Rule changes

- \[`ruff`\] Avoid false positive on `ClassVar` reassignment (`RUF012`) ([#21478](https://github.com/astral-sh/ruff/pull/21478))

### CLI

- Render hyperlinks for lint errors ([#21514](https://github.com/astral-sh/ruff/pull/21514))
- Add a `ruff analyze` option to skip over imports in `TYPE_CHECKING` blocks ([#21472](https://github.com/astral-sh/ruff/pull/21472))

### Documentation

- Limit `eglot-format` hook to eglot-managed Python buffers ([#21459](https://github.com/astral-sh/ruff/pull/21459))
- Mention `force-exclude` in "Configuration > Python file discovery" ([#21500](https://github.com/astral-sh/ruff/pull/21500))

### Contributors

- [@ntBre](https://github.com/ntBre)
- [@dylwil3](https://github.com/dylwil3)
- [@gauthsvenkat](https://github.com/gauthsvenkat)
- [@MichaReiser](https://github.com/MichaReiser)
- [@thamer](https://github.com/thamer)
- [@Ruchir28](https://github.com/Ruchir28)
- [@thejcannon](https://github.com/thejcannon)
- [@danparizher](https://github.com/danparizher)
- [@chirizxc](https://github.com/chirizxc)

## 0.14.5

Released on 2025-11-13.

### Preview features

- \[`flake8-simplify`\] Apply `SIM113` when index variable is of type `int` ([#21395](https://github.com/astral-sh/ruff/pull/21395))
- \[`pydoclint`\] Fix false positive when Sphinx directives follow a "Raises" section (`DOC502`) ([#20535](https://github.com/astral-sh/ruff/pull/20535))
- \[`pydoclint`\] Support NumPy-style comma-separated parameters (`DOC102`) ([#20972](https://github.com/astral-sh/ruff/pull/20972))
- \[`refurb`\] Auto-fix annotated assignments (`FURB101`) ([#21278](https://github.com/astral-sh/ruff/pull/21278))
- \[`ruff`\] Ignore `str()` when not used for simple conversion (`RUF065`) ([#21330](https://github.com/astral-sh/ruff/pull/21330))

### Bug fixes

- Fix syntax error false positive on alternative `match` patterns ([#21362](https://github.com/astral-sh/ruff/pull/21362))
- \[`flake8-simplify`\] Fix false positive for iterable initializers with generator arguments (`SIM222`) ([#21187](https://github.com/astral-sh/ruff/pull/21187))
- \[`pyupgrade`\] Fix false positive on relative imports from local `.builtins` module (`UP029`) ([#21309](https://github.com/astral-sh/ruff/pull/21309))
- \[`pyupgrade`\] Consistently set the deprecated tag (`UP035`) ([#21396](https://github.com/astral-sh/ruff/pull/21396))

### Rule changes

- \[`refurb`\] Detect empty f-strings (`FURB105`) ([#21348](https://github.com/astral-sh/ruff/pull/21348))

### CLI

- Add option to provide a reason to `--add-noqa` ([#21294](https://github.com/astral-sh/ruff/pull/21294))
- Add upstream linter URL to `ruff linter --output-format=json` ([#21316](https://github.com/astral-sh/ruff/pull/21316))
- Add color to `--help` ([#21337](https://github.com/astral-sh/ruff/pull/21337))

### Documentation

- Add a new "Opening a PR" section to the contribution guide ([#21298](https://github.com/astral-sh/ruff/pull/21298))
- Added the PyScripter IDE to the list of "Who is using Ruff?" ([#21402](https://github.com/astral-sh/ruff/pull/21402))
- Update PyCharm setup instructions ([#21409](https://github.com/astral-sh/ruff/pull/21409))
- \[`flake8-annotations`\] Add link to `allow-star-arg-any` option (`ANN401`) ([#21326](https://github.com/astral-sh/ruff/pull/21326))

### Other changes

- \[`configuration`\] Improve error message when `line-length` exceeds `u16::MAX` ([#21329](https://github.com/astral-sh/ruff/pull/21329))

### Contributors

- [@njhearp](https://github.com/njhearp)
- [@11happy](https://github.com/11happy)
- [@hugovk](https://github.com/hugovk)
- [@Gankra](https://github.com/Gankra)
- [@ntBre](https://github.com/ntBre)
- [@pyscripter](https://github.com/pyscripter)
- [@danparizher](https://github.com/danparizher)
- [@MichaReiser](https://github.com/MichaReiser)
- [@henryiii](https://github.com/henryiii)
- [@charliecloudberry](https://github.com/charliecloudberry)

## 0.14.4

Released on 2025-11-06.

### Preview features

- [formatter] Allow newlines after function headers without docstrings ([#21110](https://github.com/astral-sh/ruff/pull/21110))
- [formatter] Avoid extra parentheses for long `match` patterns with `as` captures ([#21176](https://github.com/astral-sh/ruff/pull/21176))
- \[`refurb`\] Expand fix safety for keyword arguments and `Decimal`s (`FURB164`) ([#21259](https://github.com/astral-sh/ruff/pull/21259))
- \[`refurb`\] Preserve argument ordering in autofix (`FURB103`) ([#20790](https://github.com/astral-sh/ruff/pull/20790))

### Bug fixes

- [server] Fix missing diagnostics for notebooks ([#21156](https://github.com/astral-sh/ruff/pull/21156))
- \[`flake8-bugbear`\] Ignore non-NFKC attribute names in `B009` and `B010` ([#21131](https://github.com/astral-sh/ruff/pull/21131))
- \[`refurb`\] Fix false negative for underscores before sign in `Decimal` constructor (`FURB157`) ([#21190](https://github.com/astral-sh/ruff/pull/21190))
- \[`ruff`\] Fix false positives on starred arguments (`RUF057`) ([#21256](https://github.com/astral-sh/ruff/pull/21256))

### Rule changes

- \[`airflow`\] extend deprecated argument `concurrency` in `airflow..DAG` (`AIR301`) ([#21220](https://github.com/astral-sh/ruff/pull/21220))

### Documentation

- Improve `extend` docs ([#21135](https://github.com/astral-sh/ruff/pull/21135))
- \[`flake8-comprehensions`\] Fix typo in `C416` documentation ([#21184](https://github.com/astral-sh/ruff/pull/21184))
- Revise Ruff setup instructions for Zed editor ([#20935](https://github.com/astral-sh/ruff/pull/20935))

### Other changes

- Make `ruff analyze graph` work with jupyter notebooks ([#21161](https://github.com/astral-sh/ruff/pull/21161))

### Contributors

- [@chirizxc](https://github.com/chirizxc)
- [@Lee-W](https://github.com/Lee-W)
- [@musicinmybrain](https://github.com/musicinmybrain)
- [@MichaReiser](https://github.com/MichaReiser)
- [@tjkuson](https://github.com/tjkuson)
- [@danparizher](https://github.com/danparizher)
- [@renovate](https://github.com/renovate)
- [@ntBre](https://github.com/ntBre)
- [@gauthsvenkat](https://github.com/gauthsvenkat)
- [@LoicRiegel](https://github.com/LoicRiegel)

## 0.14.3

Released on 2025-10-30.

### Preview features

- Respect `--output-format` with `--watch` ([#21097](https://github.com/astral-sh/ruff/pull/21097))
- \[`pydoclint`\] Fix false positive on explicit exception re-raising (`DOC501`, `DOC502`) ([#21011](https://github.com/astral-sh/ruff/pull/21011))
- \[`pyflakes`\] Revert to stable behavior if imports for module lie in alternate branches for `F401` ([#20878](https://github.com/astral-sh/ruff/pull/20878))
- \[`pylint`\] Implement `stop-iteration-return` (`PLR1708`) ([#20733](https://github.com/astral-sh/ruff/pull/20733))
- \[`ruff`\] Add support for additional eager conversion patterns (`RUF065`) ([#20657](https://github.com/astral-sh/ruff/pull/20657))

### Bug fixes

- Fix finding keyword range for clause header after statement ending with semicolon ([#21067](https://github.com/astral-sh/ruff/pull/21067))
- Fix syntax error false positive on nested alternative patterns ([#21104](https://github.com/astral-sh/ruff/pull/21104))
- \[`ISC001`\] Fix panic when string literals are unclosed ([#21034](https://github.com/astral-sh/ruff/pull/21034))
- \[`flake8-django`\] Apply `DJ001` to annotated fields ([#20907](https://github.com/astral-sh/ruff/pull/20907))
- \[`flake8-pyi`\] Fix `PYI034` to not trigger on metaclasses (`PYI034`) ([#20881](https://github.com/astral-sh/ruff/pull/20881))
- \[`flake8-type-checking`\] Fix `TC003` false positive with `future-annotations` ([#21125](https://github.com/astral-sh/ruff/pull/21125))
- \[`pyflakes`\] Fix false positive for `__class__` in lambda expressions within class definitions (`F821`) ([#20564](https://github.com/astral-sh/ruff/pull/20564))
- \[`pyupgrade`\] Fix false positive for `TypeVar` with default on Python \<3.13 (`UP046`,`UP047`) ([#21045](https://github.com/astral-sh/ruff/pull/21045))

### Rule changes

- Add missing docstring sections to the numpy list ([#20931](https://github.com/astral-sh/ruff/pull/20931))
- \[`airflow`\] Extend `airflow.models..Param` check (`AIR311`) ([#21043](https://github.com/astral-sh/ruff/pull/21043))
- \[`airflow`\] Warn that `airflow....DAG.create_dagrun` has been removed (`AIR301`) ([#21093](https://github.com/astral-sh/ruff/pull/21093))
- \[`refurb`\] Preserve digit separators in `Decimal` constructor (`FURB157`) ([#20588](https://github.com/astral-sh/ruff/pull/20588))

### Server

- Avoid sending an unnecessary "clear diagnostics" message for clients supporting pull diagnostics ([#21105](https://github.com/astral-sh/ruff/pull/21105))

### Documentation

- \[`flake8-bandit`\] Fix correct example for `S308` ([#21128](https://github.com/astral-sh/ruff/pull/21128))

### Other changes

- Clearer error message when `line-length` goes beyond threshold ([#21072](https://github.com/astral-sh/ruff/pull/21072))

### Contributors

- [@danparizher](https://github.com/danparizher)
- [@jvacek](https://github.com/jvacek)
- [@ntBre](https://github.com/ntBre)
- [@augustelalande](https://github.com/augustelalande)
- [@prakhar1144](https://github.com/prakhar1144)
- [@TaKO8Ki](https://github.com/TaKO8Ki)
- [@dylwil3](https://github.com/dylwil3)
- [@fatelei](https://github.com/fatelei)
- [@ShaharNaveh](https://github.com/ShaharNaveh)
- [@Lee-W](https://github.com/Lee-W)

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
