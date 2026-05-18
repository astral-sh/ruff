# Changelog

## 0.15.13

Released on 2026-05-14.

### Preview features

- Add a rule to flag lazy imports that are eagerly evaluated ([#25016](https://github.com/astral-sh/ruff/pull/25016))
- \[`pylint`\] Standardize diagnostic message (`PLR0914`, `PLR0917`) ([#24996](https://github.com/astral-sh/ruff/pull/24996))

### Bug fixes

- Fix `F811` false positive for class methods ([#24933](https://github.com/astral-sh/ruff/pull/24933))
- Fix setting selection for multi-folder workspace ([#24819](https://github.com/astral-sh/ruff/pull/24819))
- \[`eradicate`\] Fix false positive for lines with leading whitespace (`ERA001`) ([#25122](https://github.com/astral-sh/ruff/pull/25122))
- \[`flake8-pyi`\] Fix false positive for f-string debug specifier (`PYI016`) ([#24098](https://github.com/astral-sh/ruff/pull/24098))

### Rule changes

- Always include panic payload in panic diagnostic message ([#24873](https://github.com/astral-sh/ruff/pull/24873))
- Restrict `PYI034` for in-place operations to enclosing class ([#24511](https://github.com/astral-sh/ruff/pull/24511))
- Improve error message for parameters that are declared `global` ([#24902](https://github.com/astral-sh/ruff/pull/24902))
- Update known stdlib ([#25103](https://github.com/astral-sh/ruff/pull/25103))

### Performance

- \[`isort`\] Avoid constructing `glob::Pattern`s for literal known modules ([#25123](https://github.com/astral-sh/ruff/pull/25123))

### CLI

- Add TOML examples to `--config` help text ([#25013](https://github.com/astral-sh/ruff/pull/25013))
- Colorize ruff check 'All checks passed' ([#25085](https://github.com/astral-sh/ruff/pull/25085))

### Configuration

- Increase max allowed value of `line-length` setting ([#24962](https://github.com/astral-sh/ruff/pull/24962))

### Documentation

- Add `D203` to rules that conflict with the formatter ([#25044](https://github.com/astral-sh/ruff/pull/25044))
- Clarify `COM819` and formatter interaction ([#25045](https://github.com/astral-sh/ruff/pull/25045))
- Clarify that `NotImplemented` is a value, not an exception (`F901`) ([#25054](https://github.com/astral-sh/ruff/pull/25054))
- Update number of lint rules supported ([#24942](https://github.com/astral-sh/ruff/pull/24942))

### Other changes

- Simplify the playground's markdown template ([#24924](https://github.com/astral-sh/ruff/pull/24924))

### Contributors

- [@MichaReiser](https://github.com/MichaReiser)
- [@brian-c11](https://github.com/brian-c11)
- [@Andrej730](https://github.com/Andrej730)
- [@denyszhak](https://github.com/denyszhak)
- [@darestack](https://github.com/darestack)
- [@sharkdp](https://github.com/sharkdp)
- [@charliermarsh](https://github.com/charliermarsh)
- [@EkriirkE](https://github.com/EkriirkE)
- [@eyupcanakman](https://github.com/eyupcanakman)
- [@Hrk84ya](https://github.com/Hrk84ya)
- [@thernstig](https://github.com/thernstig)
- [@ntBre](https://github.com/ntBre)

## 0.15.12

Released on 2026-04-24.

### Preview features

- Implement `#ruff:file-ignore` file-level suppressions ([#23599](https://github.com/astral-sh/ruff/pull/23599))
- Implement `#ruff:ignore` logical-line suppressions ([#23404](https://github.com/astral-sh/ruff/pull/23404))
- Revert preview changes to displayed diagnostic severity in LSP ([#24789](https://github.com/astral-sh/ruff/pull/24789))
- \[`airflow`\] Implement `task-branch-as-short-circuit` (`AIR004`) ([#23579](https://github.com/astral-sh/ruff/pull/23579))
- \[`flake8-bugbear`\] Fix `break`/`continue` handling in `loop-iterator-mutation` (`B909`) ([#24440](https://github.com/astral-sh/ruff/pull/24440))
- \[`pylint`\] Fix `PLC2701` for type parameter scopes ([#24576](https://github.com/astral-sh/ruff/pull/24576))

### Rule changes

- \[`pandas-vet`\] Suggest `.array` as well in `PD011` ([#24805](https://github.com/astral-sh/ruff/pull/24805))

### CLI

- Respect default Unix permissions for cache files ([#24794](https://github.com/astral-sh/ruff/pull/24794))

### Documentation

- \[`pylint`\] Fix `PLR0124` description not to claim self-comparison always returns the same value ([#24749](https://github.com/astral-sh/ruff/pull/24749))
- \[`pyupgrade`\] Expand docs on reusable `TypeVar`s and scoping (`UP046`) ([#24153](https://github.com/astral-sh/ruff/pull/24153))
- Improve rules table accessibility ([#24711](https://github.com/astral-sh/ruff/pull/24711))

### Contributors

- [@dylwil3](https://github.com/dylwil3)
- [@AlexWaygood](https://github.com/AlexWaygood)
- [@woodruffw](https://github.com/woodruffw)
- [@avasis-ai](https://github.com/avasis-ai)
- [@Dev-iL](https://github.com/Dev-iL)
- [@denyszhak](https://github.com/denyszhak)
- [@ShipItAndPray](https://github.com/ShipItAndPray)
- [@anishgirianish](https://github.com/anishgirianish)
- [@augustelalande](https://github.com/augustelalande)
- [@amyreese](https://github.com/amyreese)
- [@majiayu000](https://github.com/majiayu000)

## 0.15.11

Released on 2026-04-16.

### Preview features

- \[`ruff`\] Ignore `RUF029` when function is decorated with `asynccontextmanager` ([#24642](https://github.com/astral-sh/ruff/pull/24642))
- \[`airflow`\] Implement `airflow-xcom-pull-in-template-string` (`AIR201`) ([#23583](https://github.com/astral-sh/ruff/pull/23583))
- \[`flake8-bandit`\] Fix `S103` false positives and negatives in mask analysis ([#24424](https://github.com/astral-sh/ruff/pull/24424))

### Bug fixes

- \[`flake8-async`\] Omit overridden methods for `ASYNC109` ([#24648](https://github.com/astral-sh/ruff/pull/24648))

### Documentation

- \[`flake8-async`\] Add override mention to `ASYNC109` docs ([#24666](https://github.com/astral-sh/ruff/pull/24666))
- Update Neovim config examples to use `vim.lsp.config` ([#24577](https://github.com/astral-sh/ruff/pull/24577))

### Contributors

- [@augustelalande](https://github.com/augustelalande)
- [@anishgirianish](https://github.com/anishgirianish)
- [@benberryallwood](https://github.com/benberryallwood)
- [@charliermarsh](https://github.com/charliermarsh)
- [@Dev-iL](https://github.com/Dev-iL)

## 0.15.10

Released on 2026-04-09.

### Preview features

- \[`flake8-logging`\] Allow closures in except handlers (`LOG004`) ([#24464](https://github.com/astral-sh/ruff/pull/24464))
- \[`flake8-self`\] Make `SLF` diagnostics robust to non-self-named variables ([#24281](https://github.com/astral-sh/ruff/pull/24281))
- \[`flake8-simplify`\] Make the fix for `collapsible-if` safe in `preview` (`SIM102`) ([#24371](https://github.com/astral-sh/ruff/pull/24371))

### Bug fixes

- Avoid emitting multi-line f-string elements before Python 3.12 ([#24377](https://github.com/astral-sh/ruff/pull/24377))
- Avoid syntax error from `E502` fixes in f-strings and t-strings ([#24410](https://github.com/astral-sh/ruff/pull/24410))
- Strip form feeds from indent passed to `dedent_to` ([#24381](https://github.com/astral-sh/ruff/pull/24381))
- \[`pyupgrade`\] Fix panic caused by handling of octals (`UP012`) ([#24390](https://github.com/astral-sh/ruff/pull/24390))
- Reject multi-line f-string elements before Python 3.12 ([#24355](https://github.com/astral-sh/ruff/pull/24355))

### Rule changes

- \[`ruff`\] Treat f-string interpolation as potential side effect (`RUF019`) ([#24426](https://github.com/astral-sh/ruff/pull/24426))

### Server

- Add support for custom file extensions ([#24463](https://github.com/astral-sh/ruff/pull/24463))

### Documentation

- Document adding fixes in CONTRIBUTING.md ([#24393](https://github.com/astral-sh/ruff/pull/24393))
- Fix JSON typo in settings example ([#24517](https://github.com/astral-sh/ruff/pull/24517))

### Contributors

- [@charliermarsh](https://github.com/charliermarsh)
- [@dylwil3](https://github.com/dylwil3)
- [@silverstein](https://github.com/silverstein)
- [@anishgirianish](https://github.com/anishgirianish)
- [@shizukushq](https://github.com/shizukushq)
- [@zanieb](https://github.com/zanieb)
- [@AlexWaygood](https://github.com/AlexWaygood)

## 0.15.9

Released on 2026-04-02.

### Preview features

- \[`pyflakes`\] Flag annotated variable redeclarations as `F811` in preview mode ([#24244](https://github.com/astral-sh/ruff/pull/24244))
- \[`ruff`\] Allow dunder-named assignments in non-strict mode for `RUF067` ([#24089](https://github.com/astral-sh/ruff/pull/24089))

### Bug fixes

- \[`flake8-errmsg`\] Avoid shadowing existing `msg` in fix for `EM101` ([#24363](https://github.com/astral-sh/ruff/pull/24363))
- \[`flake8-simplify`\] Ignore pre-initialization references in `SIM113` ([#24235](https://github.com/astral-sh/ruff/pull/24235))
- \[`pycodestyle`\] Fix `W391` fixes for consecutive empty notebook cells ([#24236](https://github.com/astral-sh/ruff/pull/24236))
- \[`pyupgrade`\] Fix `UP008` nested class matching ([#24273](https://github.com/astral-sh/ruff/pull/24273))
- \[`pyupgrade`\] Ignore strings with string-only escapes (`UP012`) ([#16058](https://github.com/astral-sh/ruff/pull/16058))
- \[`ruff`\] `RUF072`: skip formfeeds on dedent ([#24308](https://github.com/astral-sh/ruff/pull/24308))
- \[`ruff`\] Avoid re-using symbol in `RUF024` fix ([#24316](https://github.com/astral-sh/ruff/pull/24316))
- \[`ruff`\] Parenthesize expression in `RUF050` fix ([#24234](https://github.com/astral-sh/ruff/pull/24234))
- Disallow starred expressions as values of starred expressions ([#24280](https://github.com/astral-sh/ruff/pull/24280))

### Rule changes

- \[`flake8-simplify`\] Suppress `SIM105` for `except*` before Python 3.12 ([#23869](https://github.com/astral-sh/ruff/pull/23869))
- \[`pyflakes`\] Extend `F507` to flag `%`-format strings with zero placeholders ([#24215](https://github.com/astral-sh/ruff/pull/24215))
- \[`pyupgrade`\] `UP018` should detect more unnecessarily wrapped literals (UP018) ([#24093](https://github.com/astral-sh/ruff/pull/24093))
- \[`pyupgrade`\] Fix `UP008` callable scope handling to support lambdas ([#24274](https://github.com/astral-sh/ruff/pull/24274))
- \[`ruff`\] `RUF010`: Mark fix as unsafe when it deletes a comment ([#24270](https://github.com/astral-sh/ruff/pull/24270))

### Formatter

- Add `nested-string-quote-style` formatting option ([#24312](https://github.com/astral-sh/ruff/pull/24312))

### Documentation

- \[`flake8-bugbear`\] Clarify RUF071 fix safety for non-path string comparisons ([#24149](https://github.com/astral-sh/ruff/pull/24149))
- \[`flake8-type-checking`\] Clarify import cycle wording for `TC001`/`TC002`/`TC003` ([#24322](https://github.com/astral-sh/ruff/pull/24322))

### Other changes

- Avoid rendering fix lines with trailing whitespace after `|` ([#24343](https://github.com/astral-sh/ruff/pull/24343))

### Contributors

- [@charliermarsh](https://github.com/charliermarsh)
- [@MichaReiser](https://github.com/MichaReiser)
- [@tranhoangtu-it](https://github.com/tranhoangtu-it)
- [@dylwil3](https://github.com/dylwil3)
- [@zsol](https://github.com/zsol)
- [@renovate](https://github.com/renovate)
- [@bitloi](https://github.com/bitloi)
- [@danparizher](https://github.com/danparizher)
- [@chinar-amrutkar](https://github.com/chinar-amrutkar)
- [@second-ed](https://github.com/second-ed)
- [@getehen](https://github.com/getehen)
- [@Redovo1](https://github.com/Redovo1)
- [@matthewlloyd](https://github.com/matthewlloyd)
- [@zanieb](https://github.com/zanieb)
- [@InSyncWithFoo](https://github.com/InSyncWithFoo)
- [@RenzoMXD](https://github.com/RenzoMXD)

## 0.15.8

Released on 2026-03-26.

### Preview features

- \[`ruff`\] New rule `unnecessary-if` (`RUF050`) ([#24114](https://github.com/astral-sh/ruff/pull/24114))
- \[`ruff`\] New rule `useless-finally` (`RUF072`) ([#24165](https://github.com/astral-sh/ruff/pull/24165))
- \[`ruff`\] New rule `f-string-percent-format` (`RUF073`): warn when using `%` operator on an f-string ([#24162](https://github.com/astral-sh/ruff/pull/24162))
- \[`pyflakes`\] Recognize `frozendict` as a builtin for Python 3.15+ ([#24100](https://github.com/astral-sh/ruff/pull/24100))

### Bug fixes

- \[`flake8-async`\] Use fully-qualified `anyio.lowlevel` import in autofix (`ASYNC115`) ([#24166](https://github.com/astral-sh/ruff/pull/24166))
- \[`flake8-bandit`\] Check tuple arguments for partial paths in `S607` ([#24080](https://github.com/astral-sh/ruff/pull/24080))
- \[`pyflakes`\] Skip `undefined-name` (`F821`) for conditionally deleted variables ([#24088](https://github.com/astral-sh/ruff/pull/24088))
- `E501`/`W505`/formatter: Exclude nested pragma comments from line width calculation ([#24071](https://github.com/astral-sh/ruff/pull/24071))
- Fix `%foo?` parsing in IPython assignment expressions ([#24152](https://github.com/astral-sh/ruff/pull/24152))
- `analyze graph`: resolve string imports that reference attributes, not just modules ([#24058](https://github.com/astral-sh/ruff/pull/24058))

### Rule changes

- \[`eradicate`\] ignore `ty: ignore` comments in `ERA001` ([#24192](https://github.com/astral-sh/ruff/pull/24192))
- \[`flake8-bandit`\] Treat `sys.executable` as trusted input in `S603` ([#24106](https://github.com/astral-sh/ruff/pull/24106))
- \[`flake8-self`\] Recognize `Self` annotation and `self` assignment in `SLF001` ([#24144](https://github.com/astral-sh/ruff/pull/24144))
- \[`pyflakes`\] `F507`: Fix false negative for non-tuple RHS in `%`-formatting ([#24142](https://github.com/astral-sh/ruff/pull/24142))
- \[`refurb`\] Parenthesize generator arguments in `FURB142` fixer ([#24200](https://github.com/astral-sh/ruff/pull/24200))

### Performance

- Speed up diagnostic rendering ([#24146](https://github.com/astral-sh/ruff/pull/24146))

### Server

- Warn when Markdown files are skipped due to preview being disabled ([#24150](https://github.com/astral-sh/ruff/pull/24150))

### Documentation

- Clarify `extend-ignore` and `extend-select` settings documentation ([#24064](https://github.com/astral-sh/ruff/pull/24064))
- Mention AI policy in PR template ([#24198](https://github.com/astral-sh/ruff/pull/24198))

### Other changes

- Use trusted publishing for NPM packages ([#24171](https://github.com/astral-sh/ruff/pull/24171))

### Contributors

- [@bitloi](https://github.com/bitloi)
- [@Sim-hu](https://github.com/Sim-hu)
- [@mvanhorn](https://github.com/mvanhorn)
- [@chinar-amrutkar](https://github.com/chinar-amrutkar)
- [@markjm](https://github.com/markjm)
- [@RenzoMXD](https://github.com/RenzoMXD)
- [@vivekkhimani](https://github.com/vivekkhimani)
- [@seroperson](https://github.com/seroperson)
- [@moktamd](https://github.com/moktamd)
- [@charliermarsh](https://github.com/charliermarsh)
- [@ntBre](https://github.com/ntBre)
- [@zanieb](https://github.com/zanieb)
- [@dylwil3](https://github.com/dylwil3)
- [@MichaReiser](https://github.com/MichaReiser)

## 0.15.7

Released on 2026-03-19.

### Preview features

- Display output severity in preview ([#23845](https://github.com/astral-sh/ruff/pull/23845))
- Don't show `noqa` hover for non-Python documents ([#24040](https://github.com/astral-sh/ruff/pull/24040))

### Rule changes

- \[`pycodestyle`\] Recognize `pyrefly:` as a pragma comment (`E501`) ([#24019](https://github.com/astral-sh/ruff/pull/24019))

### Server

- Don't return code actions for non-Python documents ([#23905](https://github.com/astral-sh/ruff/pull/23905))

### Documentation

- Add company AI policy to contributing guide ([#24021](https://github.com/astral-sh/ruff/pull/24021))
- Document editor features for Markdown code formatting ([#23924](https://github.com/astral-sh/ruff/pull/23924))
- \[`pylint`\] Improve phrasing (`PLC0208`) ([#24033](https://github.com/astral-sh/ruff/pull/24033))

### Other changes

- Use PEP 639 license information ([#19661](https://github.com/astral-sh/ruff/pull/19661))

### Contributors

- [@tmimmanuel](https://github.com/tmimmanuel)
- [@DimitriPapadopoulos](https://github.com/DimitriPapadopoulos)
- [@amyreese](https://github.com/amyreese)
- [@statxc](https://github.com/statxc)
- [@dylwil3](https://github.com/dylwil3)
- [@hunterhogan](https://github.com/hunterhogan)
- [@renovate](https://github.com/renovate)

## 0.15.6

Released on 2026-03-12.

### Preview features

- Add support for `lazy` import parsing ([#23755](https://github.com/astral-sh/ruff/pull/23755))
- Add support for star-unpacking of comprehensions (PEP 798) ([#23788](https://github.com/astral-sh/ruff/pull/23788))
- Reject semantic syntax errors for lazy imports ([#23757](https://github.com/astral-sh/ruff/pull/23757))
- Drop a few rules from the preview default set ([#23879](https://github.com/astral-sh/ruff/pull/23879))
- \[`airflow`\] Flag `Variable.get()` calls outside of task execution context (`AIR003`) ([#23584](https://github.com/astral-sh/ruff/pull/23584))
- \[`airflow`\] Flag runtime-varying values in DAG/task constructor arguments (`AIR304`) ([#23631](https://github.com/astral-sh/ruff/pull/23631))
- \[`flake8-bugbear`\] Implement `delattr-with-constant` (`B043`) ([#23737](https://github.com/astral-sh/ruff/pull/23737))
- \[`flake8-tidy-imports`\] Add `TID254` to enforce lazy imports ([#23777](https://github.com/astral-sh/ruff/pull/23777))
- \[`flake8-tidy-imports`\] Allow users to ban lazy imports with `TID254` ([#23847](https://github.com/astral-sh/ruff/pull/23847))
- \[`isort`\] Retain `lazy` keyword when sorting imports ([#23762](https://github.com/astral-sh/ruff/pull/23762))
- \[`pyupgrade`\] Add `from __future__ import annotations` automatically (`UP006`) ([#23260](https://github.com/astral-sh/ruff/pull/23260))
- \[`refurb`\] Support `newline` parameter in `FURB101` for Python 3.13+ ([#23754](https://github.com/astral-sh/ruff/pull/23754))
- \[`ruff`\] Add `os-path-commonprefix` (`RUF071`) ([#23814](https://github.com/astral-sh/ruff/pull/23814))
- \[`ruff`\] Add unsafe fix for os-path-commonprefix (`RUF071`) ([#23852](https://github.com/astral-sh/ruff/pull/23852))
- \[`ruff`\] Limit `RUF036` to typing contexts; make it unsafe for non-typing-only ([#23765](https://github.com/astral-sh/ruff/pull/23765))
- \[`ruff`\] Use starred unpacking for `RUF017` in Python 3.15+ ([#23789](https://github.com/astral-sh/ruff/pull/23789))

### Bug fixes

- Fix `--add-noqa` creating unwanted leading whitespace ([#23773](https://github.com/astral-sh/ruff/pull/23773))
- Fix `--add-noqa` breaking shebangs ([#23577](https://github.com/astral-sh/ruff/pull/23577))
- [formatter] Fix lambda body formatting for multiline calls and subscripts ([#23866](https://github.com/astral-sh/ruff/pull/23866))
- [formatter] Preserve required annotation parentheses in annotated assignments ([#23865](https://github.com/astral-sh/ruff/pull/23865))
- [formatter] Preserve type-expression parentheses in the formatter ([#23867](https://github.com/astral-sh/ruff/pull/23867))
- \[`flake8-annotations`\] Fix stack overflow in `ANN401` on quoted annotations with escape sequences ([#23912](https://github.com/astral-sh/ruff/pull/23912))
- \[`pep8-naming`\] Check naming conventions in `match` pattern bindings (`N806`, `N815`, `N816`) ([#23899](https://github.com/astral-sh/ruff/pull/23899))
- \[`perflint`\] Fix comment duplication in fixes (`PERF401`, `PERF403`) ([#23729](https://github.com/astral-sh/ruff/pull/23729))
- \[`pyupgrade`\] Properly trigger `super` change in nested class (`UP008`) ([#22677](https://github.com/astral-sh/ruff/pull/22677))
- \[`ruff`\] Avoid syntax errors in `RUF036` fixes ([#23764](https://github.com/astral-sh/ruff/pull/23764))

### Rule changes

- \[`flake8-bandit`\] Flag `S501` with `requests.request` ([#23873](https://github.com/astral-sh/ruff/pull/23873))
- \[`flake8-executable`\] Fix WSL detection in non-Docker containers ([#22879](https://github.com/astral-sh/ruff/pull/22879))
- \[`flake8-print`\] Ignore `pprint` calls with `stream=` ([#23787](https://github.com/astral-sh/ruff/pull/23787))

### Documentation

- Update docs for Markdown code block formatting ([#23871](https://github.com/astral-sh/ruff/pull/23871))
- \[`flake8-bugbear`\] Fix misleading description for `B904` ([#23731](https://github.com/astral-sh/ruff/pull/23731))

### Contributors

- [@zsol](https://github.com/zsol)
- [@carljm](https://github.com/carljm)
- [@ntBre](https://github.com/ntBre)
- [@Bortlesboat](https://github.com/Bortlesboat)
- [@sososonia-cyber](https://github.com/sososonia-cyber)
- [@chirizxc](https://github.com/chirizxc)
- [@leandrobbraga](https://github.com/leandrobbraga)
- [@11happy](https://github.com/11happy)
- [@Acelogic](https://github.com/Acelogic)
- [@anishgirianish](https://github.com/anishgirianish)
- [@amyreese](https://github.com/amyreese)
- [@xvchris](https://github.com/xvchris)
- [@charliermarsh](https://github.com/charliermarsh)
- [@getehen](https://github.com/getehen)
- [@Dev-iL](https://github.com/Dev-iL)

## 0.15.5

Released on 2026-03-05.

### Preview features

- Discover Markdown files by default in preview mode ([#23434](https://github.com/astral-sh/ruff/pull/23434))
- \[`perflint`\] Extend `PERF102` to comprehensions and generators ([#23473](https://github.com/astral-sh/ruff/pull/23473))
- \[`refurb`\] Fix `FURB101` and `FURB103` false positives when I/O variable is used later ([#23542](https://github.com/astral-sh/ruff/pull/23542))
- \[`ruff`\] Add fix for `none-not-at-end-of-union` (`RUF036`) ([#22829](https://github.com/astral-sh/ruff/pull/22829))
- \[`ruff`\] Fix false positive for `re.split` with empty string pattern (`RUF055`) ([#23634](https://github.com/astral-sh/ruff/pull/23634))

### Bug fixes

- \[`fastapi`\] Handle callable class dependencies with `__call__` method (`FAST003`) ([#23553](https://github.com/astral-sh/ruff/pull/23553))
- \[`pydocstyle`\] Fix numpy section ordering (`D420`) ([#23685](https://github.com/astral-sh/ruff/pull/23685))
- \[`pyflakes`\] Fix false positive for names shadowing re-exports (`F811`) ([#23356](https://github.com/astral-sh/ruff/pull/23356))
- \[`pyupgrade`\] Avoid inserting redundant `None` elements in `UP045` ([#23459](https://github.com/astral-sh/ruff/pull/23459))

### Documentation

- Document extension mapping for Markdown code formatting ([#23574](https://github.com/astral-sh/ruff/pull/23574))
- Update default Python version examples ([#23605](https://github.com/astral-sh/ruff/pull/23605))

### Other changes

- Publish releases to Astral mirror ([#23616](https://github.com/astral-sh/ruff/pull/23616))

### Contributors

- [@amyreese](https://github.com/amyreese)
- [@stakeswky](https://github.com/stakeswky)
- [@chirizxc](https://github.com/chirizxc)
- [@anishgirianish](https://github.com/anishgirianish)
- [@bxff](https://github.com/bxff)
- [@zsol](https://github.com/zsol)
- [@charliermarsh](https://github.com/charliermarsh)
- [@ntBre](https://github.com/ntBre)
- [@kar-ganap](https://github.com/kar-ganap)

## 0.15.4

Released on 2026-02-26.

This is a follow-up release to 0.15.3 that resolves a panic when the new rule `PLR1712` was enabled with any rule that analyzes definitions, such as many of the `ANN` or `D` rules.

### Bug fixes

- Fix panic on access to definitions after analyzing definitions ([#23588](https://github.com/astral-sh/ruff/pull/23588))
- \[`pyflakes`\] Suppress false positive in `F821` for names used before `del` in stub files ([#23550](https://github.com/astral-sh/ruff/pull/23550))

### Documentation

- Clarify first-party import detection in Ruff ([#23591](https://github.com/astral-sh/ruff/pull/23591))
- Fix incorrect `import-heading` example ([#23568](https://github.com/astral-sh/ruff/pull/23568))

### Contributors

- [@stakeswky](https://github.com/stakeswky)
- [@ntBre](https://github.com/ntBre)
- [@thejcannon](https://github.com/thejcannon)
- [@GeObts](https://github.com/GeObts)

## 0.15.3

Released on 2026-02-26.

### Preview features

- Drop explicit support for `.qmd` file extension ([#23572](https://github.com/astral-sh/ruff/pull/23572))

    This can now be enabled instead by setting the [`extension`](https://docs.astral.sh/ruff/settings/#extension) option:

    ```toml
    # ruff.toml
    extension = { qmd = "markdown" }

    # pyproject.toml
    [tool.ruff]
    extension = { qmd = "markdown" }
    ```

- Include configured extensions in file discovery ([#23400](https://github.com/astral-sh/ruff/pull/23400))

- \[`flake8-bandit`\] Allow suspicious imports in `TYPE_CHECKING` blocks (`S401`-`S415`) ([#23441](https://github.com/astral-sh/ruff/pull/23441))

- \[`flake8-bugbear`\] Allow `B901` in pytest hook wrappers ([#21931](https://github.com/astral-sh/ruff/pull/21931))

- \[`flake8-import-conventions`\] Add missing conventions from upstream (`ICN001`, `ICN002`) ([#21373](https://github.com/astral-sh/ruff/pull/21373))

- \[`pydocstyle`\] Add rule to enforce docstring section ordering (`D420`) ([#23537](https://github.com/astral-sh/ruff/pull/23537))

- \[`pylint`\] Implement `swap-with-temporary-variable` (`PLR1712`) ([#22205](https://github.com/astral-sh/ruff/pull/22205))

- \[`ruff`\] Add `unnecessary-assign-before-yield` (`RUF070`) ([#23300](https://github.com/astral-sh/ruff/pull/23300))

- \[`ruff`\] Support file-level noqa in `RUF102` ([#23535](https://github.com/astral-sh/ruff/pull/23535))

- \[`ruff`\] Suppress diagnostic for invalid f-strings before Python 3.12 (`RUF027`) ([#23480](https://github.com/astral-sh/ruff/pull/23480))

- \[`flake8-bandit`\] Don't flag `BaseLoader`/`CBaseLoader` as unsafe (`S506`) ([#23510](https://github.com/astral-sh/ruff/pull/23510))

### Bug fixes

- Avoid infinite loop between `I002` and `PYI025` ([#23352](https://github.com/astral-sh/ruff/pull/23352))
- \[`pyflakes`\] Fix false positive for `@overload` from `lint.typing-modules` (`F811`) ([#23357](https://github.com/astral-sh/ruff/pull/23357))
- \[`pyupgrade`\] Fix false positive for `TypeVar` default before Python 3.12 (`UP046`) ([#23540](https://github.com/astral-sh/ruff/pull/23540))
- \[`pyupgrade`\] Fix handling of `\N` in raw strings (`UP032`) ([#22149](https://github.com/astral-sh/ruff/pull/22149))

### Rule changes

- Render sub-diagnostics in the GitHub output format ([#23455](https://github.com/astral-sh/ruff/pull/23455))

- \[`flake8-bugbear`\] Tag certain `B007` diagnostics as unnecessary ([#23453](https://github.com/astral-sh/ruff/pull/23453))

- \[`ruff`\] Ignore unknown rule codes in `RUF100` ([#23531](https://github.com/astral-sh/ruff/pull/23531))

    These are now flagged by [`RUF102`](https://docs.astral.sh/ruff/rules/invalid-rule-code/) instead.

### Documentation

- Fix missing settings links for several linters ([#23519](https://github.com/astral-sh/ruff/pull/23519))
- Update isort action comments heading ([#23515](https://github.com/astral-sh/ruff/pull/23515))
- \[`pydocstyle`\] Fix double comma in description of `D404` ([#23440](https://github.com/astral-sh/ruff/pull/23440))

### Other changes

- Update the Python module (notably `find_ruff_bin`) for parity with uv ([#23406](https://github.com/astral-sh/ruff/pull/23406))

### Contributors

- [@zanieb](https://github.com/zanieb)
- [@o1x3](https://github.com/o1x3)
- [@assadyousuf](https://github.com/assadyousuf)
- [@kar-ganap](https://github.com/kar-ganap)
- [@denyszhak](https://github.com/denyszhak)
- [@amyreese](https://github.com/amyreese)
- [@carljm](https://github.com/carljm)
- [@anishgirianish](https://github.com/anishgirianish)
- [@Bnyro](https://github.com/Bnyro)
- [@danparizher](https://github.com/danparizher)
- [@ntBre](https://github.com/ntBre)
- [@gcomneno](https://github.com/gcomneno)
- [@jaap3](https://github.com/jaap3)
- [@stakeswky](https://github.com/stakeswky)

## 0.15.2

Released on 2026-02-19.

### Preview features

- Expand the default rule set ([#23385](https://github.com/astral-sh/ruff/pull/23385))

    In preview, Ruff now enables a significantly expanded default rule set of 412
    rules, up from the stable default set of 59 rules. The new rules are mostly a
    superset of the stable defaults, with the exception of these rules, which are
    removed from the preview defaults:

    - [`multiple-imports-on-one-line`](https://docs.astral.sh/ruff/rules/multiple-imports-on-one-line) (`E401`)
    - [`module-import-not-at-top-of-file`](https://docs.astral.sh/ruff/rules/module-import-not-at-top-of-file) (`E402`)
    - [`module-import-not-at-top-of-file`](https://docs.astral.sh/ruff/rules/module-import-not-at-top-of-file) (`E701`)
    - [`multiple-statements-on-one-line-semicolon`](https://docs.astral.sh/ruff/rules/multiple-statements-on-one-line-semicolon) (`E702`)
    - [`useless-semicolon`](https://docs.astral.sh/ruff/rules/useless-semicolon) (`E703`)
    - [`none-comparison`](https://docs.astral.sh/ruff/rules/none-comparison) (`E711`)
    - [`true-false-comparison`](https://docs.astral.sh/ruff/rules/true-false-comparison) (`E712`)
    - [`not-in-test`](https://docs.astral.sh/ruff/rules/not-in-test) (`E713`)
    - [`not-is-test`](https://docs.astral.sh/ruff/rules/not-is-test) (`E714`)
    - [`type-comparison`](https://docs.astral.sh/ruff/rules/type-comparison) (`E721`)
    - [`lambda-assignment`](https://docs.astral.sh/ruff/rules/lambda-assignment) (`E731`)
    - [`ambiguous-variable-name`](https://docs.astral.sh/ruff/rules/ambiguous-variable-name) (`E741`)
    - [`ambiguous-class-name`](https://docs.astral.sh/ruff/rules/ambiguous-class-name) (`E742`)
    - [`ambiguous-function-name`](https://docs.astral.sh/ruff/rules/ambiguous-function-name) (`E743`)
    - [`undefined-local-with-import-star`](https://docs.astral.sh/ruff/rules/undefined-local-with-import-star) (`F403`)
    - [`undefined-local-with-import-star-usage`](https://docs.astral.sh/ruff/rules/undefined-local-with-import-star-usage) (`F405`)
    - [`undefined-local-with-nested-import-star-usage`](https://docs.astral.sh/ruff/rules/undefined-local-with-nested-import-star-usage) (`F406`)
    - [`forward-annotation-syntax-error`](https://docs.astral.sh/ruff/rules/forward-annotation-syntax-error) (`F722`)

    If you use preview and prefer the old defaults, you can restore them with
    configuration like:

    ```toml

    # ruff.toml

    [lint]
    select = ["E4", "E7", "E9", "F"]

    # pyproject.toml

    [tool.ruff.lint]
    select = ["E4", "E7", "E9", "F"]
    ```

    If you do give them a try, feel free to share your feedback in the [GitHub
    discussion](https://github.com/astral-sh/ruff/discussions/23203)!

- \[`flake8-pyi`\] Also check string annotations (`PYI041`) ([#19023](https://github.com/astral-sh/ruff/pull/19023))

### Bug fixes

- \[`flake8-async`\] Fix `in_async_context` logic ([#23426](https://github.com/astral-sh/ruff/pull/23426))
- \[`ruff`\] Fix for `RUF102` should delete entire comment ([#23380](https://github.com/astral-sh/ruff/pull/23380))
- \[`ruff`\] Suppress diagnostic for strings with backslashes in interpolations before Python 3.12 (`RUF027`) ([#21069](https://github.com/astral-sh/ruff/pull/21069))
- \[`flake8-bugbear`\] Fix `B023` false positive for immediately-invoked lambdas ([#23294](https://github.com/astral-sh/ruff/pull/23294))
- [parser] Fix false syntax error for match-like annotated assignments ([#23297](https://github.com/astral-sh/ruff/pull/23297))
- [parser] Fix indentation tracking after line continuations ([#23417](https://github.com/astral-sh/ruff/pull/23417))

### Rule changes

- \[`flake8-executable`\] Allow global flags in uv shebangs (`EXE003`) ([#22582](https://github.com/astral-sh/ruff/pull/22582))
- \[`pyupgrade`\] Fix handling of `typing.{io,re}` (`UP035`) ([#23131](https://github.com/astral-sh/ruff/pull/23131))
- \[`ruff`\] Detect `PLC0207` on chained `str.split()` calls ([#23275](https://github.com/astral-sh/ruff/pull/23275))

### CLI

- Remove invalid inline `noqa` warning ([#23270](https://github.com/astral-sh/ruff/pull/23270))

### Configuration

- Add extension mapping to configuration file options ([#23384](https://github.com/astral-sh/ruff/pull/23384))

### Documentation

- Add `Q004` to the list of conflicting rules ([#23340](https://github.com/astral-sh/ruff/pull/23340))
- \[`ruff`\] Expand `lint.external` docs and add sub-diagnostic (`RUF100`, `RUF102`) ([#23268](https://github.com/astral-sh/ruff/pull/23268))

### Contributors

- [@dylwil3](https://github.com/dylwil3)
- [@Jkhall81](https://github.com/Jkhall81)
- [@danparizher](https://github.com/danparizher)
- [@dhruvmanila](https://github.com/dhruvmanila)
- [@harupy](https://github.com/harupy)
- [@ngnpope](https://github.com/ngnpope)
- [@amyreese](https://github.com/amyreese)
- [@kar-ganap](https://github.com/kar-ganap)
- [@robsdedude](https://github.com/robsdedude)
- [@shaanmajid](https://github.com/shaanmajid)
- [@ntBre](https://github.com/ntBre)
- [@toslunar](https://github.com/toslunar)

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
