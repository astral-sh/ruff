# Changelog

## 0.11.11

### Preview features

- \[`airflow`\] Add autofixes for `AIR302` and `AIR312` ([#17942](https://github.com/astral-sh/ruff/pull/17942))
- \[`airflow`\] Move rules from `AIR312` to `AIR302` ([#17940](https://github.com/astral-sh/ruff/pull/17940))
- \[`airflow`\] Update `AIR301` and `AIR311` with the latest Airflow implementations ([#17985](https://github.com/astral-sh/ruff/pull/17985))
- \[`flake8-simplify`\] Enable fix in preview mode (`SIM117`) ([#18208](https://github.com/astral-sh/ruff/pull/18208))

### Bug fixes

- Fix inconsistent formatting of match-case on `[]` and `_` ([#18147](https://github.com/astral-sh/ruff/pull/18147))
- \[`pylint`\] Fix `PLW1514` not recognizing the `encoding` positional argument of `codecs.open` ([#18109](https://github.com/astral-sh/ruff/pull/18109))

### CLI

- Add full option name in formatter warning ([#18217](https://github.com/astral-sh/ruff/pull/18217))

### Documentation

- Fix rendering of admonition in docs ([#18163](https://github.com/astral-sh/ruff/pull/18163))
- \[`flake8-print`\] Improve print/pprint docs for `T201` and `T203` ([#18130](https://github.com/astral-sh/ruff/pull/18130))
- \[`flake8-simplify`\] Add fix safety section (`SIM110`,`SIM210`) ([#18114](https://github.com/astral-sh/ruff/pull/18114),[#18100](https://github.com/astral-sh/ruff/pull/18100))
- \[`pylint`\] Fix docs example that produced different output (`PLW0603`) ([#18216](https://github.com/astral-sh/ruff/pull/18216))

## 0.11.10

### Preview features

- \[`ruff`\] Implement a recursive check for `RUF060` ([#17976](https://github.com/astral-sh/ruff/pull/17976))
- \[`airflow`\] Enable autofixes for `AIR301` and `AIR311` ([#17941](https://github.com/astral-sh/ruff/pull/17941))
- \[`airflow`\] Apply try catch guard to all `AIR3` rules ([#17887](https://github.com/astral-sh/ruff/pull/17887))
- \[`airflow`\] Extend `AIR311` rules ([#17913](https://github.com/astral-sh/ruff/pull/17913))

### Bug fixes

- \[`flake8-bugbear`\] Ignore `B028` if `skip_file_prefixes` is present ([#18047](https://github.com/astral-sh/ruff/pull/18047))
- \[`flake8-pie`\] Mark autofix for `PIE804` as unsafe if the dictionary contains comments ([#18046](https://github.com/astral-sh/ruff/pull/18046))
- \[`flake8-simplify`\] Correct behavior for `str.split`/`rsplit` with `maxsplit=0` (`SIM905`) ([#18075](https://github.com/astral-sh/ruff/pull/18075))
- \[`flake8-simplify`\] Fix `SIM905` autofix for `rsplit` creating a reversed list literal ([#18045](https://github.com/astral-sh/ruff/pull/18045))
- \[`flake8-use-pathlib`\] Suppress diagnostics for all `os.*` functions that have the `dir_fd` parameter (`PTH`) ([#17968](https://github.com/astral-sh/ruff/pull/17968))
- \[`refurb`\] Mark autofix as safe only for number literals (`FURB116`) ([#17692](https://github.com/astral-sh/ruff/pull/17692))

### Rule changes

- \[`flake8-bandit`\] Skip `S608` for expressionless f-strings ([#17999](https://github.com/astral-sh/ruff/pull/17999))
- \[`flake8-pytest-style`\] Don't recommend `usefixtures` for `parametrize` values (`PT019`) ([#17650](https://github.com/astral-sh/ruff/pull/17650))
- \[`pyupgrade`\] Add `resource.error` as deprecated alias of `OSError` (`UP024`) ([#17933](https://github.com/astral-sh/ruff/pull/17933))

### CLI

- Disable jemalloc on Android ([#18033](https://github.com/astral-sh/ruff/pull/18033))

### Documentation

- Update Neovim setup docs ([#18108](https://github.com/astral-sh/ruff/pull/18108))
- \[`flake8-simplify`\] Add fix safety section (`SIM103`) ([#18086](https://github.com/astral-sh/ruff/pull/18086))
- \[`flake8-simplify`\] Add fix safety section (`SIM112`) ([#18099](https://github.com/astral-sh/ruff/pull/18099))
- \[`pylint`\] Add fix safety section (`PLC0414`) ([#17802](https://github.com/astral-sh/ruff/pull/17802))
- \[`pylint`\] Add fix safety section (`PLE4703`) ([#17824](https://github.com/astral-sh/ruff/pull/17824))
- \[`pylint`\] Add fix safety section (`PLW1514`) ([#17932](https://github.com/astral-sh/ruff/pull/17932))
- \[`pylint`\] Add fix safety section (`PLW3301`) ([#17878](https://github.com/astral-sh/ruff/pull/17878))
- \[`ruff`\] Add fix safety section (`RUF007`) ([#17755](https://github.com/astral-sh/ruff/pull/17755))
- \[`ruff`\] Add fix safety section (`RUF033`) ([#17760](https://github.com/astral-sh/ruff/pull/17760))

## 0.11.9

### Preview features

- Default to latest supported Python version for version-related syntax errors ([#17529](https://github.com/astral-sh/ruff/pull/17529))
- Implement deferred annotations for Python 3.14 ([#17658](https://github.com/astral-sh/ruff/pull/17658))
- \[`airflow`\] Fix `SQLTableCheckOperator` typo (`AIR302`) ([#17946](https://github.com/astral-sh/ruff/pull/17946))
- \[`airflow`\] Remove `airflow.utils.dag_parsing_context.get_parsing_context` (`AIR301`) ([#17852](https://github.com/astral-sh/ruff/pull/17852))
- \[`airflow`\] Skip attribute check in try catch block (`AIR301`) ([#17790](https://github.com/astral-sh/ruff/pull/17790))
- \[`flake8-bandit`\] Mark tuples of string literals as trusted input in `S603` ([#17801](https://github.com/astral-sh/ruff/pull/17801))
- \[`isort`\] Check full module path against project root(s) when categorizing first-party imports ([#16565](https://github.com/astral-sh/ruff/pull/16565))
- \[`ruff`\] Add new rule `in-empty-collection` (`RUF060`) ([#16480](https://github.com/astral-sh/ruff/pull/16480))

### Bug fixes

- Fix missing `combine` call for `lint.typing-extensions` setting ([#17823](https://github.com/astral-sh/ruff/pull/17823))
- \[`flake8-async`\] Fix module name in `ASYNC110`, `ASYNC115`, and `ASYNC116` fixes ([#17774](https://github.com/astral-sh/ruff/pull/17774))
- \[`pyupgrade`\] Add spaces between tokens as necessary to avoid syntax errors in `UP018` autofix ([#17648](https://github.com/astral-sh/ruff/pull/17648))
- \[`refurb`\] Fix false positive for float and complex numbers in `FURB116` ([#17661](https://github.com/astral-sh/ruff/pull/17661))
- [parser] Flag single unparenthesized generator expr with trailing comma in arguments. ([#17893](https://github.com/astral-sh/ruff/pull/17893))

### Documentation

- Add instructions on how to upgrade to a newer Rust version ([#17928](https://github.com/astral-sh/ruff/pull/17928))
- Update code of conduct email address ([#17875](https://github.com/astral-sh/ruff/pull/17875))
- Add fix safety sections to `PLC2801`, `PLR1722`, and `RUF013` ([#17825](https://github.com/astral-sh/ruff/pull/17825), [#17826](https://github.com/astral-sh/ruff/pull/17826), [#17759](https://github.com/astral-sh/ruff/pull/17759))
- Add link to `check-typed-exception` from `S110` and `S112` ([#17786](https://github.com/astral-sh/ruff/pull/17786))

### Other changes

- Allow passing a virtual environment to `ruff analyze graph` ([#17743](https://github.com/astral-sh/ruff/pull/17743))

## 0.11.8

### Preview features

- \[`airflow`\] Apply auto fixes to cases where the names have changed in Airflow 3 (`AIR302`, `AIR311`) ([#17553](https://github.com/astral-sh/ruff/pull/17553), [#17570](https://github.com/astral-sh/ruff/pull/17570), [#17571](https://github.com/astral-sh/ruff/pull/17571))
- \[`airflow`\] Extend `AIR301` rule ([#17598](https://github.com/astral-sh/ruff/pull/17598))
- \[`airflow`\] Update existing `AIR302` rules with better suggestions ([#17542](https://github.com/astral-sh/ruff/pull/17542))
- \[`refurb`\] Mark fix as safe for `readlines-in-for` (`FURB129`) ([#17644](https://github.com/astral-sh/ruff/pull/17644))
- [syntax-errors] `nonlocal` declaration at module level ([#17559](https://github.com/astral-sh/ruff/pull/17559))
- [syntax-errors] Detect single starred expression assignment `x = *y` ([#17624](https://github.com/astral-sh/ruff/pull/17624))

### Bug fixes

- \[`flake8-pyi`\] Ensure `Literal[None,] | Literal[None,]` is not autofixed to `None | None` (`PYI061`) ([#17659](https://github.com/astral-sh/ruff/pull/17659))
- \[`flake8-use-pathlib`\] Avoid suggesting `Path.iterdir()` for `os.listdir` with file descriptor (`PTH208`) ([#17715](https://github.com/astral-sh/ruff/pull/17715))
- \[`flake8-use-pathlib`\] Fix `PTH104` false positive when `rename` is passed a file descriptor ([#17712](https://github.com/astral-sh/ruff/pull/17712))
- \[`flake8-use-pathlib`\] Fix `PTH116` false positive when `stat` is passed a file descriptor ([#17709](https://github.com/astral-sh/ruff/pull/17709))
- \[`flake8-use-pathlib`\] Fix `PTH123` false positive when `open` is passed a file descriptor from a function call ([#17705](https://github.com/astral-sh/ruff/pull/17705))
- \[`pycodestyle`\] Fix duplicated diagnostic in `E712` ([#17651](https://github.com/astral-sh/ruff/pull/17651))
- \[`pylint`\] Detect `global` declarations in module scope (`PLE0118`) ([#17411](https://github.com/astral-sh/ruff/pull/17411))
- [syntax-errors] Make `async-comprehension-in-sync-comprehension` more specific ([#17460](https://github.com/astral-sh/ruff/pull/17460))

### Configuration

- Add option to disable `typing_extensions` imports ([#17611](https://github.com/astral-sh/ruff/pull/17611))

### Documentation

- Fix example syntax for the `lint.pydocstyle.ignore-var-parameters` option ([#17740](https://github.com/astral-sh/ruff/pull/17740))
- Add fix safety sections (`ASYNC116`, `FLY002`, `D200`, `RUF005`, `RUF017`, `RUF027`, `RUF028`, `RUF057`) ([#17497](https://github.com/astral-sh/ruff/pull/17497), [#17496](https://github.com/astral-sh/ruff/pull/17496), [#17502](https://github.com/astral-sh/ruff/pull/17502), [#17484](https://github.com/astral-sh/ruff/pull/17484), [#17480](https://github.com/astral-sh/ruff/pull/17480), [#17485](https://github.com/astral-sh/ruff/pull/17485), [#17722](https://github.com/astral-sh/ruff/pull/17722), [#17483](https://github.com/astral-sh/ruff/pull/17483))

### Other changes

- Add Python 3.14 to configuration options ([#17647](https://github.com/astral-sh/ruff/pull/17647))
- Make syntax error for unparenthesized except tuples version specific to before 3.14 ([#17660](https://github.com/astral-sh/ruff/pull/17660))

## 0.11.7

### Preview features

- \[`airflow`\] Apply auto fixes to cases where the names have changed in Airflow 3 (`AIR301`) ([#17355](https://github.com/astral-sh/ruff/pull/17355))
- \[`perflint`\] Implement fix for `manual-dict-comprehension` (`PERF403`) ([#16719](https://github.com/astral-sh/ruff/pull/16719))
- [syntax-errors] Make duplicate parameter names a semantic error ([#17131](https://github.com/astral-sh/ruff/pull/17131))

### Bug fixes

- \[`airflow`\] Fix typos in provider package names (`AIR302`, `AIR312`) ([#17574](https://github.com/astral-sh/ruff/pull/17574))
- \[`flake8-type-checking`\] Visit keyword arguments in checks involving `typing.cast`/`typing.NewType` arguments ([#17538](https://github.com/astral-sh/ruff/pull/17538))
- \[`pyupgrade`\] Preserve parenthesis when fixing native literals containing newlines (`UP018`) ([#17220](https://github.com/astral-sh/ruff/pull/17220))
- \[`refurb`\] Mark the `FURB161` fix unsafe except for integers and booleans ([#17240](https://github.com/astral-sh/ruff/pull/17240))

### Rule changes

- \[`perflint`\] Allow list function calls to be replaced with a comprehension (`PERF401`) ([#17519](https://github.com/astral-sh/ruff/pull/17519))
- \[`pycodestyle`\] Auto-fix redundant boolean comparison (`E712`) ([#17090](https://github.com/astral-sh/ruff/pull/17090))
- \[`pylint`\] make fix unsafe if delete comments (`PLR1730`) ([#17459](https://github.com/astral-sh/ruff/pull/17459))

### Documentation

- Add fix safety sections to docs for several rules ([#17410](https://github.com/astral-sh/ruff/pull/17410),[#17440](https://github.com/astral-sh/ruff/pull/17440),[#17441](https://github.com/astral-sh/ruff/pull/17441),[#17443](https://github.com/astral-sh/ruff/pull/17443),[#17444](https://github.com/astral-sh/ruff/pull/17444))

## 0.11.6

### Preview features

- Avoid adding whitespace to the end of a docstring after an escaped quote ([#17216](https://github.com/astral-sh/ruff/pull/17216))
- \[`airflow`\] Extract `AIR311` from `AIR301` rules (`AIR301`, `AIR311`) ([#17310](https://github.com/astral-sh/ruff/pull/17310), [#17422](https://github.com/astral-sh/ruff/pull/17422))

### Bug fixes

- Raise syntax error when `\` is at end of file ([#17409](https://github.com/astral-sh/ruff/pull/17409))

## 0.11.5

### Preview features

- \[`airflow`\] Add missing `AIR302` attribute check ([#17115](https://github.com/astral-sh/ruff/pull/17115))
- \[`airflow`\] Expand module path check to individual symbols (`AIR302`) ([#17278](https://github.com/astral-sh/ruff/pull/17278))
- \[`airflow`\] Extract `AIR312` from `AIR302` rules (`AIR302`, `AIR312`) ([#17152](https://github.com/astral-sh/ruff/pull/17152))
- \[`airflow`\] Update oudated `AIR301`, `AIR302` rules ([#17123](https://github.com/astral-sh/ruff/pull/17123))
- [syntax-errors] Async comprehension in sync comprehension ([#17177](https://github.com/astral-sh/ruff/pull/17177))
- [syntax-errors] Check annotations in annotated assignments ([#17283](https://github.com/astral-sh/ruff/pull/17283))
- [syntax-errors] Extend annotation checks to `await` ([#17282](https://github.com/astral-sh/ruff/pull/17282))

### Bug fixes

- \[`flake8-pie`\] Avoid false positive for multiple assignment with `auto()` (`PIE796`) ([#17274](https://github.com/astral-sh/ruff/pull/17274))

### Rule changes

- \[`ruff`\] Fix `RUF100` to detect unused file-level `noqa` directives with specific codes (#17042) ([#17061](https://github.com/astral-sh/ruff/pull/17061))
- \[`flake8-pytest-style`\] Avoid false positive for legacy form of `pytest.raises` (`PT011`) ([#17231](https://github.com/astral-sh/ruff/pull/17231))

### Documentation

- Fix formatting of "See Style Guide" link ([#17272](https://github.com/astral-sh/ruff/pull/17272))

## 0.11.4

### Preview features

- \[`ruff`\] Implement `invalid-rule-code` as `RUF102` ([#17138](https://github.com/astral-sh/ruff/pull/17138))
- [syntax-errors] Detect duplicate keys in `match` mapping patterns ([#17129](https://github.com/astral-sh/ruff/pull/17129))
- [syntax-errors] Detect duplicate attributes in `match` class patterns ([#17186](https://github.com/astral-sh/ruff/pull/17186))
- [syntax-errors] Detect invalid syntax in annotations ([#17101](https://github.com/astral-sh/ruff/pull/17101))

### Bug fixes

- [syntax-errors] Fix multiple assignment error for class fields in `match` patterns ([#17184](https://github.com/astral-sh/ruff/pull/17184))
- Don't skip visiting non-tuple slice in `typing.Annotated` subscripts ([#17201](https://github.com/astral-sh/ruff/pull/17201))

## 0.11.3

### Preview features

- \[`airflow`\] Add more autofixes for `AIR302` ([#16876](https://github.com/astral-sh/ruff/pull/16876), [#16977](https://github.com/astral-sh/ruff/pull/16977), [#16976](https://github.com/astral-sh/ruff/pull/16976), [#16965](https://github.com/astral-sh/ruff/pull/16965))
- \[`airflow`\] Move `AIR301` to `AIR002` ([#16978](https://github.com/astral-sh/ruff/pull/16978))
- \[`airflow`\] Move `AIR302` to `AIR301` and `AIR303` to `AIR302` ([#17151](https://github.com/astral-sh/ruff/pull/17151))
- \[`flake8-bandit`\] Mark `str` and `list[str]` literals as trusted input (`S603`) ([#17136](https://github.com/astral-sh/ruff/pull/17136))
- \[`ruff`\] Support slices in `RUF005` ([#17078](https://github.com/astral-sh/ruff/pull/17078))
- [syntax-errors] Start detecting compile-time syntax errors ([#16106](https://github.com/astral-sh/ruff/pull/16106))
- [syntax-errors] Duplicate type parameter names ([#16858](https://github.com/astral-sh/ruff/pull/16858))
- [syntax-errors] Irrefutable `case` pattern before final case ([#16905](https://github.com/astral-sh/ruff/pull/16905))
- [syntax-errors] Multiple assignments in `case` pattern ([#16957](https://github.com/astral-sh/ruff/pull/16957))
- [syntax-errors] Single starred assignment target ([#17024](https://github.com/astral-sh/ruff/pull/17024))
- [syntax-errors] Starred expressions in `return`, `yield`, and `for` ([#17134](https://github.com/astral-sh/ruff/pull/17134))
- [syntax-errors] Store to or delete `__debug__` ([#16984](https://github.com/astral-sh/ruff/pull/16984))

### Bug fixes

- Error instead of `panic!` when running Ruff from a deleted directory (#16903) ([#17054](https://github.com/astral-sh/ruff/pull/17054))
- [syntax-errors] Fix false positive for parenthesized tuple index ([#16948](https://github.com/astral-sh/ruff/pull/16948))

### CLI

- Check `pyproject.toml` correctly when it is passed via stdin ([#16971](https://github.com/astral-sh/ruff/pull/16971))

### Configuration

- \[`flake8-import-conventions`\] Add import `numpy.typing as npt` to default `flake8-import-conventions.aliases` ([#17133](https://github.com/astral-sh/ruff/pull/17133))

### Documentation

- \[`refurb`\] Document why `UserDict`, `UserList`, and `UserString` are preferred over `dict`, `list`, and `str` (`FURB189`) ([#16927](https://github.com/astral-sh/ruff/pull/16927))

## 0.11.2

### Preview features

- [syntax-errors] Fix false-positive syntax errors emitted for annotations on variadic parameters before Python 3.11 ([#16878](https://github.com/astral-sh/ruff/pull/16878))

## 0.11.1

### Preview features

- \[`airflow`\] Add `chain`, `chain_linear` and `cross_downstream` for `AIR302` ([#16647](https://github.com/astral-sh/ruff/pull/16647))
- [syntax-errors] Improve error message and range for pre-PEP-614 decorator syntax errors ([#16581](https://github.com/astral-sh/ruff/pull/16581))
- [syntax-errors] PEP 701 f-strings before Python 3.12 ([#16543](https://github.com/astral-sh/ruff/pull/16543))
- [syntax-errors] Parenthesized context managers before Python 3.9 ([#16523](https://github.com/astral-sh/ruff/pull/16523))
- [syntax-errors] Star annotations before Python 3.11 ([#16545](https://github.com/astral-sh/ruff/pull/16545))
- [syntax-errors] Star expression in index before Python 3.11 ([#16544](https://github.com/astral-sh/ruff/pull/16544))
- [syntax-errors] Unparenthesized assignment expressions in sets and indexes ([#16404](https://github.com/astral-sh/ruff/pull/16404))

### Bug fixes

- Server: Allow `FixAll` action in presence of version-specific syntax errors ([#16848](https://github.com/astral-sh/ruff/pull/16848))
- \[`flake8-bandit`\] Allow raw strings in `suspicious-mark-safe-usage` (`S308`) #16702 ([#16770](https://github.com/astral-sh/ruff/pull/16770))
- \[`refurb`\] Avoid panicking `unwrap` in `verbose-decimal-constructor` (`FURB157`) ([#16777](https://github.com/astral-sh/ruff/pull/16777))
- \[`refurb`\] Fix starred expressions fix (`FURB161`) ([#16550](https://github.com/astral-sh/ruff/pull/16550))
- Fix `--statistics` reporting for unsafe fixes ([#16756](https://github.com/astral-sh/ruff/pull/16756))

### Rule changes

- \[`flake8-executables`\] Allow `uv run` in shebang line for `shebang-missing-python` (`EXE003`) ([#16849](https://github.com/astral-sh/ruff/pull/16849),[#16855](https://github.com/astral-sh/ruff/pull/16855))

### CLI

- Add `--exit-non-zero-on-format` ([#16009](https://github.com/astral-sh/ruff/pull/16009))

### Documentation

- Update Ruff tutorial to avoid non-existent fix in `__init__.py` ([#16818](https://github.com/astral-sh/ruff/pull/16818))
- \[`flake8-gettext`\] Swap `format-` and `printf-in-get-text-func-call` examples (`INT002`, `INT003`) ([#16769](https://github.com/astral-sh/ruff/pull/16769))

## 0.11.0

This is a follow-up to release 0.10.0. Because of a mistake in the release process, the `requires-python` inference changes were not included in that release. Ruff 0.11.0 now includes this change as well as the stabilization of the preview behavior for `PGH004`.

### Breaking changes

- **Changes to how the Python version is inferred when a `target-version` is not specified** ([#16319](https://github.com/astral-sh/ruff/pull/16319))

    In previous versions of Ruff, you could specify your Python version with:

    - The `target-version` option in a `ruff.toml` file or the `[tool.ruff]` section of a pyproject.toml file.
    - The `project.requires-python` field in a `pyproject.toml` file with a `[tool.ruff]` section.

    These options worked well in most cases, and are still recommended for fine control of the Python version. However, because of the way Ruff discovers config files, `pyproject.toml` files without a `[tool.ruff]` section would be ignored, including the `requires-python` setting. Ruff would then use the default Python version (3.9 as of this writing) instead, which is surprising when you've attempted to request another version.

    In v0.10, config discovery has been updated to address this issue:

    - If Ruff finds a `ruff.toml` file without a `target-version`, it will check
        for a `pyproject.toml` file in the same directory and respect its
        `requires-python` version, even if it does not contain a `[tool.ruff]`
        section.
    - If Ruff finds a user-level configuration, the `requires-python` field of the closest `pyproject.toml` in a parent directory will take precedence.
    - If there is no config file (`ruff.toml`or `pyproject.toml` with a
        `[tool.ruff]` section) in the directory of the file being checked, Ruff will
        search for the closest `pyproject.toml` in the parent directories and use its
        `requires-python` setting.

### Stabilization

The following behaviors have been stabilized:

- [`blanket-noqa`](https://docs.astral.sh/ruff/rules/blanket-noqa/) (`PGH004`): Also detect blanked file-level noqa comments (and not just line level comments).

### Preview features

- [syntax-errors] Tuple unpacking in `for` statement iterator clause before Python 3.9 ([#16558](https://github.com/astral-sh/ruff/pull/16558))

## 0.10.0

Check out the [blog post](https://astral.sh/blog/ruff-v0.10.0) for a migration guide and overview of the changes!

### Breaking changes

See also, the "Remapped rules" section which may result in disabled rules.

- **Changes to how the Python version is inferred when a `target-version` is not specified** ([#16319](https://github.com/astral-sh/ruff/pull/16319))

    Because of a mistake in the release process, the `requires-python` inference changes are not included in this release and instead shipped as part of 0.11.0.
    You can find a description of this change in the 0.11.0 section.

- **Updated `TYPE_CHECKING` behavior** ([#16669](https://github.com/astral-sh/ruff/pull/16669))

    Previously, Ruff only recognized typechecking blocks that tested the `typing.TYPE_CHECKING` symbol. Now, Ruff recognizes any local variable named `TYPE_CHECKING`. This release also removes support for the legacy `if 0:` and `if False:` typechecking checks. Use a local `TYPE_CHECKING` variable instead.

- **More robust noqa parsing** ([#16483](https://github.com/astral-sh/ruff/pull/16483))

    The syntax for both file-level and in-line suppression comments has been unified and made more robust to certain errors. In most cases, this will result in more suppression comments being read by Ruff, but there are a few instances where previously read comments will now log an error to the user instead. Please refer to the documentation on [*Error suppression*](https://docs.astral.sh/ruff/linter/#error-suppression) for the full specification.

- **Avoid unnecessary parentheses around with statements with a single context manager and a trailing comment** ([#14005](https://github.com/astral-sh/ruff/pull/14005))

    This change fixes a bug in the formatter where it introduced unnecessary parentheses around with statements with a single context manager and a trailing comment. This change may result in a change in formatting for some users.

- **Bump alpine default tag to 3.21 for derived Docker images** ([#16456](https://github.com/astral-sh/ruff/pull/16456))

    Alpine 3.21 was released in Dec 2024 and is used in the official Alpine-based Python images. Now the ruff:alpine image will use 3.21 instead of 3.20 and ruff:alpine3.20 will no longer be updated.

### Deprecated Rules

The following rules have been deprecated:

- [`non-pep604-isinstance`](https://docs.astral.sh/ruff/rules/non-pep604-isinstance/) (`UP038`)
- [`suspicious-xmle-tree-usage`](https://docs.astral.sh/ruff/rules/suspicious-xmle-tree-usage/) (`S320`)

### Remapped rules

The following rules have been remapped to new rule codes:

- \[`unsafe-markup-use`\]: `RUF035` to `S704`

### Stabilization

The following rules have been stabilized and are no longer in preview:

- [`batched-without-explicit-strict`](https://docs.astral.sh/ruff/rules/batched-without-explicit-strict) (`B911`)
- [`unnecessary-dict-comprehension-for-iterable`](https://docs.astral.sh/ruff/rules/unnecessary-dict-comprehension-for-iterable) (`C420`)
- [`datetime-min-max`](https://docs.astral.sh/ruff/rules/datetime-min-max) (`DTZ901`)
- [`fast-api-unused-path-parameter`](https://docs.astral.sh/ruff/rules/fast-api-unused-path-parameter) (`FAST003`)
- [`root-logger-call`](https://docs.astral.sh/ruff/rules/root-logger-call) (`LOG015`)
- [`len-test`](https://docs.astral.sh/ruff/rules/len-test) (`PLC1802`)
- [`shallow-copy-environ`](https://docs.astral.sh/ruff/rules/shallow-copy-environ) (`PLW1507`)
- [`os-listdir`](https://docs.astral.sh/ruff/rules/os-listdir) (`PTH208`)
- [`invalid-pathlib-with-suffix`](https://docs.astral.sh/ruff/rules/invalid-pathlib-with-suffix) (`PTH210`)
- [`invalid-assert-message-literal-argument`](https://docs.astral.sh/ruff/rules/invalid-assert-message-literal-argument) (`RUF040`)
- [`unnecessary-nested-literal`](https://docs.astral.sh/ruff/rules/unnecessary-nested-literal) (`RUF041`)
- [`unnecessary-cast-to-int`](https://docs.astral.sh/ruff/rules/unnecessary-cast-to-int) (`RUF046`)
- [`map-int-version-parsing`](https://docs.astral.sh/ruff/rules/map-int-version-parsing) (`RUF048`)
- [`if-key-in-dict-del`](https://docs.astral.sh/ruff/rules/if-key-in-dict-del) (`RUF051`)
- [`unsafe-markup-use`](https://docs.astral.sh/ruff/rules/unsafe-markup-use) (`S704`). This rule has also been renamed from `RUF035`.
- [`split-static-string`](https://docs.astral.sh/ruff/rules/split-static-string) (`SIM905`)
- [`runtime-cast-value`](https://docs.astral.sh/ruff/rules/runtime-cast-value) (`TC006`)
- [`unquoted-type-alias`](https://docs.astral.sh/ruff/rules/unquoted-type-alias) (`TC007`)
- [`non-pep646-unpack`](https://docs.astral.sh/ruff/rules/non-pep646-unpack) (`UP044`)

The following behaviors have been stabilized:

- [`bad-staticmethod-argument`](https://docs.astral.sh/ruff/rules/bad-staticmethod-argument/) (`PLW0211`) [`invalid-first-argument-name-for-class-method`](https://docs.astral.sh/ruff/rules/invalid-first-argument-name-for-class-method/) (`N804`): `__new__` methods are now no longer flagged by `invalid-first-argument-name-for-class-method` (`N804`) but instead by `bad-staticmethod-argument` (`PLW0211`)
- [`bad-str-strip-call`](https://docs.astral.sh/ruff/rules/bad-str-strip-call/) (`PLE1310`): The rule now applies to objects which are known to have type `str` or `bytes`.
- [`custom-type-var-for-self`](https://docs.astral.sh/ruff/rules/custom-type-var-for-self/) (`PYI019`): More accurate detection of custom `TypeVars` replaceable by `Self`. The range of the diagnostic is now the full function header rather than just the return annotation.
- [`invalid-argument-name`](https://docs.astral.sh/ruff/rules/invalid-argument-name/) (`N803`): Ignore argument names of functions decorated with `typing.override`
- [`invalid-envvar-default`](https://docs.astral.sh/ruff/rules/invalid-envvar-default/) (`PLW1508`): Detect default value arguments to `os.environ.get` with invalid type.
- [`pytest-raises-with-multiple-statements`](https://docs.astral.sh/ruff/rules/pytest-raises-with-multiple-statements/) (`PT012`) [`pytest-warns-with-multiple-statements`](https://docs.astral.sh/ruff/rules/pytest-warns-with-multiple-statements/) (`PT031`): Allow `for` statements with an empty body in `pytest.raises` and `pytest.warns` `with` statements.
- [`redundant-open-modes`](https://docs.astral.sh/ruff/rules/redundant-open-modes/) (`UP015`): The diagnostic range is now the range of the redundant mode argument where it previously was the range of the entire open call. You may have to replace your `noqa` comments when suppressing `UP015`.
- [`stdlib-module-shadowing`](https://docs.astral.sh/ruff/rules/stdlib-module-shadowing/) (`A005`): Changes the default value of `lint.flake8-builtins.strict-checking` from `true` to `false`.
- [`type-none-comparison`](https://docs.astral.sh/ruff/rules/type-none-comparison/) (`FURB169`): Now also recognizes `type(expr) is type(None)` comparisons where `expr` isn't a name expression.

The following fixes or improvements to fixes have been stabilized:

- [`repeated-equality-comparison`](https://docs.astral.sh/ruff/rules/repeated-equality-comparison/) (`PLR1714`) ([#16685](https://github.com/astral-sh/ruff/pull/16685))
- [`needless-bool`](https://docs.astral.sh/ruff/rules/needless-bool/) (`SIM103`) ([#16684](https://github.com/astral-sh/ruff/pull/16684))
- [`unused-private-type-var`](https://docs.astral.sh/ruff/rules/unused-private-type-var/) (`PYI018`) ([#16682](https://github.com/astral-sh/ruff/pull/16682))

### Server

- Remove logging output for `ruff.printDebugInformation` ([#16617](https://github.com/astral-sh/ruff/pull/16617))

### Configuration

- \[`flake8-builtins`\] Deprecate the `builtins-` prefixed options in favor of the unprefixed options (e.g. `builtins-allowed-modules` is now deprecated in favor of `allowed-modules`) ([#16092](https://github.com/astral-sh/ruff/pull/16092))

### Bug fixes

- [flake8-bandit] Fix mixed-case hash algorithm names (S324) ([#16552](https://github.com/astral-sh/ruff/pull/16552))

### CLI

- [ruff] Fix `last_tag`/`commits_since_last_tag` for `version` command ([#16686](https://github.com/astral-sh/ruff/pull/16686))

## 0.9.10

### Preview features

- \[`ruff`\] Add new rule `RUF059`: Unused unpacked assignment ([#16449](https://github.com/astral-sh/ruff/pull/16449))
- \[`syntax-errors`\] Detect assignment expressions before Python 3.8 ([#16383](https://github.com/astral-sh/ruff/pull/16383))
- \[`syntax-errors`\] Named expressions in decorators before Python 3.9 ([#16386](https://github.com/astral-sh/ruff/pull/16386))
- \[`syntax-errors`\] Parenthesized keyword argument names after Python 3.8 ([#16482](https://github.com/astral-sh/ruff/pull/16482))
- \[`syntax-errors`\] Positional-only parameters before Python 3.8 ([#16481](https://github.com/astral-sh/ruff/pull/16481))
- \[`syntax-errors`\] Tuple unpacking in `return` and `yield` before Python 3.8 ([#16485](https://github.com/astral-sh/ruff/pull/16485))
- \[`syntax-errors`\] Type parameter defaults before Python 3.13 ([#16447](https://github.com/astral-sh/ruff/pull/16447))
- \[`syntax-errors`\] Type parameter lists before Python 3.12 ([#16479](https://github.com/astral-sh/ruff/pull/16479))
- \[`syntax-errors`\] `except*` before Python 3.11 ([#16446](https://github.com/astral-sh/ruff/pull/16446))
- \[`syntax-errors`\] `type` statements before Python 3.12 ([#16478](https://github.com/astral-sh/ruff/pull/16478))

### Bug fixes

- Escape template filenames in glob patterns in configuration ([#16407](https://github.com/astral-sh/ruff/pull/16407))
- \[`flake8-simplify`\] Exempt unittest context methods for `SIM115` rule ([#16439](https://github.com/astral-sh/ruff/pull/16439))
- Formatter: Fix syntax error location in notebooks ([#16499](https://github.com/astral-sh/ruff/pull/16499))
- \[`pyupgrade`\] Do not offer fix when at least one target is `global`/`nonlocal` (`UP028`) ([#16451](https://github.com/astral-sh/ruff/pull/16451))
- \[`flake8-builtins`\] Ignore variables matching module attribute names (`A001`) ([#16454](https://github.com/astral-sh/ruff/pull/16454))
- \[`pylint`\] Convert `code` keyword argument to a positional argument in fix for (`PLR1722`) ([#16424](https://github.com/astral-sh/ruff/pull/16424))

### CLI

- Move rule code from `description` to `check_name` in GitLab output serializer ([#16437](https://github.com/astral-sh/ruff/pull/16437))

### Documentation

- \[`pydocstyle`\] Clarify that `D417` only checks docstrings with an arguments section ([#16494](https://github.com/astral-sh/ruff/pull/16494))

## 0.9.9

### Preview features

- Fix caching of unsupported-syntax errors ([#16425](https://github.com/astral-sh/ruff/pull/16425))

### Bug fixes

- Only show unsupported-syntax errors in editors when preview mode is enabled ([#16429](https://github.com/astral-sh/ruff/pull/16429))

## 0.9.8

### Preview features

- Start detecting version-related syntax errors in the parser ([#16090](https://github.com/astral-sh/ruff/pull/16090))

### Rule changes

- \[`pylint`\] Mark fix unsafe (`PLW1507`) ([#16343](https://github.com/astral-sh/ruff/pull/16343))
- \[`pylint`\] Catch `case np.nan`/`case math.nan` in `match` statements (`PLW0177`) ([#16378](https://github.com/astral-sh/ruff/pull/16378))
- \[`ruff`\] Add more Pydantic models variants to the list of default copy semantics (`RUF012`) ([#16291](https://github.com/astral-sh/ruff/pull/16291))

### Server

- Avoid indexing the project if `configurationPreference` is `editorOnly` ([#16381](https://github.com/astral-sh/ruff/pull/16381))
- Avoid unnecessary info at non-trace server log level ([#16389](https://github.com/astral-sh/ruff/pull/16389))
- Expand `ruff.configuration` to allow inline config ([#16296](https://github.com/astral-sh/ruff/pull/16296))
- Notify users for invalid client settings ([#16361](https://github.com/astral-sh/ruff/pull/16361))

### Configuration

- Add `per-file-target-version` option ([#16257](https://github.com/astral-sh/ruff/pull/16257))

### Bug fixes

- \[`refurb`\] Do not consider docstring(s) (`FURB156`) ([#16391](https://github.com/astral-sh/ruff/pull/16391))
- \[`flake8-self`\] Ignore attribute accesses on instance-like variables (`SLF001`) ([#16149](https://github.com/astral-sh/ruff/pull/16149))
- \[`pylint`\] Fix false positives, add missing methods, and support positional-only parameters (`PLE0302`) ([#16263](https://github.com/astral-sh/ruff/pull/16263))
- \[`flake8-pyi`\] Mark `PYI030` fix unsafe when comments are deleted ([#16322](https://github.com/astral-sh/ruff/pull/16322))

### Documentation

- Fix example for `S611` ([#16316](https://github.com/astral-sh/ruff/pull/16316))
- Normalize inconsistent markdown headings in docstrings ([#16364](https://github.com/astral-sh/ruff/pull/16364))
- Document MSRV policy ([#16384](https://github.com/astral-sh/ruff/pull/16384))

## 0.9.7

### Preview features

- Consider `__new__` methods as special function type for enforcing class method or static method rules ([#13305](https://github.com/astral-sh/ruff/pull/13305))
- \[`airflow`\] Improve the internal logic to differentiate deprecated symbols (`AIR303`) ([#16013](https://github.com/astral-sh/ruff/pull/16013))
- \[`refurb`\] Manual timezone monkeypatching (`FURB162`) ([#16113](https://github.com/astral-sh/ruff/pull/16113))
- \[`ruff`\] Implicit class variable in dataclass (`RUF045`) ([#14349](https://github.com/astral-sh/ruff/pull/14349))
- \[`ruff`\] Skip singleton starred expressions for `incorrectly-parenthesized-tuple-in-subscript` (`RUF031`) ([#16083](https://github.com/astral-sh/ruff/pull/16083))
- \[`refurb`\] Check for subclasses includes subscript expressions (`FURB189`) ([#16155](https://github.com/astral-sh/ruff/pull/16155))

### Rule changes

- \[`flake8-debugger`\] Also flag `sys.breakpointhook` and `sys.__breakpointhook__` (`T100`) ([#16191](https://github.com/astral-sh/ruff/pull/16191))
- \[`pycodestyle`\] Exempt `site.addsitedir(...)` calls (`E402`) ([#16251](https://github.com/astral-sh/ruff/pull/16251))

### Formatter

- Fix unstable formatting of trailing end-of-line comments of parenthesized attribute values ([#16187](https://github.com/astral-sh/ruff/pull/16187))

### Server

- Fix handling of requests received after shutdown message ([#16262](https://github.com/astral-sh/ruff/pull/16262))
- Ignore `source.organizeImports.ruff` and `source.fixAll.ruff` code actions for a notebook cell ([#16154](https://github.com/astral-sh/ruff/pull/16154))
- Include document specific debug info for `ruff.printDebugInformation` ([#16215](https://github.com/astral-sh/ruff/pull/16215))
- Update server to return the debug info as string with `ruff.printDebugInformation` ([#16214](https://github.com/astral-sh/ruff/pull/16214))

### CLI

- Warn on invalid `noqa` even when there are no diagnostics ([#16178](https://github.com/astral-sh/ruff/pull/16178))
- Better error messages while loading configuration `extend`s ([#15658](https://github.com/astral-sh/ruff/pull/15658))

### Bug fixes

- \[`flake8-comprehensions`\] Handle trailing comma in `C403` fix ([#16110](https://github.com/astral-sh/ruff/pull/16110))
- \[`flake8-pyi`\] Avoid flagging `custom-typevar-for-self` on metaclass methods (`PYI019`) ([#16141](https://github.com/astral-sh/ruff/pull/16141))
- \[`pydocstyle`\] Handle arguments with the same names as sections (`D417`) ([#16011](https://github.com/astral-sh/ruff/pull/16011))
- \[`pylint`\] Correct ordering of arguments in fix for `if-stmt-min-max` (`PLR1730`) ([#16080](https://github.com/astral-sh/ruff/pull/16080))
- \[`pylint`\] Do not offer fix for raw strings (`PLE251`) ([#16132](https://github.com/astral-sh/ruff/pull/16132))
- \[`pyupgrade`\] Do not upgrade functional `TypedDicts` with private field names to the class-based syntax (`UP013`) ([#16219](https://github.com/astral-sh/ruff/pull/16219))
- \[`pyupgrade`\] Handle micro version numbers correctly (`UP036`) ([#16091](https://github.com/astral-sh/ruff/pull/16091))
- \[`pyupgrade`\] Unwrap unary expressions correctly (`UP018`) ([#15919](https://github.com/astral-sh/ruff/pull/15919))
- \[`refurb`\] Correctly handle lengths of literal strings in `slice-to-remove-prefix-or-suffix` (`FURB188`) ([#16237](https://github.com/astral-sh/ruff/pull/16237))
- \[`ruff`\] Skip `RUF001` diagnostics when visiting string type definitions ([#16122](https://github.com/astral-sh/ruff/pull/16122))

### Documentation

- Add FAQ entry for `source.*` code actions in Notebook ([#16212](https://github.com/astral-sh/ruff/pull/16212))
- Add `SECURITY.md` ([#16224](https://github.com/astral-sh/ruff/pull/16224))

## 0.9.6

### Preview features

- \[`airflow`\] Add `external_task.{ExternalTaskMarker, ExternalTaskSensor}` for `AIR302` ([#16014](https://github.com/astral-sh/ruff/pull/16014))
- \[`flake8-builtins`\] Make strict module name comparison optional (`A005`) ([#15951](https://github.com/astral-sh/ruff/pull/15951))
- \[`flake8-pyi`\] Extend fix to Python \<= 3.9 for `redundant-none-literal` (`PYI061`) ([#16044](https://github.com/astral-sh/ruff/pull/16044))
- \[`pylint`\] Also report when the object isn't a literal (`PLE1310`) ([#15985](https://github.com/astral-sh/ruff/pull/15985))
- \[`ruff`\] Implement `indented-form-feed` (`RUF054`) ([#16049](https://github.com/astral-sh/ruff/pull/16049))
- \[`ruff`\] Skip type definitions for `missing-f-string-syntax` (`RUF027`) ([#16054](https://github.com/astral-sh/ruff/pull/16054))

### Rule changes

- \[`flake8-annotations`\] Correct syntax for `typing.Union` in suggested return type fixes for `ANN20x` rules ([#16025](https://github.com/astral-sh/ruff/pull/16025))
- \[`flake8-builtins`\] Match upstream module name comparison (`A005`) ([#16006](https://github.com/astral-sh/ruff/pull/16006))
- \[`flake8-comprehensions`\] Detect overshadowed `list`/`set`/`dict`, ignore variadics and named expressions (`C417`) ([#15955](https://github.com/astral-sh/ruff/pull/15955))
- \[`flake8-pie`\] Remove following comma correctly when the unpacked dictionary is empty (`PIE800`) ([#16008](https://github.com/astral-sh/ruff/pull/16008))
- \[`flake8-simplify`\] Only trigger `SIM401` on known dictionaries ([#15995](https://github.com/astral-sh/ruff/pull/15995))
- \[`pylint`\] Do not report calls when object type and argument type mismatch, remove custom escape handling logic (`PLE1310`) ([#15984](https://github.com/astral-sh/ruff/pull/15984))
- \[`pyupgrade`\] Comments within parenthesized value ranges should not affect applicability (`UP040`) ([#16027](https://github.com/astral-sh/ruff/pull/16027))
- \[`pyupgrade`\] Don't introduce invalid syntax when upgrading old-style type aliases with parenthesized multiline values (`UP040`) ([#16026](https://github.com/astral-sh/ruff/pull/16026))
- \[`pyupgrade`\] Ensure we do not rename two type parameters to the same name (`UP049`) ([#16038](https://github.com/astral-sh/ruff/pull/16038))
- \[`pyupgrade`\] \[`ruff`\] Don't apply renamings if the new name is shadowed in a scope of one of the references to the binding (`UP049`, `RUF052`) ([#16032](https://github.com/astral-sh/ruff/pull/16032))
- \[`ruff`\] Update `RUF009` to behave similar to `B008` and ignore attributes with immutable types ([#16048](https://github.com/astral-sh/ruff/pull/16048))

### Server

- Root exclusions in the server to project root ([#16043](https://github.com/astral-sh/ruff/pull/16043))

### Bug fixes

- \[`flake8-datetime`\] Ignore `.replace()` calls while looking for `.astimezone` ([#16050](https://github.com/astral-sh/ruff/pull/16050))
- \[`flake8-type-checking`\] Avoid `TC004` false positive where the runtime definition is provided by `__getattr__` ([#16052](https://github.com/astral-sh/ruff/pull/16052))

### Documentation

- Improve `ruff-lsp` migration document ([#16072](https://github.com/astral-sh/ruff/pull/16072))
- Undeprecate `ruff.nativeServer` ([#16039](https://github.com/astral-sh/ruff/pull/16039))

## 0.9.5

### Preview features

- Recognize all symbols named `TYPE_CHECKING` for `in_type_checking_block` ([#15719](https://github.com/astral-sh/ruff/pull/15719))
- \[`flake8-comprehensions`\] Handle builtins at top of file correctly for `unnecessary-dict-comprehension-for-iterable` (`C420`) ([#15837](https://github.com/astral-sh/ruff/pull/15837))
- \[`flake8-logging`\] `.exception()` and `exc_info=` outside exception handlers (`LOG004`, `LOG014`) ([#15799](https://github.com/astral-sh/ruff/pull/15799))
- \[`flake8-pyi`\] Fix incorrect behaviour of `custom-typevar-return-type` preview-mode autofix if `typing` was already imported (`PYI019`) ([#15853](https://github.com/astral-sh/ruff/pull/15853))
- \[`flake8-pyi`\] Fix more complex cases (`PYI019`) ([#15821](https://github.com/astral-sh/ruff/pull/15821))
- \[`flake8-pyi`\] Make `PYI019` autofixable for `.py` files in preview mode as well as stubs ([#15889](https://github.com/astral-sh/ruff/pull/15889))
- \[`flake8-pyi`\] Remove type parameter correctly when it is the last (`PYI019`) ([#15854](https://github.com/astral-sh/ruff/pull/15854))
- \[`pylint`\] Fix missing parens in unsafe fix for `unnecessary-dunder-call` (`PLC2801`) ([#15762](https://github.com/astral-sh/ruff/pull/15762))
- \[`pyupgrade`\] Better messages and diagnostic range (`UP015`) ([#15872](https://github.com/astral-sh/ruff/pull/15872))
- \[`pyupgrade`\] Rename private type parameters in PEP 695 generics (`UP049`) ([#15862](https://github.com/astral-sh/ruff/pull/15862))
- \[`refurb`\] Also report non-name expressions (`FURB169`) ([#15905](https://github.com/astral-sh/ruff/pull/15905))
- \[`refurb`\] Mark fix as unsafe if there are comments (`FURB171`) ([#15832](https://github.com/astral-sh/ruff/pull/15832))
- \[`ruff`\] Classes with mixed type variable style (`RUF053`) ([#15841](https://github.com/astral-sh/ruff/pull/15841))
- \[`airflow`\] `BashOperator` has been moved to `airflow.providers.standard.operators.bash.BashOperator` (`AIR302`) ([#15922](https://github.com/astral-sh/ruff/pull/15922))
- \[`flake8-pyi`\] Add autofix for unused-private-type-var (`PYI018`) ([#15999](https://github.com/astral-sh/ruff/pull/15999))
- \[`flake8-pyi`\] Significantly improve accuracy of `PYI019` if preview mode is enabled ([#15888](https://github.com/astral-sh/ruff/pull/15888))

### Rule changes

- Preserve triple quotes and prefixes for strings ([#15818](https://github.com/astral-sh/ruff/pull/15818))
- \[`flake8-comprehensions`\] Skip when `TypeError` present from too many (kw)args for `C410`,`C411`, and `C418` ([#15838](https://github.com/astral-sh/ruff/pull/15838))
- \[`flake8-pyi`\] Rename `PYI019` and improve its diagnostic message ([#15885](https://github.com/astral-sh/ruff/pull/15885))
- \[`pep8-naming`\] Ignore `@override` methods (`N803`) ([#15954](https://github.com/astral-sh/ruff/pull/15954))
- \[`pyupgrade`\] Reuse replacement logic from `UP046` and `UP047` to preserve more comments (`UP040`) ([#15840](https://github.com/astral-sh/ruff/pull/15840))
- \[`ruff`\] Analyze deferred annotations before enforcing `mutable-(data)class-default` and `function-call-in-dataclass-default-argument` (`RUF008`,`RUF009`,`RUF012`) ([#15921](https://github.com/astral-sh/ruff/pull/15921))
- \[`pycodestyle`\] Exempt `sys.path += ...` calls (`E402`) ([#15980](https://github.com/astral-sh/ruff/pull/15980))

### Configuration

- Config error only when `flake8-import-conventions` alias conflicts with `isort.required-imports` bound name ([#15918](https://github.com/astral-sh/ruff/pull/15918))
- Workaround Even Better TOML crash related to `allOf` ([#15992](https://github.com/astral-sh/ruff/pull/15992))

### Bug fixes

- \[`flake8-comprehensions`\] Unnecessary `list` comprehension (rewrite as a `set` comprehension) (`C403`) - Handle extraneous parentheses around list comprehension ([#15877](https://github.com/astral-sh/ruff/pull/15877))
- \[`flake8-comprehensions`\] Handle trailing comma in fixes for `unnecessary-generator-list/set` (`C400`,`C401`) ([#15929](https://github.com/astral-sh/ruff/pull/15929))
- \[`flake8-pyi`\] Fix several correctness issues with `custom-type-var-return-type` (`PYI019`) ([#15851](https://github.com/astral-sh/ruff/pull/15851))
- \[`pep8-naming`\] Consider any number of leading underscore for `N801` ([#15988](https://github.com/astral-sh/ruff/pull/15988))
- \[`pyflakes`\] Visit forward annotations in `TypeAliasType` as types (`F401`) ([#15829](https://github.com/astral-sh/ruff/pull/15829))
- \[`pylint`\] Correct min/max auto-fix and suggestion for (`PL1730`) ([#15930](https://github.com/astral-sh/ruff/pull/15930))
- \[`refurb`\] Handle unparenthesized tuples correctly (`FURB122`, `FURB142`) ([#15953](https://github.com/astral-sh/ruff/pull/15953))
- \[`refurb`\] Avoid `None | None` as well as better detection and fix (`FURB168`) ([#15779](https://github.com/astral-sh/ruff/pull/15779))

### Documentation

- Add deprecation warning for `ruff-lsp` related settings ([#15850](https://github.com/astral-sh/ruff/pull/15850))
- Docs (`linter.md`): clarify that Python files are always searched for in subdirectories ([#15882](https://github.com/astral-sh/ruff/pull/15882))
- Fix a typo in `non_pep695_generic_class.rs` ([#15946](https://github.com/astral-sh/ruff/pull/15946))
- Improve Docs: Pylint subcategories' codes ([#15909](https://github.com/astral-sh/ruff/pull/15909))
- Remove non-existing `lint.extendIgnore` editor setting ([#15844](https://github.com/astral-sh/ruff/pull/15844))
- Update black deviations ([#15928](https://github.com/astral-sh/ruff/pull/15928))
- Mention `UP049` in `UP046` and `UP047`, add `See also` section to `UP040` ([#15956](https://github.com/astral-sh/ruff/pull/15956))
- Add instance variable examples to `RUF012` ([#15982](https://github.com/astral-sh/ruff/pull/15982))
- Explain precedence for `ignore` and `select` config ([#15883](https://github.com/astral-sh/ruff/pull/15883))

## 0.9.4

### Preview features

- \[`airflow`\] Extend airflow context parameter check for `BaseOperator.execute` (`AIR302`) ([#15713](https://github.com/astral-sh/ruff/pull/15713))
- \[`airflow`\] Update `AIR302` to check for deprecated context keys ([#15144](https://github.com/astral-sh/ruff/pull/15144))
- \[`flake8-bandit`\] Permit suspicious imports within stub files (`S4`) ([#15822](https://github.com/astral-sh/ruff/pull/15822))
- \[`pylint`\] Do not trigger `PLR6201` on empty collections ([#15732](https://github.com/astral-sh/ruff/pull/15732))
- \[`refurb`\] Do not emit diagnostic when loop variables are used outside loop body (`FURB122`) ([#15757](https://github.com/astral-sh/ruff/pull/15757))
- \[`ruff`\] Add support for more `re` patterns (`RUF055`) ([#15764](https://github.com/astral-sh/ruff/pull/15764))
- \[`ruff`\] Check for shadowed `map` before suggesting fix (`RUF058`) ([#15790](https://github.com/astral-sh/ruff/pull/15790))
- \[`ruff`\] Do not emit diagnostic when all arguments to `zip()` are variadic (`RUF058`) ([#15744](https://github.com/astral-sh/ruff/pull/15744))
- \[`ruff`\] Parenthesize fix when argument spans multiple lines for `unnecessary-round` (`RUF057`) ([#15703](https://github.com/astral-sh/ruff/pull/15703))

### Rule changes

- Preserve quote style in generated code ([#15726](https://github.com/astral-sh/ruff/pull/15726), [#15778](https://github.com/astral-sh/ruff/pull/15778), [#15794](https://github.com/astral-sh/ruff/pull/15794))
- \[`flake8-bugbear`\] Exempt `NewType` calls where the original type is immutable (`B008`) ([#15765](https://github.com/astral-sh/ruff/pull/15765))
- \[`pylint`\] Honor banned top-level imports by `TID253` in `PLC0415`. ([#15628](https://github.com/astral-sh/ruff/pull/15628))
- \[`pyupgrade`\] Ignore `is_typeddict` and `TypedDict` for `deprecated-import` (`UP035`) ([#15800](https://github.com/astral-sh/ruff/pull/15800))

### CLI

- Fix formatter warning message for `flake8-quotes` option ([#15788](https://github.com/astral-sh/ruff/pull/15788))
- Implement tab autocomplete for `ruff config` ([#15603](https://github.com/astral-sh/ruff/pull/15603))

### Bug fixes

- \[`flake8-comprehensions`\] Do not emit `unnecessary-map` diagnostic when lambda has different arity (`C417`) ([#15802](https://github.com/astral-sh/ruff/pull/15802))
- \[`flake8-comprehensions`\] Parenthesize `sorted` when needed for `unnecessary-call-around-sorted` (`C413`) ([#15825](https://github.com/astral-sh/ruff/pull/15825))
- \[`pyupgrade`\] Handle end-of-line comments for `quoted-annotation` (`UP037`) ([#15824](https://github.com/astral-sh/ruff/pull/15824))

### Documentation

- Add missing config docstrings ([#15803](https://github.com/astral-sh/ruff/pull/15803))
- Add references to `trio.run_process` and `anyio.run_process` ([#15761](https://github.com/astral-sh/ruff/pull/15761))
- Use `uv init --lib` in tutorial ([#15718](https://github.com/astral-sh/ruff/pull/15718))

## 0.9.3

### Preview features

- \[`airflow`\] Argument `fail_stop` in DAG has been renamed as `fail_fast` (`AIR302`) ([#15633](https://github.com/astral-sh/ruff/pull/15633))
- \[`airflow`\] Extend `AIR303` with more symbols ([#15611](https://github.com/astral-sh/ruff/pull/15611))
- \[`flake8-bandit`\] Report all references to suspicious functions (`S3`) ([#15541](https://github.com/astral-sh/ruff/pull/15541))
- \[`flake8-pytest-style`\] Do not emit diagnostics for empty `for` loops (`PT012`, `PT031`) ([#15542](https://github.com/astral-sh/ruff/pull/15542))
- \[`flake8-simplify`\] Avoid double negations (`SIM103`) ([#15562](https://github.com/astral-sh/ruff/pull/15562))
- \[`pyflakes`\] Fix infinite loop with unused local import in `__init__.py` (`F401`) ([#15517](https://github.com/astral-sh/ruff/pull/15517))
- \[`pylint`\] Do not report methods with only one `EM101`-compatible `raise` (`PLR6301`) ([#15507](https://github.com/astral-sh/ruff/pull/15507))
- \[`pylint`\] Implement `redefined-slots-in-subclass` (`W0244`) ([#9640](https://github.com/astral-sh/ruff/pull/9640))
- \[`pyupgrade`\] Add rules to use PEP 695 generics in classes and functions (`UP046`, `UP047`) ([#15565](https://github.com/astral-sh/ruff/pull/15565), [#15659](https://github.com/astral-sh/ruff/pull/15659))
- \[`refurb`\] Implement `for-loop-writes` (`FURB122`) ([#10630](https://github.com/astral-sh/ruff/pull/10630))
- \[`ruff`\] Implement `needless-else` clause (`RUF047`) ([#15051](https://github.com/astral-sh/ruff/pull/15051))
- \[`ruff`\] Implement `starmap-zip` (`RUF058`) ([#15483](https://github.com/astral-sh/ruff/pull/15483))

### Rule changes

- \[`flake8-bugbear`\] Do not raise error if keyword argument is present and target-python version is less or equals than 3.9 (`B903`) ([#15549](https://github.com/astral-sh/ruff/pull/15549))
- \[`flake8-comprehensions`\] strip parentheses around generators in `unnecessary-generator-set` (`C401`) ([#15553](https://github.com/astral-sh/ruff/pull/15553))
- \[`flake8-pytest-style`\] Rewrite references to `.exception` (`PT027`) ([#15680](https://github.com/astral-sh/ruff/pull/15680))
- \[`flake8-simplify`\] Mark fixes as unsafe (`SIM201`, `SIM202`) ([#15626](https://github.com/astral-sh/ruff/pull/15626))
- \[`flake8-type-checking`\] Fix some safe fixes being labeled unsafe (`TC006`,`TC008`) ([#15638](https://github.com/astral-sh/ruff/pull/15638))
- \[`isort`\] Omit trailing whitespace in `unsorted-imports` (`I001`) ([#15518](https://github.com/astral-sh/ruff/pull/15518))
- \[`pydoclint`\] Allow ignoring one line docstrings for `DOC` rules ([#13302](https://github.com/astral-sh/ruff/pull/13302))
- \[`pyflakes`\] Apply redefinition fixes by source code order (`F811`) ([#15575](https://github.com/astral-sh/ruff/pull/15575))
- \[`pyflakes`\] Avoid removing too many imports in `redefined-while-unused` (`F811`) ([#15585](https://github.com/astral-sh/ruff/pull/15585))
- \[`pyflakes`\] Group redefinition fixes by source statement (`F811`) ([#15574](https://github.com/astral-sh/ruff/pull/15574))
- \[`pylint`\] Include name of base class in message for `redefined-slots-in-subclass` (`W0244`) ([#15559](https://github.com/astral-sh/ruff/pull/15559))
- \[`ruff`\] Update fix for `RUF055` to use `var == value` ([#15605](https://github.com/astral-sh/ruff/pull/15605))

### Formatter

- Fix bracket spacing for single-element tuples in f-string expressions ([#15537](https://github.com/astral-sh/ruff/pull/15537))
- Fix unstable f-string formatting for expressions containing a trailing comma ([#15545](https://github.com/astral-sh/ruff/pull/15545))

### Performance

- Avoid quadratic membership check in import fixes ([#15576](https://github.com/astral-sh/ruff/pull/15576))

### Server

- Allow `unsafe-fixes` settings for code actions ([#15666](https://github.com/astral-sh/ruff/pull/15666))

### Bug fixes

- \[`flake8-bandit`\] Add missing single-line/dotall regex flag (`S608`) ([#15654](https://github.com/astral-sh/ruff/pull/15654))
- \[`flake8-import-conventions`\] Fix infinite loop between `ICN001` and `I002` (`ICN001`) ([#15480](https://github.com/astral-sh/ruff/pull/15480))
- \[`flake8-simplify`\] Do not emit diagnostics for expressions inside string type annotations (`SIM222`, `SIM223`) ([#15405](https://github.com/astral-sh/ruff/pull/15405))
- \[`pyflakes`\] Treat arguments passed to the `default=` parameter of `TypeVar` as type expressions (`F821`) ([#15679](https://github.com/astral-sh/ruff/pull/15679))
- \[`pyupgrade`\] Avoid syntax error when the iterable is a non-parenthesized tuple (`UP028`) ([#15543](https://github.com/astral-sh/ruff/pull/15543))
- \[`ruff`\] Exempt `NewType` calls where the original type is immutable (`RUF009`) ([#15588](https://github.com/astral-sh/ruff/pull/15588))
- Preserve raw string prefix and escapes in all codegen fixes ([#15694](https://github.com/astral-sh/ruff/pull/15694))

### Documentation

- Generate documentation redirects for lowercase rule codes ([#15564](https://github.com/astral-sh/ruff/pull/15564))
- `TRY300`: Add some extra notes on not catching exceptions you didn't expect ([#15036](https://github.com/astral-sh/ruff/pull/15036))

## 0.9.2

### Preview features

- \[`airflow`\] Fix typo "security_managr" to "security_manager" (`AIR303`) ([#15463](https://github.com/astral-sh/ruff/pull/15463))
- \[`airflow`\] extend and fix AIR302 rules ([#15525](https://github.com/astral-sh/ruff/pull/15525))
- \[`fastapi`\] Handle parameters with `Depends` correctly (`FAST003`) ([#15364](https://github.com/astral-sh/ruff/pull/15364))
- \[`flake8-pytest-style`\] Implement pytest.warns diagnostics (`PT029`, `PT030`, `PT031`) ([#15444](https://github.com/astral-sh/ruff/pull/15444))
- \[`flake8-pytest-style`\] Test function parameters with default arguments (`PT028`) ([#15449](https://github.com/astral-sh/ruff/pull/15449))
- \[`flake8-type-checking`\] Avoid false positives for `|` in `TC008` ([#15201](https://github.com/astral-sh/ruff/pull/15201))

### Rule changes

- \[`flake8-todos`\] Allow VSCode GitHub PR extension style links in `missing-todo-link` (`TD003`) ([#15519](https://github.com/astral-sh/ruff/pull/15519))
- \[`pyflakes`\] Show syntax error message for `F722` ([#15523](https://github.com/astral-sh/ruff/pull/15523))

### Formatter

- Fix curly bracket spacing around f-string expressions containing curly braces ([#15471](https://github.com/astral-sh/ruff/pull/15471))
- Fix joining of f-strings with different quotes when using quote style `Preserve` ([#15524](https://github.com/astral-sh/ruff/pull/15524))

### Server

- Avoid indexing the same workspace multiple times ([#15495](https://github.com/astral-sh/ruff/pull/15495))
- Display context for `ruff.configuration` errors ([#15452](https://github.com/astral-sh/ruff/pull/15452))

### Configuration

- Remove `flatten` to improve deserialization error messages ([#15414](https://github.com/astral-sh/ruff/pull/15414))

### Bug fixes

- Parse triple-quoted string annotations as if parenthesized ([#15387](https://github.com/astral-sh/ruff/pull/15387))
- \[`fastapi`\] Update `Annotated` fixes (`FAST002`) ([#15462](https://github.com/astral-sh/ruff/pull/15462))
- \[`flake8-bandit`\] Check for `builtins` instead of `builtin` (`S102`, `PTH123`) ([#15443](https://github.com/astral-sh/ruff/pull/15443))
- \[`flake8-pathlib`\] Fix `--select` for `os-path-dirname` (`PTH120`) ([#15446](https://github.com/astral-sh/ruff/pull/15446))
- \[`ruff`\] Fix false positive on global keyword (`RUF052`) ([#15235](https://github.com/astral-sh/ruff/pull/15235))

## 0.9.1

### Preview features

- \[`pycodestyle`\] Run `too-many-newlines-at-end-of-file` on each cell in notebooks (`W391`) ([#15308](https://github.com/astral-sh/ruff/pull/15308))
- \[`ruff`\] Omit diagnostic for shadowed private function parameters in `used-dummy-variable` (`RUF052`) ([#15376](https://github.com/astral-sh/ruff/pull/15376))

### Rule changes

- \[`flake8-bugbear`\] Improve `assert-raises-exception` message (`B017`) ([#15389](https://github.com/astral-sh/ruff/pull/15389))

### Formatter

- Preserve trailing end-of line comments for the last string literal in implicitly concatenated strings ([#15378](https://github.com/astral-sh/ruff/pull/15378))

### Server

- Fix a bug where the server and client notebooks were out of sync after reordering cells ([#15398](https://github.com/astral-sh/ruff/pull/15398))

### Bug fixes

- \[`flake8-pie`\] Correctly remove wrapping parentheses (`PIE800`) ([#15394](https://github.com/astral-sh/ruff/pull/15394))
- \[`pyupgrade`\] Handle comments and multiline expressions correctly (`UP037`) ([#15337](https://github.com/astral-sh/ruff/pull/15337))

## 0.9.0

Check out the [blog post](https://astral.sh/blog/ruff-v0.9.0) for a migration guide and overview of the changes!

### Breaking changes

Ruff now formats your code according to the 2025 style guide. As a result, your code might now get formatted differently. See the formatter section for a detailed list of changes.

This release doesn’t remove or remap any existing stable rules.

### Stabilization

The following rules have been stabilized and are no longer in preview:

- [`stdlib-module-shadowing`](https://docs.astral.sh/ruff/rules/stdlib-module-shadowing/) (`A005`).
    This rule has also been renamed: previously, it was called `builtin-module-shadowing`.
- [`builtin-lambda-argument-shadowing`](https://docs.astral.sh/ruff/rules/builtin-lambda-argument-shadowing/) (`A006`)
- [`slice-to-remove-prefix-or-suffix`](https://docs.astral.sh/ruff/rules/slice-to-remove-prefix-or-suffix/) (`FURB188`)
- [`boolean-chained-comparison`](https://docs.astral.sh/ruff/rules/boolean-chained-comparison/) (`PLR1716`)
- [`decimal-from-float-literal`](https://docs.astral.sh/ruff/rules/decimal-from-float-literal/) (`RUF032`)
- [`post-init-default`](https://docs.astral.sh/ruff/rules/post-init-default/) (`RUF033`)
- [`useless-if-else`](https://docs.astral.sh/ruff/rules/useless-if-else/) (`RUF034`)

The following behaviors have been stabilized:

- [`pytest-parametrize-names-wrong-type`](https://docs.astral.sh/ruff/rules/pytest-parametrize-names-wrong-type/) (`PT006`): Detect [`pytest.parametrize`](https://docs.pytest.org/en/7.1.x/how-to/parametrize.html#parametrize) calls outside decorators and calls with keyword arguments.
- [`module-import-not-at-top-of-file`](https://docs.astral.sh/ruff/rules/module-import-not-at-top-of-file/) (`E402`): Ignore [`pytest.importorskip`](https://docs.pytest.org/en/7.1.x/reference/reference.html#pytest-importorskip) calls between import statements.
- [`mutable-dataclass-default`](https://docs.astral.sh/ruff/rules/mutable-dataclass-default/) (`RUF008`) and [`function-call-in-dataclass-default-argument`](https://docs.astral.sh/ruff/rules/function-call-in-dataclass-default-argument/) (`RUF009`): Add support for [`attrs`](https://www.attrs.org/en/stable/).
- [`bad-version-info-comparison`](https://docs.astral.sh/ruff/rules/bad-version-info-comparison/) (`PYI006`): Extend the rule to check non-stub files.

The following fixes or improvements to fixes have been stabilized:

- [`redundant-numeric-union`](https://docs.astral.sh/ruff/rules/redundant-numeric-union/) (`PYI041`)
- [`duplicate-union-members`](https://docs.astral.sh/ruff/rules/duplicate-union-member/) (`PYI016`)

### Formatter

This release introduces the new 2025 stable style ([#13371](https://github.com/astral-sh/ruff/issues/13371)), stabilizing the following changes:

- Format expressions in f-string elements ([#7594](https://github.com/astral-sh/ruff/issues/7594))
- Alternate quotes for strings inside f-strings ([#13860](https://github.com/astral-sh/ruff/pull/13860))
- Preserve the casing of hex codes in f-string debug expressions ([#14766](https://github.com/astral-sh/ruff/issues/14766))
- Choose the quote style for each string literal in an implicitly concatenated f-string rather than for the entire string ([#13539](https://github.com/astral-sh/ruff/pull/13539))
- Automatically join an implicitly concatenated string into a single string literal if it fits on a single line ([#9457](https://github.com/astral-sh/ruff/issues/9457))
- Remove the [`ISC001`](https://docs.astral.sh/ruff/rules/single-line-implicit-string-concatenation/) incompatibility warning ([#15123](https://github.com/astral-sh/ruff/pull/15123))
- Prefer parenthesizing the `assert` message over breaking the assertion expression ([#9457](https://github.com/astral-sh/ruff/issues/9457))
- Automatically parenthesize over-long `if` guards in `match` `case` clauses ([#13513](https://github.com/astral-sh/ruff/pull/13513))
- More consistent formatting for `match` `case` patterns ([#6933](https://github.com/astral-sh/ruff/issues/6933))
- Avoid unnecessary parentheses around return type annotations ([#13381](https://github.com/astral-sh/ruff/pull/13381))
- Keep the opening parentheses on the same line as the `if` keyword for comprehensions where the condition has a leading comment ([#12282](https://github.com/astral-sh/ruff/pull/12282))
- More consistent formatting for `with` statements with a single context manager for Python 3.8 or older ([#10276](https://github.com/astral-sh/ruff/pull/10276))
- Correctly calculate the line-width for code blocks in docstrings when using `max-doc-code-line-length = "dynamic"` ([#13523](https://github.com/astral-sh/ruff/pull/13523))

### Preview features

- \[`flake8-bugbear`\] Implement `class-as-data-structure` (`B903`) ([#9601](https://github.com/astral-sh/ruff/pull/9601))
- \[`flake8-type-checking`\] Apply `quoted-type-alias` more eagerly in `TYPE_CHECKING` blocks and ignore it in stubs (`TC008`) ([#15180](https://github.com/astral-sh/ruff/pull/15180))
- \[`pylint`\] Ignore `eq-without-hash` in stub files (`PLW1641`) ([#15310](https://github.com/astral-sh/ruff/pull/15310))
- \[`pyupgrade`\] Split `UP007` into two individual rules: `UP007` for `Union` and `UP045` for `Optional` (`UP007`, `UP045`) ([#15313](https://github.com/astral-sh/ruff/pull/15313))
- \[`ruff`\] New rule that detects classes that are both an enum and a `dataclass` (`RUF049`) ([#15299](https://github.com/astral-sh/ruff/pull/15299))
- \[`ruff`\] Recode `RUF025` to `RUF037` (`RUF037`) ([#15258](https://github.com/astral-sh/ruff/pull/15258))

### Rule changes

- \[`flake8-builtins`\] Ignore [`stdlib-module-shadowing`](https://docs.astral.sh/ruff/rules/stdlib-module-shadowing/) in stub files(`A005`) ([#15350](https://github.com/astral-sh/ruff/pull/15350))
- \[`flake8-return`\] Add support for functions returning `typing.Never` (`RET503`) ([#15298](https://github.com/astral-sh/ruff/pull/15298))

### Server

- Improve the observability by removing the need for the ["trace" value](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#traceValue) to turn on or off logging. The server logging is solely controlled using the [`logLevel` server setting](https://docs.astral.sh/ruff/editors/settings/#loglevel)
    which defaults to `info`. This addresses the issue where users were notified about an error and told to consult the log, but it didn’t contain any messages. ([#15232](https://github.com/astral-sh/ruff/pull/15232))
- Ignore diagnostics from other sources for code action requests ([#15373](https://github.com/astral-sh/ruff/pull/15373))

### CLI

- Improve the error message for `--config key=value` when the `key` is for a table and it’s a simple `value`

### Bug fixes

- \[`eradicate`\] Ignore metadata blocks directly followed by normal blocks (`ERA001`) ([#15330](https://github.com/astral-sh/ruff/pull/15330))
- \[`flake8-django`\] Recognize other magic methods (`DJ012`) ([#15365](https://github.com/astral-sh/ruff/pull/15365))
- \[`pycodestyle`\] Avoid false positives related to type aliases (`E252`) ([#15356](https://github.com/astral-sh/ruff/pull/15356))
- \[`pydocstyle`\] Avoid treating newline-separated sections as sub-sections (`D405`) ([#15311](https://github.com/astral-sh/ruff/pull/15311))
- \[`pyflakes`\] Remove call when removing final argument from `format` (`F523`) ([#15309](https://github.com/astral-sh/ruff/pull/15309))
- \[`refurb`\] Mark fix as unsafe when the right-hand side is a string (`FURB171`) ([#15273](https://github.com/astral-sh/ruff/pull/15273))
- \[`ruff`\] Treat `)` as a regex metacharacter (`RUF043`, `RUF055`) ([#15318](https://github.com/astral-sh/ruff/pull/15318))
- \[`ruff`\] Parenthesize the `int`-call argument when removing the `int` call would change semantics (`RUF046`) ([#15277](https://github.com/astral-sh/ruff/pull/15277))

## 0.8.6

### Preview features

- \[`format`\]: Preserve multiline implicit concatenated strings in docstring positions ([#15126](https://github.com/astral-sh/ruff/pull/15126))
- \[`ruff`\] Add rule to detect empty literal in deque call (`RUF025`) ([#15104](https://github.com/astral-sh/ruff/pull/15104))
- \[`ruff`\] Avoid reporting when `ndigits` is possibly negative (`RUF057`) ([#15234](https://github.com/astral-sh/ruff/pull/15234))

### Rule changes

- \[`flake8-todos`\] remove issue code length restriction (`TD003`) ([#15175](https://github.com/astral-sh/ruff/pull/15175))
- \[`pyflakes`\] Ignore errors in `@no_type_check` string annotations (`F722`, `F821`) ([#15215](https://github.com/astral-sh/ruff/pull/15215))

### CLI

- Show errors for attempted fixes only when passed `--verbose` ([#15237](https://github.com/astral-sh/ruff/pull/15237))

### Bug fixes

- \[`ruff`\] Avoid syntax error when removing int over multiple lines (`RUF046`) ([#15230](https://github.com/astral-sh/ruff/pull/15230))
- \[`pyupgrade`\] Revert "Add all PEP-585 names to `UP006` rule" ([#15250](https://github.com/astral-sh/ruff/pull/15250))

## 0.8.5

### Preview features

- \[`airflow`\] Extend names moved from core to provider (`AIR303`) ([#15145](https://github.com/astral-sh/ruff/pull/15145), [#15159](https://github.com/astral-sh/ruff/pull/15159), [#15196](https://github.com/astral-sh/ruff/pull/15196), [#15216](https://github.com/astral-sh/ruff/pull/15216))
- \[`airflow`\] Extend rule to check class attributes, methods, arguments (`AIR302`) ([#15054](https://github.com/astral-sh/ruff/pull/15054), [#15083](https://github.com/astral-sh/ruff/pull/15083))
- \[`fastapi`\] Update `FAST002` to check keyword-only arguments ([#15119](https://github.com/astral-sh/ruff/pull/15119))
- \[`flake8-type-checking`\] Disable `TC006` and `TC007` in stub files ([#15179](https://github.com/astral-sh/ruff/pull/15179))
- \[`pylint`\] Detect nested methods correctly (`PLW1641`) ([#15032](https://github.com/astral-sh/ruff/pull/15032))
- \[`ruff`\] Detect more strict-integer expressions (`RUF046`) ([#14833](https://github.com/astral-sh/ruff/pull/14833))
- \[`ruff`\] Implement `falsy-dict-get-fallback` (`RUF056`) ([#15160](https://github.com/astral-sh/ruff/pull/15160))
- \[`ruff`\] Implement `unnecessary-round` (`RUF057`) ([#14828](https://github.com/astral-sh/ruff/pull/14828))

### Rule changes

- Visit PEP 764 inline `TypedDict` keys as non-type-expressions ([#15073](https://github.com/astral-sh/ruff/pull/15073))
- \[`flake8-comprehensions`\] Skip `C416` if comprehension contains unpacking ([#14909](https://github.com/astral-sh/ruff/pull/14909))
- \[`flake8-pie`\] Allow `cast(SomeType, ...)` (`PIE796`) ([#15141](https://github.com/astral-sh/ruff/pull/15141))
- \[`flake8-simplify`\] More precise inference for dictionaries (`SIM300`) ([#15164](https://github.com/astral-sh/ruff/pull/15164))
- \[`flake8-use-pathlib`\] Catch redundant joins in `PTH201` and avoid syntax errors ([#15177](https://github.com/astral-sh/ruff/pull/15177))
- \[`pycodestyle`\] Preserve original value format (`E731`) ([#15097](https://github.com/astral-sh/ruff/pull/15097))
- \[`pydocstyle`\] Split on first whitespace character (`D403`) ([#15082](https://github.com/astral-sh/ruff/pull/15082))
- \[`pyupgrade`\] Add all PEP-585 names to `UP006` rule ([#5454](https://github.com/astral-sh/ruff/pull/5454))

### Configuration

- \[`flake8-type-checking`\] Improve flexibility of `runtime-evaluated-decorators` ([#15204](https://github.com/astral-sh/ruff/pull/15204))
- \[`pydocstyle`\] Add setting to ignore missing documentation for `*args` and `**kwargs` parameters (`D417`) ([#15210](https://github.com/astral-sh/ruff/pull/15210))
- \[`ruff`\] Add an allowlist for `unsafe-markup-use` (`RUF035`) ([#15076](https://github.com/astral-sh/ruff/pull/15076))

### Bug fixes

- Fix type subscript on older python versions ([#15090](https://github.com/astral-sh/ruff/pull/15090))
- Use `TypeChecker` for detecting `fastapi` routes ([#15093](https://github.com/astral-sh/ruff/pull/15093))
- \[`pycodestyle`\] Avoid false positives and negatives related to type parameter default syntax (`E225`, `E251`) ([#15214](https://github.com/astral-sh/ruff/pull/15214))

### Documentation

- Fix incorrect doc in `shebang-not-executable` (`EXE001`) and add git+windows solution to executable bit ([#15208](https://github.com/astral-sh/ruff/pull/15208))
- Rename rules currently not conforming to naming convention ([#15102](https://github.com/astral-sh/ruff/pull/15102))

## 0.8.4

### Preview features

- \[`airflow`\] Extend `AIR302` with additional functions and classes ([#15015](https://github.com/astral-sh/ruff/pull/15015))
- \[`airflow`\] Implement `moved-to-provider-in-3` for modules that has been moved to Airflow providers (`AIR303`) ([#14764](https://github.com/astral-sh/ruff/pull/14764))
- \[`flake8-use-pathlib`\] Extend check for invalid path suffix to include the case `"."` (`PTH210`) ([#14902](https://github.com/astral-sh/ruff/pull/14902))
- \[`perflint`\] Fix panic in `PERF401` when list variable is after the `for` loop ([#14971](https://github.com/astral-sh/ruff/pull/14971))
- \[`perflint`\] Simplify finding the loop target in `PERF401` ([#15025](https://github.com/astral-sh/ruff/pull/15025))
- \[`pylint`\] Preserve original value format (`PLR6104`) ([#14978](https://github.com/astral-sh/ruff/pull/14978))
- \[`ruff`\] Avoid false positives for `RUF027` for typing context bindings ([#15037](https://github.com/astral-sh/ruff/pull/15037))
- \[`ruff`\] Check for ambiguous pattern passed to `pytest.raises()` (`RUF043`) ([#14966](https://github.com/astral-sh/ruff/pull/14966))

### Rule changes

- \[`flake8-bandit`\] Check `S105` for annotated assignment ([#15059](https://github.com/astral-sh/ruff/pull/15059))
- \[`flake8-pyi`\] More autofixes for `redundant-none-literal` (`PYI061`) ([#14872](https://github.com/astral-sh/ruff/pull/14872))
- \[`pydocstyle`\] Skip leading whitespace for `D403` ([#14963](https://github.com/astral-sh/ruff/pull/14963))
- \[`ruff`\] Skip `SQLModel` base classes for `mutable-class-default` (`RUF012`) ([#14949](https://github.com/astral-sh/ruff/pull/14949))

### Bug

- \[`perflint`\] Parenthesize walrus expressions in autofix for `manual-list-comprehension` (`PERF401`) ([#15050](https://github.com/astral-sh/ruff/pull/15050))

### Server

- Check diagnostic refresh support from client capability which enables dynamic configuration for various editors ([#15014](https://github.com/astral-sh/ruff/pull/15014))

## 0.8.3

### Preview features

- Fix fstring formatting removing overlong implicit concatenated string in expression part ([#14811](https://github.com/astral-sh/ruff/pull/14811))
- \[`airflow`\] Add fix to remove deprecated keyword arguments (`AIR302`) ([#14887](https://github.com/astral-sh/ruff/pull/14887))
- \[`airflow`\]: Extend rule to include deprecated names for Airflow 3.0 (`AIR302`) ([#14765](https://github.com/astral-sh/ruff/pull/14765) and [#14804](https://github.com/astral-sh/ruff/pull/14804))
- \[`flake8-bugbear`\] Improve error messages for `except*` (`B025`, `B029`, `B030`, `B904`) ([#14815](https://github.com/astral-sh/ruff/pull/14815))
- \[`flake8-bugbear`\] `itertools.batched()` without explicit `strict` (`B911`) ([#14408](https://github.com/astral-sh/ruff/pull/14408))
- \[`flake8-use-pathlib`\] Dotless suffix passed to `Path.with_suffix()` (`PTH210`) ([#14779](https://github.com/astral-sh/ruff/pull/14779))
- \[`pylint`\] Include parentheses and multiple comparators in check for `boolean-chained-comparison` (`PLR1716`) ([#14781](https://github.com/astral-sh/ruff/pull/14781))
- \[`ruff`\] Do not simplify `round()` calls (`RUF046`) ([#14832](https://github.com/astral-sh/ruff/pull/14832))
- \[`ruff`\] Don't emit `used-dummy-variable` on function parameters (`RUF052`) ([#14818](https://github.com/astral-sh/ruff/pull/14818))
- \[`ruff`\] Implement `if-key-in-dict-del` (`RUF051`) ([#14553](https://github.com/astral-sh/ruff/pull/14553))
- \[`ruff`\] Mark autofix for `RUF052` as always unsafe ([#14824](https://github.com/astral-sh/ruff/pull/14824))
- \[`ruff`\] Teach autofix for `used-dummy-variable` about TypeVars etc. (`RUF052`) ([#14819](https://github.com/astral-sh/ruff/pull/14819))

### Rule changes

- \[`flake8-bugbear`\] Offer unsafe autofix for `no-explicit-stacklevel` (`B028`) ([#14829](https://github.com/astral-sh/ruff/pull/14829))
- \[`flake8-pyi`\] Skip all type definitions in `string-or-bytes-too-long` (`PYI053`) ([#14797](https://github.com/astral-sh/ruff/pull/14797))
- \[`pyupgrade`\] Do not report when a UTF-8 comment is followed by a non-UTF-8 one (`UP009`) ([#14728](https://github.com/astral-sh/ruff/pull/14728))
- \[`pyupgrade`\] Mark fixes for `convert-typed-dict-functional-to-class` and `convert-named-tuple-functional-to-class` as unsafe if they will remove comments (`UP013`, `UP014`) ([#14842](https://github.com/astral-sh/ruff/pull/14842))

### Bug fixes

- Raise syntax error for mixing `except` and `except*` ([#14895](https://github.com/astral-sh/ruff/pull/14895))
- \[`flake8-bugbear`\] Fix `B028` to allow `stacklevel` to be explicitly assigned as a positional argument ([#14868](https://github.com/astral-sh/ruff/pull/14868))
- \[`flake8-bugbear`\] Skip `B028` if `warnings.warn` is called with `*args` or `**kwargs` ([#14870](https://github.com/astral-sh/ruff/pull/14870))
- \[`flake8-comprehensions`\] Skip iterables with named expressions in `unnecessary-map` (`C417`) ([#14827](https://github.com/astral-sh/ruff/pull/14827))
- \[`flake8-pyi`\] Also remove `self` and `cls`'s annotation (`PYI034`) ([#14801](https://github.com/astral-sh/ruff/pull/14801))
- \[`flake8-pytest-style`\] Fix `pytest-parametrize-names-wrong-type` (`PT006`) to edit both `argnames` and `argvalues` if both of them are single-element tuples/lists ([#14699](https://github.com/astral-sh/ruff/pull/14699))
- \[`perflint`\] Improve autofix for `PERF401` ([#14369](https://github.com/astral-sh/ruff/pull/14369))
- \[`pylint`\] Fix `PLW1508` false positive for default string created via a mult operation ([#14841](https://github.com/astral-sh/ruff/pull/14841))

## 0.8.2

### Preview features

- \[`airflow`\] Avoid deprecated values (`AIR302`) ([#14582](https://github.com/astral-sh/ruff/pull/14582))
- \[`airflow`\] Extend removed names for `AIR302` ([#14734](https://github.com/astral-sh/ruff/pull/14734))
- \[`ruff`\] Extend `unnecessary-regular-expression` to non-literal strings (`RUF055`) ([#14679](https://github.com/astral-sh/ruff/pull/14679))
- \[`ruff`\] Implement `used-dummy-variable` (`RUF052`) ([#14611](https://github.com/astral-sh/ruff/pull/14611))
- \[`ruff`\] Implement `unnecessary-cast-to-int` (`RUF046`) ([#14697](https://github.com/astral-sh/ruff/pull/14697))

### Rule changes

- \[`airflow`\] Check `AIR001` from builtin or providers `operators` module ([#14631](https://github.com/astral-sh/ruff/pull/14631))
- \[`flake8-pytest-style`\] Remove `@` in `pytest.mark.parametrize` rule messages ([#14770](https://github.com/astral-sh/ruff/pull/14770))
- \[`pandas-vet`\] Skip rules if the `panda` module hasn't been seen ([#14671](https://github.com/astral-sh/ruff/pull/14671))
- \[`pylint`\] Fix false negatives for `ascii` and `sorted` in `len-as-condition` (`PLC1802`) ([#14692](https://github.com/astral-sh/ruff/pull/14692))
- \[`refurb`\] Guard `hashlib` imports and mark `hashlib-digest-hex` fix as safe (`FURB181`) ([#14694](https://github.com/astral-sh/ruff/pull/14694))

### Configuration

- \[`flake8-import-conventions`\] Improve syntax check for aliases supplied in configuration for `unconventional-import-alias` (`ICN001`) ([#14745](https://github.com/astral-sh/ruff/pull/14745))

### Bug fixes

- Revert: [pyflakes] Avoid false positives in `@no_type_check` contexts (`F821`, `F722`) (#14615) ([#14726](https://github.com/astral-sh/ruff/pull/14726))
- \[`pep8-naming`\] Avoid false positive for `class Bar(type(foo))` (`N804`) ([#14683](https://github.com/astral-sh/ruff/pull/14683))
- \[`pycodestyle`\] Handle f-strings properly for `invalid-escape-sequence` (`W605`) ([#14748](https://github.com/astral-sh/ruff/pull/14748))
- \[`pylint`\] Ignore `@overload` in `PLR0904` ([#14730](https://github.com/astral-sh/ruff/pull/14730))
- \[`refurb`\] Handle non-finite decimals in `verbose-decimal-constructor` (`FURB157`) ([#14596](https://github.com/astral-sh/ruff/pull/14596))
- \[`ruff`\] Avoid emitting `assignment-in-assert` when all references to the assigned variable are themselves inside `assert`s (`RUF018`) ([#14661](https://github.com/astral-sh/ruff/pull/14661))

### Documentation

- Improve docs for `flake8-use-pathlib` rules ([#14741](https://github.com/astral-sh/ruff/pull/14741))
- Improve error messages and docs for `flake8-comprehensions` rules ([#14729](https://github.com/astral-sh/ruff/pull/14729))
- \[`flake8-type-checking`\] Expands `TC006` docs to better explain itself ([#14749](https://github.com/astral-sh/ruff/pull/14749))

## 0.8.1

### Preview features

- Formatter: Avoid invalid syntax for format-spec with quotes for all Python versions ([#14625](https://github.com/astral-sh/ruff/pull/14625))
- Formatter: Consider quotes inside format-specs when choosing the quotes for an f-string ([#14493](https://github.com/astral-sh/ruff/pull/14493))
- Formatter: Do not consider f-strings with escaped newlines as multiline ([#14624](https://github.com/astral-sh/ruff/pull/14624))
- Formatter: Fix f-string formatting in assignment statement ([#14454](https://github.com/astral-sh/ruff/pull/14454))
- Formatter: Fix unnecessary space around power operator (`**`) in overlong f-string expressions ([#14489](https://github.com/astral-sh/ruff/pull/14489))
- \[`airflow`\] Avoid implicit `schedule` argument to `DAG` and `@dag` (`AIR301`) ([#14581](https://github.com/astral-sh/ruff/pull/14581))
- \[`flake8-builtins`\] Exempt private built-in modules (`A005`) ([#14505](https://github.com/astral-sh/ruff/pull/14505))
- \[`flake8-pytest-style`\] Fix `pytest.mark.parametrize` rules to check calls instead of decorators ([#14515](https://github.com/astral-sh/ruff/pull/14515))
- \[`flake8-type-checking`\] Implement `runtime-cast-value` (`TC006`) ([#14511](https://github.com/astral-sh/ruff/pull/14511))
- \[`flake8-type-checking`\] Implement `unquoted-type-alias` (`TC007`) and `quoted-type-alias` (`TC008`) ([#12927](https://github.com/astral-sh/ruff/pull/12927))
- \[`flake8-use-pathlib`\] Recommend `Path.iterdir()` over `os.listdir()` (`PTH208`) ([#14509](https://github.com/astral-sh/ruff/pull/14509))
- \[`pylint`\] Extend `invalid-envvar-default` to detect `os.environ.get` (`PLW1508`) ([#14512](https://github.com/astral-sh/ruff/pull/14512))
- \[`pylint`\] Implement `len-test` (`PLC1802`) ([#14309](https://github.com/astral-sh/ruff/pull/14309))
- \[`refurb`\] Fix bug where methods defined using lambdas were flagged by `FURB118` ([#14639](https://github.com/astral-sh/ruff/pull/14639))
- \[`ruff`\] Auto-add `r` prefix when string has no backslashes for `unraw-re-pattern` (`RUF039`) ([#14536](https://github.com/astral-sh/ruff/pull/14536))
- \[`ruff`\] Implement `invalid-assert-message-literal-argument` (`RUF040`) ([#14488](https://github.com/astral-sh/ruff/pull/14488))
- \[`ruff`\] Implement `unnecessary-nested-literal` (`RUF041`) ([#14323](https://github.com/astral-sh/ruff/pull/14323))
- \[`ruff`\] Implement `unnecessary-regular-expression` (`RUF055`) ([#14659](https://github.com/astral-sh/ruff/pull/14659))

### Rule changes

- Ignore more rules for stub files ([#14541](https://github.com/astral-sh/ruff/pull/14541))
- \[`pep8-naming`\] Eliminate false positives for single-letter names (`N811`, `N814`) ([#14584](https://github.com/astral-sh/ruff/pull/14584))
- \[`pyflakes`\] Avoid false positives in `@no_type_check` contexts (`F821`, `F722`) ([#14615](https://github.com/astral-sh/ruff/pull/14615))
- \[`ruff`\] Detect redirected-noqa in file-level comments (`RUF101`) ([#14635](https://github.com/astral-sh/ruff/pull/14635))
- \[`ruff`\] Mark fixes for `unsorted-dunder-all` and `unsorted-dunder-slots` as unsafe when there are complex comments in the sequence (`RUF022`, `RUF023`) ([#14560](https://github.com/astral-sh/ruff/pull/14560))

### Bug fixes

- Avoid fixing code to `None | None` for `redundant-none-literal` (`PYI061`) and `never-union` (`RUF020`) ([#14583](https://github.com/astral-sh/ruff/pull/14583), [#14589](https://github.com/astral-sh/ruff/pull/14589))
- \[`flake8-bugbear`\] Fix `mutable-contextvar-default` to resolve annotated function calls properly (`B039`) ([#14532](https://github.com/astral-sh/ruff/pull/14532))
- \[`flake8-pyi`, `ruff`\] Fix traversal of nested literals and unions (`PYI016`, `PYI051`, `PYI055`, `PYI062`, `RUF041`) ([#14641](https://github.com/astral-sh/ruff/pull/14641))
- \[`flake8-pyi`\] Avoid rewriting invalid type expressions in `unnecessary-type-union` (`PYI055`) ([#14660](https://github.com/astral-sh/ruff/pull/14660))
- \[`flake8-type-checking`\] Avoid syntax errors and type checking problem for quoted annotations autofix (`TC003`, `TC006`) ([#14634](https://github.com/astral-sh/ruff/pull/14634))
- \[`pylint`\] Do not wrap function calls in parentheses in the fix for unnecessary-dunder-call (`PLC2801`) ([#14601](https://github.com/astral-sh/ruff/pull/14601))
- \[`ruff`\] Handle `attrs`'s `auto_attribs` correctly (`RUF009`) ([#14520](https://github.com/astral-sh/ruff/pull/14520))

## 0.8.0

Check out the [blog post](https://astral.sh/blog/ruff-v0.8.0) for a migration guide and overview of the changes!

### Breaking changes

See also, the "Remapped rules" section which may result in disabled rules.

- **Default to Python 3.9**

    Ruff now defaults to Python 3.9 instead of 3.8 if no explicit Python version is configured using [`ruff.target-version`](https://docs.astral.sh/ruff/settings/#target-version) or [`project.requires-python`](https://packaging.python.org/en/latest/guides/writing-pyproject-toml/#python-requires) ([#13896](https://github.com/astral-sh/ruff/pull/13896))

- **Changed location of `pydoclint` diagnostics**

    [`pydoclint`](https://docs.astral.sh/ruff/rules/#pydoclint-doc) diagnostics now point to the first-line of the problematic docstring. Previously, this was not the case.

    If you've opted into these preview rules but have them suppressed using
    [`noqa`](https://docs.astral.sh/ruff/linter/#error-suppression) comments in
    some places, this change may mean that you need to move the `noqa` suppression
    comments. Most users should be unaffected by this change.

- **Use XDG (i.e. `~/.local/bin`) instead of the Cargo home directory in the standalone installer**

    Previously, Ruff's installer used `$CARGO_HOME` or `~/.cargo/bin` for its target install directory. Now, Ruff will be installed into `$XDG_BIN_HOME`, `$XDG_DATA_HOME/../bin`, or `~/.local/bin` (in that order).

    This change is only relevant to users of the standalone Ruff installer (using the shell or PowerShell script). If you installed Ruff using uv or pip, you should be unaffected.

- **Changes to the line width calculation**

    Ruff now uses a new version of the [unicode-width](https://github.com/unicode-rs/unicode-width) Rust crate to calculate the line width. In very rare cases, this may lead to lines containing Unicode characters being reformatted, or being considered too long when they were not before ([`E501`](https://docs.astral.sh/ruff/rules/line-too-long/)).

### Removed Rules

The following deprecated rules have been removed:

- [`missing-type-self`](https://docs.astral.sh/ruff/rules/missing-type-self/) (`ANN101`)
- [`missing-type-cls`](https://docs.astral.sh/ruff/rules/missing-type-cls/) (`ANN102`)
- [`syntax-error`](https://docs.astral.sh/ruff/rules/syntax-error/) (`E999`)
- [`pytest-missing-fixture-name-underscore`](https://docs.astral.sh/ruff/rules/pytest-missing-fixture-name-underscore/) (`PT004`)
- [`pytest-incorrect-fixture-name-underscore`](https://docs.astral.sh/ruff/rules/pytest-incorrect-fixture-name-underscore/) (`PT005`)
- [`unpacked-list-comprehension`](https://docs.astral.sh/ruff/rules/unpacked-list-comprehension/) (`UP027`)

### Remapped rules

The following rules have been remapped to new rule codes:

- [`flake8-type-checking`](https://docs.astral.sh/ruff/rules/#flake8-type-checking-tc): `TCH` to `TC`

### Stabilization

The following rules have been stabilized and are no longer in preview:

- [`builtin-import-shadowing`](https://docs.astral.sh/ruff/rules/builtin-import-shadowing/) (`A004`)
- [`mutable-contextvar-default`](https://docs.astral.sh/ruff/rules/mutable-contextvar-default/) (`B039`)
- [`fast-api-redundant-response-model`](https://docs.astral.sh/ruff/rules/fast-api-redundant-response-model/) (`FAST001`)
- [`fast-api-non-annotated-dependency`](https://docs.astral.sh/ruff/rules/fast-api-non-annotated-dependency/) (`FAST002`)
- [`dict-index-missing-items`](https://docs.astral.sh/ruff/rules/dict-index-missing-items/) (`PLC0206`)
- [`pep484-style-positional-only-parameter`](https://docs.astral.sh/ruff/rules/pep484-style-positional-only-parameter/) (`PYI063`)
- [`redundant-final-literal`](https://docs.astral.sh/ruff/rules/redundant-final-literal/) (`PYI064`)
- [`bad-version-info-order`](https://docs.astral.sh/ruff/rules/bad-version-info-order/) (`PYI066`)
- [`parenthesize-chained-operators`](https://docs.astral.sh/ruff/rules/parenthesize-chained-operators/) (`RUF021`)
- [`unsorted-dunder-all`](https://docs.astral.sh/ruff/rules/unsorted-dunder-all/) (`RUF022`)
- [`unsorted-dunder-slots`](https://docs.astral.sh/ruff/rules/unsorted-dunder-slots/) (`RUF023`)
- [`assert-with-print-message`](https://docs.astral.sh/ruff/rules/assert-with-print-message/) (`RUF030`)
- [`unnecessary-default-type-args`](https://docs.astral.sh/ruff/rules/unnecessary-default-type-args/) (`UP043`)

The following behaviors have been stabilized:

- [`ambiguous-variable-name`](https://docs.astral.sh/ruff/rules/ambiguous-variable-name/) (`E741`): Violations in stub files are now ignored. Stub authors typically don't control variable names.
- [`printf-string-formatting`](https://docs.astral.sh/ruff/rules/printf-string-formatting/) (`UP031`): Report all `printf`-like usages even if no autofix is available

The following fixes have been stabilized:

- [`zip-instead-of-pairwise`](https://docs.astral.sh/ruff/rules/zip-instead-of-pairwise/) (`RUF007`)

### Preview features

- \[`flake8-datetimez`\] Exempt `min.time()` and `max.time()` (`DTZ901`) ([#14394](https://github.com/astral-sh/ruff/pull/14394))
- \[`flake8-pie`\] Mark fix as unsafe if the following statement is a string literal (`PIE790`) ([#14393](https://github.com/astral-sh/ruff/pull/14393))
- \[`flake8-pyi`\] New rule `redundant-none-literal` (`PYI061`) ([#14316](https://github.com/astral-sh/ruff/pull/14316))
- \[`flake8-pyi`\] Add autofix for `redundant-numeric-union` (`PYI041`) ([#14273](https://github.com/astral-sh/ruff/pull/14273))
- \[`ruff`\] New rule `map-int-version-parsing` (`RUF048`) ([#14373](https://github.com/astral-sh/ruff/pull/14373))
- \[`ruff`\] New rule `redundant-bool-literal` (`RUF038`) ([#14319](https://github.com/astral-sh/ruff/pull/14319))
- \[`ruff`\] New rule `unraw-re-pattern` (`RUF039`) ([#14446](https://github.com/astral-sh/ruff/pull/14446))
- \[`pycodestyle`\] Exempt `pytest.importorskip()` calls (`E402`) ([#14474](https://github.com/astral-sh/ruff/pull/14474))
- \[`pylint`\] Autofix suggests using sets when possible (`PLR1714`) ([#14372](https://github.com/astral-sh/ruff/pull/14372))

### Rule changes

- [`invalid-pyproject-toml`](https://docs.astral.sh/ruff/rules/invalid-pyproject-toml/) (`RUF200`): Updated to reflect the provisionally accepted [PEP 639](https://peps.python.org/pep-0639/).
- \[`flake8-pyi`\] Avoid panic in unfixable case (`PYI041`) ([#14402](https://github.com/astral-sh/ruff/pull/14402))
- \[`flake8-type-checking`\] Correctly handle quotes in subscript expression when generating an autofix ([#14371](https://github.com/astral-sh/ruff/pull/14371))
- \[`pylint`\] Suggest correct autofix for `__contains__` (`PLC2801`) ([#14424](https://github.com/astral-sh/ruff/pull/14424))

### Configuration

- Ruff now emits a warning instead of an error when a configuration [`ignore`](https://docs.astral.sh/ruff/settings/#lint_ignore)s a rule that has been removed ([#14435](https://github.com/astral-sh/ruff/pull/14435))
- Ruff now validates that `lint.flake8-import-conventions.aliases` only uses valid module names and aliases ([#14477](https://github.com/astral-sh/ruff/pull/14477))

## 0.7.4

### Preview features

- \[`flake8-datetimez`\] Detect usages of `datetime.max`/`datetime.min` (`DTZ901`) ([#14288](https://github.com/astral-sh/ruff/pull/14288))
- \[`flake8-logging`\] Implement `root-logger-calls` (`LOG015`) ([#14302](https://github.com/astral-sh/ruff/pull/14302))
- \[`flake8-no-pep420`\] Detect empty implicit namespace packages (`INP001`) ([#14236](https://github.com/astral-sh/ruff/pull/14236))
- \[`flake8-pyi`\] Add "replace with `Self`" fix (`PYI019`) ([#14238](https://github.com/astral-sh/ruff/pull/14238))
- \[`perflint`\] Implement quick-fix for `manual-list-comprehension` (`PERF401`) ([#13919](https://github.com/astral-sh/ruff/pull/13919))
- \[`pylint`\] Implement `shallow-copy-environ` (`W1507`) ([#14241](https://github.com/astral-sh/ruff/pull/14241))
- \[`ruff`\] Implement `none-not-at-end-of-union` (`RUF036`) ([#14314](https://github.com/astral-sh/ruff/pull/14314))
- \[`ruff`\] Implementation `unsafe-markup-call` from `flake8-markupsafe` plugin (`RUF035`) ([#14224](https://github.com/astral-sh/ruff/pull/14224))
- \[`ruff`\] Report problems for `attrs` dataclasses (`RUF008`, `RUF009`) ([#14327](https://github.com/astral-sh/ruff/pull/14327))

### Rule changes

- \[`flake8-boolean-trap`\] Exclude dunder methods that define operators (`FBT001`) ([#14203](https://github.com/astral-sh/ruff/pull/14203))
- \[`flake8-pyi`\] Add "replace with `Self`" fix (`PYI034`) ([#14217](https://github.com/astral-sh/ruff/pull/14217))
- \[`flake8-pyi`\] Always autofix `duplicate-union-members` (`PYI016`) ([#14270](https://github.com/astral-sh/ruff/pull/14270))
- \[`flake8-pyi`\] Improve autofix for nested and mixed type unions for `unnecessary-type-union` (`PYI055`) ([#14272](https://github.com/astral-sh/ruff/pull/14272))
- \[`flake8-pyi`\] Mark fix as unsafe when type annotation contains comments for `duplicate-literal-member` (`PYI062`) ([#14268](https://github.com/astral-sh/ruff/pull/14268))

### Server

- Use the current working directory to resolve settings from `ruff.configuration` ([#14352](https://github.com/astral-sh/ruff/pull/14352))

### Bug fixes

- Avoid conflicts between `PLC014` (`useless-import-alias`) and `I002` (`missing-required-import`) by considering `lint.isort.required-imports` for `PLC014` ([#14287](https://github.com/astral-sh/ruff/pull/14287))
- \[`flake8-type-checking`\] Skip quoting annotation if it becomes invalid syntax (`TCH001`)
- \[`flake8-pyi`\] Avoid using `typing.Self` in stub files pre-Python 3.11 (`PYI034`) ([#14230](https://github.com/astral-sh/ruff/pull/14230))
- \[`flake8-pytest-style`\] Flag `pytest.raises` call with keyword argument `expected_exception` (`PT011`) ([#14298](https://github.com/astral-sh/ruff/pull/14298))
- \[`flake8-simplify`\] Infer "unknown" truthiness for literal iterables whose items are all unpacks (`SIM222`) ([#14263](https://github.com/astral-sh/ruff/pull/14263))
- \[`flake8-type-checking`\] Fix false positives for `typing.Annotated` (`TCH001`) ([#14311](https://github.com/astral-sh/ruff/pull/14311))
- \[`pylint`\] Allow `await` at the top-level scope of a notebook (`PLE1142`) ([#14225](https://github.com/astral-sh/ruff/pull/14225))
- \[`pylint`\] Fix miscellaneous issues in `await-outside-async` detection (`PLE1142`) ([#14218](https://github.com/astral-sh/ruff/pull/14218))
- \[`pyupgrade`\] Avoid applying PEP 646 rewrites in invalid contexts (`UP044`) ([#14234](https://github.com/astral-sh/ruff/pull/14234))
- \[`pyupgrade`\] Detect permutations in redundant open modes (`UP015`) ([#14255](https://github.com/astral-sh/ruff/pull/14255))
- \[`refurb`\] Avoid triggering `hardcoded-string-charset` for reordered sets (`FURB156`) ([#14233](https://github.com/astral-sh/ruff/pull/14233))
- \[`refurb`\] Further special cases added to `verbose-decimal-constructor` (`FURB157`) ([#14216](https://github.com/astral-sh/ruff/pull/14216))
- \[`refurb`\] Use `UserString` instead of non-existent `UserStr` (`FURB189`) ([#14209](https://github.com/astral-sh/ruff/pull/14209))
- \[`ruff`\] Avoid treating lowercase letters as `# noqa` codes (`RUF100`) ([#14229](https://github.com/astral-sh/ruff/pull/14229))
- \[`ruff`\] Do not report when `Optional` has no type arguments (`RUF013`) ([#14181](https://github.com/astral-sh/ruff/pull/14181))

### Documentation

- Add "Notebook behavior" section for `F704`, `PLE1142` ([#14266](https://github.com/astral-sh/ruff/pull/14266))
- Document comment policy around fix safety ([#14300](https://github.com/astral-sh/ruff/pull/14300))

## 0.7.3

### Preview features

- Formatter: Disallow single-line implicit concatenated strings ([#13928](https://github.com/astral-sh/ruff/pull/13928))
- \[`flake8-pyi`\] Include all Python file types for `PYI006` and `PYI066` ([#14059](https://github.com/astral-sh/ruff/pull/14059))
- \[`flake8-simplify`\] Implement `split-of-static-string` (`SIM905`) ([#14008](https://github.com/astral-sh/ruff/pull/14008))
- \[`refurb`\] Implement `subclass-builtin` (`FURB189`) ([#14105](https://github.com/astral-sh/ruff/pull/14105))
- \[`ruff`\] Improve diagnostic messages and docs (`RUF031`, `RUF032`, `RUF034`) ([#14068](https://github.com/astral-sh/ruff/pull/14068))

### Rule changes

- Detect items that hash to same value in duplicate sets (`B033`, `PLC0208`) ([#14064](https://github.com/astral-sh/ruff/pull/14064))
- \[`eradicate`\] Better detection of IntelliJ language injection comments (`ERA001`) ([#14094](https://github.com/astral-sh/ruff/pull/14094))
- \[`flake8-pyi`\] Add autofix for `docstring-in-stub` (`PYI021`) ([#14150](https://github.com/astral-sh/ruff/pull/14150))
- \[`flake8-pyi`\] Update `duplicate-literal-member` (`PYI062`) to alawys provide an autofix ([#14188](https://github.com/astral-sh/ruff/pull/14188))
- \[`pyflakes`\] Detect items that hash to same value in duplicate dictionaries (`F601`) ([#14065](https://github.com/astral-sh/ruff/pull/14065))
- \[`ruff`\] Fix false positive for decorators (`RUF028`) ([#14061](https://github.com/astral-sh/ruff/pull/14061))

### Bug fixes

- Avoid parsing joint rule codes as distinct codes in `# noqa` ([#12809](https://github.com/astral-sh/ruff/pull/12809))
- \[`eradicate`\] ignore `# language=` in commented-out-code rule (ERA001) ([#14069](https://github.com/astral-sh/ruff/pull/14069))
- \[`flake8-bugbear`\] - do not run `mutable-argument-default` on stubs (`B006`) ([#14058](https://github.com/astral-sh/ruff/pull/14058))
- \[`flake8-builtins`\] Skip lambda expressions in `builtin-argument-shadowing (A002)` ([#14144](https://github.com/astral-sh/ruff/pull/14144))
- \[`flake8-comprehension`\] Also remove trailing comma while fixing `C409` and `C419` ([#14097](https://github.com/astral-sh/ruff/pull/14097))
- \[`flake8-simplify`\] Allow `open` without context manager in `return` statement (`SIM115`) ([#14066](https://github.com/astral-sh/ruff/pull/14066))
- \[`pylint`\] Respect hash-equivalent literals in `iteration-over-set` (`PLC0208`) ([#14063](https://github.com/astral-sh/ruff/pull/14063))
- \[`pylint`\] Update known dunder methods for Python 3.13 (`PLW3201`) ([#14146](https://github.com/astral-sh/ruff/pull/14146))
- \[`pyupgrade`\] - ignore kwarg unpacking for `UP044` ([#14053](https://github.com/astral-sh/ruff/pull/14053))
- \[`refurb`\] Parse more exotic decimal strings in `verbose-decimal-constructor` (`FURB157`) ([#14098](https://github.com/astral-sh/ruff/pull/14098))

### Documentation

- Add links to missing related options within rule documentations ([#13971](https://github.com/astral-sh/ruff/pull/13971))
- Add rule short code to mkdocs tags to allow searching via rule codes ([#14040](https://github.com/astral-sh/ruff/pull/14040))

## 0.7.2

### Preview features

- Fix formatting of single with-item with trailing comment ([#14005](https://github.com/astral-sh/ruff/pull/14005))
- \[`pyupgrade`\] Add PEP 646 `Unpack` conversion to `*` with fix (`UP044`) ([#13988](https://github.com/astral-sh/ruff/pull/13988))

### Rule changes

- Regenerate `known_stdlibs.rs` with stdlibs 2024.10.25 ([#13963](https://github.com/astral-sh/ruff/pull/13963))
- \[`flake8-no-pep420`\] Skip namespace package enforcement for PEP 723 scripts (`INP001`) ([#13974](https://github.com/astral-sh/ruff/pull/13974))

### Server

- Fix server panic when undoing an edit ([#14010](https://github.com/astral-sh/ruff/pull/14010))

### Bug fixes

- Fix issues in discovering ruff in pip build environments ([#13881](https://github.com/astral-sh/ruff/pull/13881))
- \[`flake8-type-checking`\] Fix false positive for `singledispatchmethod` (`TCH003`) ([#13941](https://github.com/astral-sh/ruff/pull/13941))
- \[`flake8-type-checking`\] Treat return type of `singledispatch` as runtime-required (`TCH003`) ([#13957](https://github.com/astral-sh/ruff/pull/13957))

### Documentation

- \[`flake8-simplify`\] Include caveats of enabling `if-else-block-instead-of-if-exp` (`SIM108`) ([#14019](https://github.com/astral-sh/ruff/pull/14019))

## 0.7.1

### Preview features

- Fix `E221` and `E222` to flag missing or extra whitespace around `==` operator ([#13890](https://github.com/astral-sh/ruff/pull/13890))
- Formatter: Alternate quotes for strings inside f-strings in preview ([#13860](https://github.com/astral-sh/ruff/pull/13860))
- Formatter: Join implicit concatenated strings when they fit on a line ([#13663](https://github.com/astral-sh/ruff/pull/13663))
- \[`pylint`\] Restrict `iteration-over-set` to only work on sets of literals (`PLC0208`) ([#13731](https://github.com/astral-sh/ruff/pull/13731))

### Rule changes

- \[`flake8-type-checking`\] Support auto-quoting when annotations contain quotes ([#11811](https://github.com/astral-sh/ruff/pull/11811))

### Server

- Avoid indexing the workspace for single-file mode ([#13770](https://github.com/astral-sh/ruff/pull/13770))

### Bug fixes

- Make `ARG002` compatible with `EM101` when raising `NotImplementedError` ([#13714](https://github.com/astral-sh/ruff/pull/13714))

### Other changes

- Introduce more Docker tags for Ruff (similar to uv) ([#13274](https://github.com/astral-sh/ruff/pull/13274))

## 0.7.0

Check out the [blog post](https://astral.sh/blog/ruff-v0.7.0) for a migration guide and overview of the changes!

### Breaking changes

- The pytest rules `PT001` and `PT023` now default to omitting the decorator parentheses when there are no arguments
    ([#12838](https://github.com/astral-sh/ruff/pull/12838), [#13292](https://github.com/astral-sh/ruff/pull/13292)).
    This was a change that we attempted to make in Ruff v0.6.0, but only partially made due to an error on our part.
    See the [blog post](https://astral.sh/blog/ruff-v0.7.0) for more details.
- The `useless-try-except` rule (in our `tryceratops` category) has been recoded from `TRY302` to
    `TRY203` ([#13502](https://github.com/astral-sh/ruff/pull/13502)). This ensures Ruff's code is consistent with
    the same rule in the [`tryceratops`](https://github.com/guilatrova/tryceratops) linter.
- The `lint.allow-unused-imports` setting has been removed ([#13677](https://github.com/astral-sh/ruff/pull/13677)). Use
    [`lint.pyflakes.allow-unused-imports`](https://docs.astral.sh/ruff/settings/#lint_pyflakes_allowed-unused-imports)
    instead.

### Formatter preview style

- Normalize implicit concatenated f-string quotes per part ([#13539](https://github.com/astral-sh/ruff/pull/13539))

### Preview linter features

- \[`refurb`\] implement `hardcoded-string-charset` (FURB156) ([#13530](https://github.com/astral-sh/ruff/pull/13530))
- \[`refurb`\] Count codepoints not bytes for `slice-to-remove-prefix-or-suffix (FURB188)` ([#13631](https://github.com/astral-sh/ruff/pull/13631))

### Rule changes

- \[`pylint`\] Mark `PLE1141` fix as unsafe ([#13629](https://github.com/astral-sh/ruff/pull/13629))
- \[`flake8-async`\] Consider async generators to be "checkpoints" for `cancel-scope-no-checkpoint` (`ASYNC100`) ([#13639](https://github.com/astral-sh/ruff/pull/13639))
- \[`flake8-bugbear`\] Do not suggest setting parameter `strict=` to `False` in `B905` diagnostic message ([#13656](https://github.com/astral-sh/ruff/pull/13656))
- \[`flake8-todos`\] Only flag the word "TODO", not words starting with "todo" (`TD006`) ([#13640](https://github.com/astral-sh/ruff/pull/13640))
- \[`pycodestyle`\] Fix whitespace-related false positives and false negatives inside type-parameter lists (`E231`, `E251`) ([#13704](https://github.com/astral-sh/ruff/pull/13704))
- \[`flake8-simplify`\] Stabilize preview behavior for `SIM115` so that the rule can detect files
    being opened from a wider range of standard-library functions ([#12959](https://github.com/astral-sh/ruff/pull/12959)).

### CLI

- Add explanation of fixable in `--statistics` command ([#13774](https://github.com/astral-sh/ruff/pull/13774))

### Bug fixes

- \[`pyflakes`\] Allow `ipytest` cell magic (`F401`) ([#13745](https://github.com/astral-sh/ruff/pull/13745))
- \[`flake8-use-pathlib`\] Fix `PTH123` false positive when `open` is passed a file descriptor ([#13616](https://github.com/astral-sh/ruff/pull/13616))
- \[`flake8-bandit`\] Detect patterns from multi line SQL statements (`S608`) ([#13574](https://github.com/astral-sh/ruff/pull/13574))
- \[`flake8-pyi`\] - Fix dropped expressions in `PYI030` autofix ([#13727](https://github.com/astral-sh/ruff/pull/13727))

## 0.6.9

### Preview features

- Fix codeblock dynamic line length calculation for indented docstring examples ([#13523](https://github.com/astral-sh/ruff/pull/13523))
- \[`refurb`\] Mark `FURB118` fix as unsafe ([#13613](https://github.com/astral-sh/ruff/pull/13613))

### Rule changes

- \[`pydocstyle`\] Don't raise `D208` when last line is non-empty ([#13372](https://github.com/astral-sh/ruff/pull/13372))
- \[`pylint`\] Preserve trivia (i.e. comments) in `PLR5501` autofix ([#13573](https://github.com/astral-sh/ruff/pull/13573))

### Configuration

- \[`pyflakes`\] Add `allow-unused-imports` setting for `unused-import` rule (`F401`) ([#13601](https://github.com/astral-sh/ruff/pull/13601))

### Bug fixes

- Support ruff discovery in pip build environments ([#13591](https://github.com/astral-sh/ruff/pull/13591))
- \[`flake8-bugbear`\] Avoid short circuiting `B017` for multiple context managers ([#13609](https://github.com/astral-sh/ruff/pull/13609))
- \[`pylint`\] Do not offer an invalid fix for `PLR1716` when the comparisons contain parenthesis ([#13527](https://github.com/astral-sh/ruff/pull/13527))
- \[`pyupgrade`\] Fix `UP043` to apply to `collections.abc.Generator` and `collections.abc.AsyncGenerator` ([#13611](https://github.com/astral-sh/ruff/pull/13611))
- \[`refurb`\] Fix handling of slices in tuples for `FURB118`, e.g., `x[:, 1]` ([#13518](https://github.com/astral-sh/ruff/pull/13518))

### Documentation

- Update GitHub Action link to `astral-sh/ruff-action` ([#13551](https://github.com/astral-sh/ruff/pull/13551))

## 0.6.8

### Preview features

- Remove unnecessary parentheses around `match case` clauses ([#13510](https://github.com/astral-sh/ruff/pull/13510))
- Parenthesize overlong `if` guards in `match..case` clauses ([#13513](https://github.com/astral-sh/ruff/pull/13513))
- Detect basic wildcard imports in `ruff analyze graph` ([#13486](https://github.com/astral-sh/ruff/pull/13486))
- \[`pylint`\] Implement `boolean-chained-comparison` (`R1716`) ([#13435](https://github.com/astral-sh/ruff/pull/13435))

### Rule changes

- \[`lake8-simplify`\] Detect `SIM910` when using variadic keyword arguments, i.e., `**kwargs` ([#13503](https://github.com/astral-sh/ruff/pull/13503))
- \[`pyupgrade`\] Avoid false negatives with non-reference shadowed bindings of loop variables (`UP028`) ([#13504](https://github.com/astral-sh/ruff/pull/13504))

### Bug fixes

- Detect tuples bound to variadic positional arguments i.e. `*args` ([#13512](https://github.com/astral-sh/ruff/pull/13512))
- Exit gracefully on broken pipe errors ([#13485](https://github.com/astral-sh/ruff/pull/13485))
- Avoid panic when analyze graph hits broken pipe ([#13484](https://github.com/astral-sh/ruff/pull/13484))

### Performance

- Reuse `BTreeSets` in module resolver ([#13440](https://github.com/astral-sh/ruff/pull/13440))
- Skip traversal for non-compound statements ([#13441](https://github.com/astral-sh/ruff/pull/13441))

## 0.6.7

### Preview features

- Add Python version support to ruff analyze CLI ([#13426](https://github.com/astral-sh/ruff/pull/13426))
- Add `exclude` support to `ruff analyze` ([#13425](https://github.com/astral-sh/ruff/pull/13425))
- Fix parentheses around return type annotations ([#13381](https://github.com/astral-sh/ruff/pull/13381))

### Rule changes

- \[`pycodestyle`\] Fix: Don't autofix if the first line ends in a question mark? (D400) ([#13399](https://github.com/astral-sh/ruff/pull/13399))

### Bug fixes

- Respect `lint.exclude` in ruff check `--add-noqa` ([#13427](https://github.com/astral-sh/ruff/pull/13427))

### Performance

- Avoid tracking module resolver files in Salsa ([#13437](https://github.com/astral-sh/ruff/pull/13437))
- Use `forget` for module resolver database ([#13438](https://github.com/astral-sh/ruff/pull/13438))

## 0.6.6

### Preview features

- \[`refurb`\] Skip `slice-to-remove-prefix-or-suffix` (`FURB188`) when non-trivial slice steps are present ([#13405](https://github.com/astral-sh/ruff/pull/13405))
- Add a subcommand to generate dependency graphs ([#13402](https://github.com/astral-sh/ruff/pull/13402))

### Formatter

- Fix placement of inline parameter comments ([#13379](https://github.com/astral-sh/ruff/pull/13379))

### Server

- Fix off-by one error in the `LineIndex::offset` calculation ([#13407](https://github.com/astral-sh/ruff/pull/13407))

### Bug fixes

- \[`fastapi`\] Respect FastAPI aliases in route definitions ([#13394](https://github.com/astral-sh/ruff/pull/13394))
- \[`pydocstyle`\] Respect word boundaries when detecting function signature in docs ([#13388](https://github.com/astral-sh/ruff/pull/13388))

### Documentation

- Add backlinks to rule overview linter ([#13368](https://github.com/astral-sh/ruff/pull/13368))
- Fix documentation for editor vim plugin ALE ([#13348](https://github.com/astral-sh/ruff/pull/13348))
- Fix rendering of `FURB188` docs ([#13406](https://github.com/astral-sh/ruff/pull/13406))

## 0.6.5

### Preview features

- \[`pydoclint`\] Ignore `DOC201` when function name is "**new**" ([#13300](https://github.com/astral-sh/ruff/pull/13300))
- \[`refurb`\] Implement `slice-to-remove-prefix-or-suffix` (`FURB188`) ([#13256](https://github.com/astral-sh/ruff/pull/13256))

### Rule changes

- \[`eradicate`\] Ignore script-comments with multiple end-tags (`ERA001`) ([#13283](https://github.com/astral-sh/ruff/pull/13283))
- \[`pyflakes`\] Improve error message for `UndefinedName` when a builtin was added in a newer version than specified in Ruff config (`F821`) ([#13293](https://github.com/astral-sh/ruff/pull/13293))

### Server

- Add support for extensionless Python files for server ([#13326](https://github.com/astral-sh/ruff/pull/13326))
- Fix configuration inheritance for configurations specified in the LSP settings ([#13285](https://github.com/astral-sh/ruff/pull/13285))

### Bug fixes

- \[`ruff`\] Handle unary operators in `decimal-from-float-literal` (`RUF032`) ([#13275](https://github.com/astral-sh/ruff/pull/13275))

### CLI

- Only include rules with diagnostics in SARIF metadata ([#13268](https://github.com/astral-sh/ruff/pull/13268))

### Playground

- Add "Copy as pyproject.toml/ruff.toml" and "Paste from TOML" ([#13328](https://github.com/astral-sh/ruff/pull/13328))
- Fix errors not shown for restored snippet on page load ([#13262](https://github.com/astral-sh/ruff/pull/13262))

## 0.6.4

### Preview features

- \[`flake8-builtins`\] Use dynamic builtins list based on Python version ([#13172](https://github.com/astral-sh/ruff/pull/13172))
- \[`pydoclint`\] Permit yielding `None` in `DOC402` and `DOC403` ([#13148](https://github.com/astral-sh/ruff/pull/13148))
- \[`pylint`\] Update diagnostic message for `PLW3201` ([#13194](https://github.com/astral-sh/ruff/pull/13194))
- \[`ruff`\] Implement `post-init-default` (`RUF033`) ([#13192](https://github.com/astral-sh/ruff/pull/13192))
- \[`ruff`\] Implement useless if-else (`RUF034`) ([#13218](https://github.com/astral-sh/ruff/pull/13218))

### Rule changes

- \[`flake8-pyi`\] Respect `pep8_naming.classmethod-decorators` settings when determining if a method is a classmethod in `custom-type-var-return-type` (`PYI019`) ([#13162](https://github.com/astral-sh/ruff/pull/13162))
- \[`flake8-pyi`\] Teach various rules that annotations might be stringized ([#12951](https://github.com/astral-sh/ruff/pull/12951))
- \[`pylint`\] Avoid `no-self-use` for `attrs`-style validators ([#13166](https://github.com/astral-sh/ruff/pull/13166))
- \[`pylint`\] Recurse into subscript subexpressions when searching for list/dict lookups (`PLR1733`, `PLR1736`) ([#13186](https://github.com/astral-sh/ruff/pull/13186))
- \[`pyupgrade`\] Detect `aiofiles.open` calls in `UP015` ([#13173](https://github.com/astral-sh/ruff/pull/13173))
- \[`pyupgrade`\] Mark `sys.version_info[0] < 3` and similar comparisons as outdated (`UP036`) ([#13175](https://github.com/astral-sh/ruff/pull/13175))

### CLI

- Enrich messages of SARIF results ([#13180](https://github.com/astral-sh/ruff/pull/13180))
- Handle singular case for incompatible rules warning in `ruff format` output ([#13212](https://github.com/astral-sh/ruff/pull/13212))

### Bug fixes

- \[`pydocstyle`\] Improve heuristics for detecting Google-style docstrings ([#13142](https://github.com/astral-sh/ruff/pull/13142))
- \[`refurb`\] Treat `sep` arguments with effects as unsafe removals (`FURB105`) ([#13165](https://github.com/astral-sh/ruff/pull/13165))

## 0.6.3

### Preview features

- \[`flake8-simplify`\] Extend `open-file-with-context-handler` to work with `dbm.sqlite3` (`SIM115`) ([#13104](https://github.com/astral-sh/ruff/pull/13104))
- \[`pycodestyle`\] Disable `E741` in stub files (`.pyi`) ([#13119](https://github.com/astral-sh/ruff/pull/13119))
- \[`pydoclint`\] Avoid `DOC201` on explicit returns in functions that only return `None` ([#13064](https://github.com/astral-sh/ruff/pull/13064))

### Rule changes

- \[`flake8-async`\] Disable check for `asyncio` before Python 3.11 (`ASYNC109`) ([#13023](https://github.com/astral-sh/ruff/pull/13023))

### Bug fixes

- \[`FastAPI`\] Avoid introducing invalid syntax in fix for `fast-api-non-annotated-dependency` (`FAST002`) ([#13133](https://github.com/astral-sh/ruff/pull/13133))
- \[`flake8-implicit-str-concat`\] Normalize octals before merging concatenated strings in `single-line-implicit-string-concatenation` (`ISC001`) ([#13118](https://github.com/astral-sh/ruff/pull/13118))
- \[`flake8-pytest-style`\] Improve help message for `pytest-incorrect-mark-parentheses-style` (`PT023`) ([#13092](https://github.com/astral-sh/ruff/pull/13092))
- \[`pylint`\] Avoid autofix for calls that aren't `min` or `max` as starred expression (`PLW3301`) ([#13089](https://github.com/astral-sh/ruff/pull/13089))
- \[`ruff`\] Add `datetime.time`, `datetime.tzinfo`, and `datetime.timezone` as immutable function calls (`RUF009`) ([#13109](https://github.com/astral-sh/ruff/pull/13109))
- \[`ruff`\] Extend comment deletion for `RUF100` to include trailing text from `noqa` directives while preserving any following comments on the same line, if any ([#13105](https://github.com/astral-sh/ruff/pull/13105))
- Fix dark theme on initial page load for the Ruff playground ([#13077](https://github.com/astral-sh/ruff/pull/13077))

## 0.6.2

### Preview features

- \[`flake8-simplify`\] Extend `open-file-with-context-handler` to work with other standard-library IO modules (`SIM115`) ([#12959](https://github.com/astral-sh/ruff/pull/12959))
- \[`ruff`\] Avoid `unused-async` for functions with FastAPI route decorator (`RUF029`) ([#12938](https://github.com/astral-sh/ruff/pull/12938))
- \[`ruff`\] Ignore `fstring-missing-syntax` (`RUF027`) for `fastAPI` paths ([#12939](https://github.com/astral-sh/ruff/pull/12939))
- \[`ruff`\] Implement check for Decimal called with a float literal (RUF032) ([#12909](https://github.com/astral-sh/ruff/pull/12909))

### Rule changes

- \[`flake8-bugbear`\] Update diagnostic message when expression is at the end of function (`B015`) ([#12944](https://github.com/astral-sh/ruff/pull/12944))
- \[`flake8-pyi`\] Skip type annotations in `string-or-bytes-too-long` (`PYI053`) ([#13002](https://github.com/astral-sh/ruff/pull/13002))
- \[`flake8-type-checking`\] Always recognise relative imports as first-party ([#12994](https://github.com/astral-sh/ruff/pull/12994))
- \[`flake8-unused-arguments`\] Ignore unused arguments on stub functions (`ARG001`) ([#12966](https://github.com/astral-sh/ruff/pull/12966))
- \[`pylint`\] Ignore augmented assignment for `self-cls-assignment` (`PLW0642`) ([#12957](https://github.com/astral-sh/ruff/pull/12957))

### Server

- Show full context in error log messages ([#13029](https://github.com/astral-sh/ruff/pull/13029))

### Bug fixes

- \[`pep8-naming`\] Don't flag `from` imports following conventional import names (`N817`) ([#12946](https://github.com/astral-sh/ruff/pull/12946))
- \[`pylint`\] - Allow `__new__` methods to have `cls` as their first argument even if decorated with `@staticmethod` for `bad-staticmethod-argument` (`PLW0211`) ([#12958](https://github.com/astral-sh/ruff/pull/12958))

### Documentation

- Add `hyperfine` installation instructions; update `hyperfine` code samples ([#13034](https://github.com/astral-sh/ruff/pull/13034))
- Expand note to use Ruff with other language server in Kate ([#12806](https://github.com/astral-sh/ruff/pull/12806))
- Update example for `PT001` as per the new default behavior ([#13019](https://github.com/astral-sh/ruff/pull/13019))
- \[`perflint`\] Improve docs for `try-except-in-loop` (`PERF203`) ([#12947](https://github.com/astral-sh/ruff/pull/12947))
- \[`pydocstyle`\] Add reference to `lint.pydocstyle.ignore-decorators` setting to rule docs ([#12996](https://github.com/astral-sh/ruff/pull/12996))

## 0.6.1

This is a hotfix release to address an issue with `ruff-pre-commit`. In v0.6,
Ruff changed its behavior to lint and format Jupyter notebooks by default;
however, due to an oversight, these files were still excluded by default if
Ruff was run via pre-commit, leading to inconsistent behavior.
This has [now been fixed](https://github.com/astral-sh/ruff-pre-commit/pull/96).

### Preview features

- \[`fastapi`\] Implement `fast-api-unused-path-parameter` (`FAST003`) ([#12638](https://github.com/astral-sh/ruff/pull/12638))

### Rule changes

- \[`pylint`\] Rename `too-many-positional` to `too-many-positional-arguments` (`R0917`) ([#12905](https://github.com/astral-sh/ruff/pull/12905))

### Server

- Fix crash when applying "fix-all" code-action to notebook cells ([#12929](https://github.com/astral-sh/ruff/pull/12929))

### Other changes

- \[`flake8-naming`\]: Respect import conventions (`N817`) ([#12922](https://github.com/astral-sh/ruff/pull/12922))

## 0.6.0

Check out the [blog post](https://astral.sh/blog/ruff-v0.6.0) for a migration guide and overview of the changes!

### Breaking changes

See also, the "Remapped rules" section which may result in disabled rules.

- Lint and format Jupyter Notebook by default ([#12878](https://github.com/astral-sh/ruff/pull/12878)).
- Detect imports in `src` layouts by default for `isort` rules ([#12848](https://github.com/astral-sh/ruff/pull/12848))
- The pytest rules `PT001` and `PT023` now default to omitting the decorator parentheses when there are no arguments ([#12838](https://github.com/astral-sh/ruff/pull/12838)).

### Deprecations

The following rules are now deprecated:

- [`pytest-missing-fixture-name-underscore`](https://docs.astral.sh/ruff/rules/pytest-missing-fixture-name-underscore/) (`PT004`)
- [`pytest-incorrect-fixture-name-underscore`](https://docs.astral.sh/ruff/rules/pytest-incorrect-fixture-name-underscore/) (`PT005`)
- [`unpacked-list-comprehension`](https://docs.astral.sh/ruff/rules/unpacked-list-comprehension/) (`UP027`)

### Remapped rules

The following rules have been remapped to new rule codes:

- [`unnecessary-dict-comprehension-for-iterable`](https://docs.astral.sh/ruff/rules/unnecessary-dict-comprehension-for-iterable/): `RUF025` to `C420`

### Stabilization

The following rules have been stabilized and are no longer in preview:

- [`singledispatch-method`](https://docs.astral.sh/ruff/rules/singledispatch-method/) (`PLE1519`)
- [`singledispatchmethod-function`](https://docs.astral.sh/ruff/rules/singledispatchmethod-function/) (`PLE1520`)
- [`bad-staticmethod-argument`](https://docs.astral.sh/ruff/rules/bad-staticmethod-argument/) (`PLW0211`)
- [`if-stmt-min-max`](https://docs.astral.sh/ruff/rules/if-stmt-min-max/) (`PLR1730`)
- [`invalid-bytes-return-type`](https://docs.astral.sh/ruff/rules/invalid-bytes-return-type/) (`PLE0308`)
- [`invalid-hash-return-type`](https://docs.astral.sh/ruff/rules/invalid-hash-return-type/) (`PLE0309`)
- [`invalid-index-return-type`](https://docs.astral.sh/ruff/rules/invalid-index-return-type/) (`PLE0305`)
- [`invalid-length-return-type`](https://docs.astral.sh/ruff/rules/invalid-length-return-type/) (`PLEE303`)
- [`self-or-cls-assignment`](https://docs.astral.sh/ruff/rules/self-or-cls-assignment/) (`PLW0642`)
- [`byte-string-usage`](https://docs.astral.sh/ruff/rules/byte-string-usage/) (`PYI057`)
- [`duplicate-literal-member`](https://docs.astral.sh/ruff/rules/duplicate-literal-member/) (`PYI062`)
- [`redirected-noqa`](https://docs.astral.sh/ruff/rules/redirected-noqa/) (`RUF101`)

The following behaviors have been stabilized:

- [`cancel-scope-no-checkpoint`](https://docs.astral.sh/ruff/rules/cancel-scope-no-checkpoint/) (`ASYNC100`): Support `asyncio` and `anyio` context managers.
- [`async-function-with-timeout`](https://docs.astral.sh/ruff/rules/async-function-with-timeout/) (`ASYNC109`): Support `asyncio` and `anyio` context managers.
- [`async-busy-wait`](https://docs.astral.sh/ruff/rules/async-busy-wait/) (`ASYNC110`): Support `asyncio` and `anyio` context managers.
- [`async-zero-sleep`](https://docs.astral.sh/ruff/rules/async-zero-sleep/) (`ASYNC115`): Support `anyio` context managers.
- [`long-sleep-not-forever`](https://docs.astral.sh/ruff/rules/long-sleep-not-forever/) (`ASYNC116`): Support `anyio` context managers.

The following fixes have been stabilized:

- [`superfluous-else-return`](https://docs.astral.sh/ruff/rules/superfluous-else-return/) (`RET505`)
- [`superfluous-else-raise`](https://docs.astral.sh/ruff/rules/superfluous-else-raise/) (`RET506`)
- [`superfluous-else-continue`](https://docs.astral.sh/ruff/rules/superfluous-else-continue/) (`RET507`)
- [`superfluous-else-break`](https://docs.astral.sh/ruff/rules/superfluous-else-break/) (`RET508`)

### Preview features

- \[`flake8-simplify`\] Further simplify to binary in preview for (`SIM108`) ([#12796](https://github.com/astral-sh/ruff/pull/12796))
- \[`pyupgrade`\] Show violations without auto-fix (`UP031`) ([#11229](https://github.com/astral-sh/ruff/pull/11229))

### Rule changes

- \[`flake8-import-conventions`\] Add `xml.etree.ElementTree` to default conventions ([#12455](https://github.com/astral-sh/ruff/pull/12455))
- \[`flake8-pytest-style`\] Add a space after comma in CSV output (`PT006`) ([#12853](https://github.com/astral-sh/ruff/pull/12853))

### Server

- Show a message for incorrect settings ([#12781](https://github.com/astral-sh/ruff/pull/12781))

### Bug fixes

- \[`flake8-async`\] Do not lint yield in context manager (`ASYNC100`) ([#12896](https://github.com/astral-sh/ruff/pull/12896))
- \[`flake8-comprehensions`\] Do not lint `async for` comprehensions (`C419`) ([#12895](https://github.com/astral-sh/ruff/pull/12895))
- \[`flake8-return`\] Only add return `None` at end of a function (`RET503`) ([#11074](https://github.com/astral-sh/ruff/pull/11074))
- \[`flake8-type-checking`\] Avoid treating `dataclasses.KW_ONLY` as typing-only (`TCH003`) ([#12863](https://github.com/astral-sh/ruff/pull/12863))
- \[`pep8-naming`\] Treat `type(Protocol)` et al as metaclass base (`N805`) ([#12770](https://github.com/astral-sh/ruff/pull/12770))
- \[`pydoclint`\] Don't enforce returns and yields in abstract methods (`DOC201`, `DOC202`) ([#12771](https://github.com/astral-sh/ruff/pull/12771))
- \[`ruff`\] Skip tuples with slice expressions in (`RUF031`) ([#12768](https://github.com/astral-sh/ruff/pull/12768))
- \[`ruff`\] Ignore unparenthesized tuples in subscripts when the subscript is a type annotation or type alias (`RUF031`) ([#12762](https://github.com/astral-sh/ruff/pull/12762))
- \[`ruff`\] Ignore template strings passed to logging and `builtins._()` calls (`RUF027`) ([#12889](https://github.com/astral-sh/ruff/pull/12889))
- \[`ruff`\] Do not remove parens for tuples with starred expressions in Python \<=3.10 (`RUF031`) ([#12784](https://github.com/astral-sh/ruff/pull/12784))
- Evaluate default parameter values for a function in that function's enclosing scope ([#12852](https://github.com/astral-sh/ruff/pull/12852))

### Other changes

- Respect VS Code cell metadata when detecting the language of Jupyter Notebook cells ([#12864](https://github.com/astral-sh/ruff/pull/12864))
- Respect `kernelspec` notebook metadata when detecting the preferred language for a Jupyter Notebook ([#12875](https://github.com/astral-sh/ruff/pull/12875))

## 0.5.7

### Preview features

- \[`flake8-comprehensions`\] Account for list and set comprehensions in `unnecessary-literal-within-tuple-call` (`C409`) ([#12657](https://github.com/astral-sh/ruff/pull/12657))
- \[`flake8-pyi`\] Add autofix for `future-annotations-in-stub` (`PYI044`) ([#12676](https://github.com/astral-sh/ruff/pull/12676))
- \[`flake8-return`\] Avoid syntax error when auto-fixing `RET505` with mixed indentation (space and tabs) ([#12740](https://github.com/astral-sh/ruff/pull/12740))
- \[`pydoclint`\] Add `docstring-missing-yields` (`DOC402`) and `docstring-extraneous-yields` (`DOC403`) ([#12538](https://github.com/astral-sh/ruff/pull/12538))
- \[`pydoclint`\] Avoid `DOC201` if docstring begins with "Return", "Returns", "Yield", or "Yields" ([#12675](https://github.com/astral-sh/ruff/pull/12675))
- \[`pydoclint`\] Deduplicate collected exceptions after traversing function bodies (`DOC501`) ([#12642](https://github.com/astral-sh/ruff/pull/12642))
- \[`pydoclint`\] Ignore `DOC` errors for stub functions ([#12651](https://github.com/astral-sh/ruff/pull/12651))
- \[`pydoclint`\] Teach rules to understand reraised exceptions as being explicitly raised (`DOC501`, `DOC502`) ([#12639](https://github.com/astral-sh/ruff/pull/12639))
- \[`ruff`\] Implement `incorrectly-parenthesized-tuple-in-subscript` (`RUF031`) ([#12480](https://github.com/astral-sh/ruff/pull/12480))
- \[`ruff`\] Mark `RUF023` fix as unsafe if `__slots__` is not a set and the binding is used elsewhere ([#12692](https://github.com/astral-sh/ruff/pull/12692))

### Rule changes

- \[`refurb`\] Add autofix for `implicit-cwd` (`FURB177`) ([#12708](https://github.com/astral-sh/ruff/pull/12708))
- \[`ruff`\] Add autofix for `zip-instead-of-pairwise` (`RUF007`) ([#12663](https://github.com/astral-sh/ruff/pull/12663))
- \[`tryceratops`\] Add `BaseException` to `raise-vanilla-class` rule (`TRY002`) ([#12620](https://github.com/astral-sh/ruff/pull/12620))

### Server

- Ignore non-file workspace URL; Ruff will display a warning notification in this case ([#12725](https://github.com/astral-sh/ruff/pull/12725))

### CLI

- Fix cache invalidation for nested `pyproject.toml` files ([#12727](https://github.com/astral-sh/ruff/pull/12727))

### Bug fixes

- \[`flake8-async`\] Fix false positives with multiple `async with` items (`ASYNC100`) ([#12643](https://github.com/astral-sh/ruff/pull/12643))
- \[`flake8-bandit`\] Avoid false-positives for list concatenations in SQL construction (`S608`) ([#12720](https://github.com/astral-sh/ruff/pull/12720))
- \[`flake8-bugbear`\] Treat `return` as equivalent to `break` (`B909`) ([#12646](https://github.com/astral-sh/ruff/pull/12646))
- \[`flake8-comprehensions`\] Set comprehensions not a violation for `sum` in `unnecessary-comprehension-in-call` (`C419`) ([#12691](https://github.com/astral-sh/ruff/pull/12691))
- \[`flake8-simplify`\] Parenthesize conditions based on precedence when merging if arms (`SIM114`) ([#12737](https://github.com/astral-sh/ruff/pull/12737))
- \[`pydoclint`\] Try both 'Raises' section styles when convention is unspecified (`DOC501`) ([#12649](https://github.com/astral-sh/ruff/pull/12649))

## 0.5.6

Ruff 0.5.6 automatically enables linting and formatting of notebooks in *preview mode*.
You can opt-out of this behavior by adding `*.ipynb` to the `extend-exclude` setting.

```toml
[tool.ruff]
extend-exclude = ["*.ipynb"]
```

### Preview features

- Enable notebooks by default in preview mode ([#12621](https://github.com/astral-sh/ruff/pull/12621))
- \[`flake8-builtins`\] Implement import, lambda, and module shadowing ([#12546](https://github.com/astral-sh/ruff/pull/12546))
- \[`pydoclint`\] Add `docstring-missing-returns` (`DOC201`) and `docstring-extraneous-returns` (`DOC202`) ([#12485](https://github.com/astral-sh/ruff/pull/12485))

### Rule changes

- \[`flake8-return`\] Exempt cached properties and other property-like decorators from explicit return rule (`RET501`) ([#12563](https://github.com/astral-sh/ruff/pull/12563))

### Server

- Make server panic hook more error resilient ([#12610](https://github.com/astral-sh/ruff/pull/12610))
- Use `$/logTrace` for server trace logs in Zed and VS Code ([#12564](https://github.com/astral-sh/ruff/pull/12564))
- Keep track of deleted cells for reorder change request ([#12575](https://github.com/astral-sh/ruff/pull/12575))

### Configuration

- \[`flake8-implicit-str-concat`\] Always allow explicit multi-line concatenations when implicit concatenations are banned ([#12532](https://github.com/astral-sh/ruff/pull/12532))

### Bug fixes

- \[`flake8-async`\] Avoid flagging `asyncio.timeout`s as unused when the context manager includes `asyncio.TaskGroup` ([#12605](https://github.com/astral-sh/ruff/pull/12605))
- \[`flake8-slots`\] Avoid recommending `__slots__` for classes that inherit from more than `namedtuple` ([#12531](https://github.com/astral-sh/ruff/pull/12531))
- \[`isort`\] Avoid marking required imports as unused ([#12537](https://github.com/astral-sh/ruff/pull/12537))
- \[`isort`\] Preserve trailing inline comments on import-from statements ([#12498](https://github.com/astral-sh/ruff/pull/12498))
- \[`pycodestyle`\] Add newlines before comments (`E305`) ([#12606](https://github.com/astral-sh/ruff/pull/12606))
- \[`pycodestyle`\] Don't attach comments with mismatched indents ([#12604](https://github.com/astral-sh/ruff/pull/12604))
- \[`pyflakes`\] Fix preview-mode bugs in `F401` when attempting to autofix unused first-party submodule imports in an `__init__.py` file ([#12569](https://github.com/astral-sh/ruff/pull/12569))
- \[`pylint`\] Respect start index in `unnecessary-list-index-lookup` ([#12603](https://github.com/astral-sh/ruff/pull/12603))
- \[`pyupgrade`\] Avoid recommending no-argument super in `slots=True` dataclasses ([#12530](https://github.com/astral-sh/ruff/pull/12530))
- \[`pyupgrade`\] Use colon rather than dot formatting for integer-only types ([#12534](https://github.com/astral-sh/ruff/pull/12534))
- Fix NFKC normalization bug when removing unused imports ([#12571](https://github.com/astral-sh/ruff/pull/12571))

### Other changes

- Consider more stdlib decorators to be property-like ([#12583](https://github.com/astral-sh/ruff/pull/12583))
- Improve handling of metaclasses in various linter rules ([#12579](https://github.com/astral-sh/ruff/pull/12579))
- Improve consistency between linter rules in determining whether a function is property ([#12581](https://github.com/astral-sh/ruff/pull/12581))

## 0.5.5

### Preview features

- \[`fastapi`\] Implement `fastapi-redundant-response-model` (`FAST001`) and `fastapi-non-annotated-dependency`(`FAST002`) ([#11579](https://github.com/astral-sh/ruff/pull/11579))
- \[`pydoclint`\] Implement `docstring-missing-exception` (`DOC501`) and `docstring-extraneous-exception` (`DOC502`) ([#11471](https://github.com/astral-sh/ruff/pull/11471))

### Rule changes

- \[`numpy`\] Fix NumPy 2.0 rule for `np.alltrue` and `np.sometrue` ([#12473](https://github.com/astral-sh/ruff/pull/12473))
- \[`numpy`\] Ignore `NPY201` inside `except` blocks for compatibility with older numpy versions ([#12490](https://github.com/astral-sh/ruff/pull/12490))
- \[`pep8-naming`\] Avoid applying `ignore-names` to `self` and `cls` function names (`N804`, `N805`) ([#12497](https://github.com/astral-sh/ruff/pull/12497))

### Formatter

- Fix incorrect placement of leading function comment with type params ([#12447](https://github.com/astral-sh/ruff/pull/12447))

### Server

- Do not bail code action resolution when a quick fix is requested ([#12462](https://github.com/astral-sh/ruff/pull/12462))

### Bug fixes

- Fix `Ord` implementation of `cmp_fix` ([#12471](https://github.com/astral-sh/ruff/pull/12471))
- Raise syntax error for unparenthesized generator expression in multi-argument call ([#12445](https://github.com/astral-sh/ruff/pull/12445))
- \[`pydoclint`\] Fix panic in `DOC501` reported in [#12428](https://github.com/astral-sh/ruff/pull/12428) ([#12435](https://github.com/astral-sh/ruff/pull/12435))
- \[`flake8-bugbear`\] Allow singleton tuples with starred expressions in `B013` ([#12484](https://github.com/astral-sh/ruff/pull/12484))

### Documentation

- Add Eglot setup guide for Emacs editor ([#12426](https://github.com/astral-sh/ruff/pull/12426))
- Add note about the breaking change in `nvim-lspconfig` ([#12507](https://github.com/astral-sh/ruff/pull/12507))
- Add note to include notebook files for native server ([#12449](https://github.com/astral-sh/ruff/pull/12449))
- Add setup docs for Zed editor ([#12501](https://github.com/astral-sh/ruff/pull/12501))

## 0.5.4

### Rule changes

- \[`ruff`\] Rename `RUF007` to `zip-instead-of-pairwise` ([#12399](https://github.com/astral-sh/ruff/pull/12399))

### Bug fixes

- \[`flake8-builtins`\] Avoid shadowing diagnostics for `@override` methods ([#12415](https://github.com/astral-sh/ruff/pull/12415))
- \[`flake8-comprehensions`\] Insert parentheses for multi-argument generators ([#12422](https://github.com/astral-sh/ruff/pull/12422))
- \[`pydocstyle`\] Handle escaped docstrings within docstring (`D301`) ([#12192](https://github.com/astral-sh/ruff/pull/12192))

### Documentation

- Fix GitHub link to Neovim setup ([#12410](https://github.com/astral-sh/ruff/pull/12410))
- Fix `output-format` default in settings reference ([#12409](https://github.com/astral-sh/ruff/pull/12409))

## 0.5.3

**Ruff 0.5.3 marks the stable release of the Ruff language server and introduces revamped
[documentation](https://docs.astral.sh/ruff/editors), including [setup guides for your editor of
choice](https://docs.astral.sh/ruff/editors/setup) and [the language server
itself](https://docs.astral.sh/ruff/editors/settings)**.

### Preview features

- Formatter: Insert empty line between suite and alternative branch after function/class definition ([#12294](https://github.com/astral-sh/ruff/pull/12294))
- \[`pyupgrade`\] Implement `unnecessary-default-type-args` (`UP043`) ([#12371](https://github.com/astral-sh/ruff/pull/12371))

### Rule changes

- \[`flake8-bugbear`\] Detect enumerate iterations in `loop-iterator-mutation` (`B909`) ([#12366](https://github.com/astral-sh/ruff/pull/12366))
- \[`flake8-bugbear`\] Remove `discard`, `remove`, and `pop` allowance for `loop-iterator-mutation` (`B909`) ([#12365](https://github.com/astral-sh/ruff/pull/12365))
- \[`pylint`\] Allow `repeated-equality-comparison` for mixed operations (`PLR1714`) ([#12369](https://github.com/astral-sh/ruff/pull/12369))
- \[`pylint`\] Ignore `self` and `cls` when counting arguments (`PLR0913`) ([#12367](https://github.com/astral-sh/ruff/pull/12367))
- \[`pylint`\] Use UTF-8 as default encoding in `unspecified-encoding` fix (`PLW1514`) ([#12370](https://github.com/astral-sh/ruff/pull/12370))

### Server

- Build settings index in parallel for the native server ([#12299](https://github.com/astral-sh/ruff/pull/12299))
- Use fallback settings when indexing the project ([#12362](https://github.com/astral-sh/ruff/pull/12362))
- Consider `--preview` flag for `server` subcommand for the linter and formatter ([#12208](https://github.com/astral-sh/ruff/pull/12208))

### Bug fixes

- \[`flake8-comprehensions`\] Allow additional arguments for `sum` and `max` comprehensions (`C419`) ([#12364](https://github.com/astral-sh/ruff/pull/12364))
- \[`pylint`\] Avoid dropping extra boolean operations in `repeated-equality-comparison` (`PLR1714`) ([#12368](https://github.com/astral-sh/ruff/pull/12368))
- \[`pylint`\] Consider expression before statement when determining binding kind (`PLR1704`) ([#12346](https://github.com/astral-sh/ruff/pull/12346))

### Documentation

- Add docs for Ruff language server ([#12344](https://github.com/astral-sh/ruff/pull/12344))
- Migrate to standalone docs repo ([#12341](https://github.com/astral-sh/ruff/pull/12341))
- Update versioning policy for editor integration ([#12375](https://github.com/astral-sh/ruff/pull/12375))

### Other changes

- Publish Wasm API to npm ([#12317](https://github.com/astral-sh/ruff/pull/12317))

## 0.5.2

### Preview features

- Use `space` separator before parenthesized expressions in comprehensions with leading comments ([#12282](https://github.com/astral-sh/ruff/pull/12282))
- \[`flake8-async`\] Update `ASYNC100` to include `anyio` and `asyncio` ([#12221](https://github.com/astral-sh/ruff/pull/12221))
- \[`flake8-async`\] Update `ASYNC109` to include `anyio` and `asyncio` ([#12236](https://github.com/astral-sh/ruff/pull/12236))
- \[`flake8-async`\] Update `ASYNC110` to include `anyio` and `asyncio` ([#12261](https://github.com/astral-sh/ruff/pull/12261))
- \[`flake8-async`\] Update `ASYNC115` to include `anyio` and `asyncio` ([#12262](https://github.com/astral-sh/ruff/pull/12262))
- \[`flake8-async`\] Update `ASYNC116` to include `anyio` and `asyncio` ([#12266](https://github.com/astral-sh/ruff/pull/12266))

### Rule changes

- \[`flake8-return`\] Exempt properties from explicit return rule (`RET501`) ([#12243](https://github.com/astral-sh/ruff/pull/12243))
- \[`numpy`\] Add `np.NAN`-to-`np.nan` diagnostic ([#12292](https://github.com/astral-sh/ruff/pull/12292))
- \[`refurb`\] Make `list-reverse-copy` an unsafe fix ([#12303](https://github.com/astral-sh/ruff/pull/12303))

### Server

- Consider `include` and `extend-include` settings in native server ([#12252](https://github.com/astral-sh/ruff/pull/12252))
- Include nested configurations in settings reloading ([#12253](https://github.com/astral-sh/ruff/pull/12253))

### CLI

- Omit code frames for fixes with empty ranges ([#12304](https://github.com/astral-sh/ruff/pull/12304))
- Warn about formatter incompatibility for `D203` ([#12238](https://github.com/astral-sh/ruff/pull/12238))

### Bug fixes

- Make cache-write failures non-fatal on Windows ([#12302](https://github.com/astral-sh/ruff/pull/12302))
- Treat `not` operations as boolean tests ([#12301](https://github.com/astral-sh/ruff/pull/12301))
- \[`flake8-bandit`\] Avoid `S310` violations for HTTP-safe f-strings ([#12305](https://github.com/astral-sh/ruff/pull/12305))
- \[`flake8-bandit`\] Support explicit string concatenations in S310 HTTP detection ([#12315](https://github.com/astral-sh/ruff/pull/12315))
- \[`flake8-bandit`\] fix S113 false positive for httpx without `timeout` argument ([#12213](https://github.com/astral-sh/ruff/pull/12213))
- \[`pycodestyle`\] Remove "non-obvious" allowance for E721 ([#12300](https://github.com/astral-sh/ruff/pull/12300))
- \[`pyflakes`\] Consider `with` blocks as single-item branches for redefinition analysis ([#12311](https://github.com/astral-sh/ruff/pull/12311))
- \[`refurb`\] Restrict forwarding for `newline` argument in `open()` calls to Python versions >= 3.10 ([#12244](https://github.com/astral-sh/ruff/pull/12244))

### Documentation

- Update help and documentation to reflect `--output-format full` default ([#12248](https://github.com/astral-sh/ruff/pull/12248))

### Performance

- Use more threads when discovering Python files ([#12258](https://github.com/astral-sh/ruff/pull/12258))

## 0.5.1

### Preview features

- \[`flake8-bugbear`\] Implement mutable-contextvar-default (B039) ([#12113](https://github.com/astral-sh/ruff/pull/12113))
- \[`pycodestyle`\] Whitespace after decorator (`E204`) ([#12140](https://github.com/astral-sh/ruff/pull/12140))
- \[`pytest`\] Reverse `PT001` and `PT0023` defaults ([#12106](https://github.com/astral-sh/ruff/pull/12106))

### Rule changes

- Enable token-based rules on source with syntax errors ([#11950](https://github.com/astral-sh/ruff/pull/11950))
- \[`flake8-bandit`\] Detect `httpx` for `S113` ([#12174](https://github.com/astral-sh/ruff/pull/12174))
- \[`numpy`\] Update `NPY201` to include exception deprecations ([#12065](https://github.com/astral-sh/ruff/pull/12065))
- \[`pylint`\] Generate autofix for `duplicate-bases` (`PLE0241`) ([#12105](https://github.com/astral-sh/ruff/pull/12105))

### Server

- Avoid syntax error notification for source code actions ([#12148](https://github.com/astral-sh/ruff/pull/12148))
- Consider the content of the new cells during notebook sync ([#12203](https://github.com/astral-sh/ruff/pull/12203))
- Fix replacement edit range computation ([#12171](https://github.com/astral-sh/ruff/pull/12171))

### Bug fixes

- Disable auto-fix when source has syntax errors ([#12134](https://github.com/astral-sh/ruff/pull/12134))
- Fix cache key collisions for paths with separators ([#12159](https://github.com/astral-sh/ruff/pull/12159))
- Make `requires-python` inference robust to `==` ([#12091](https://github.com/astral-sh/ruff/pull/12091))
- Use char-wise width instead of `str`-width ([#12135](https://github.com/astral-sh/ruff/pull/12135))
- \[`pycodestyle`\] Avoid `E275` if keyword followed by comma ([#12136](https://github.com/astral-sh/ruff/pull/12136))
- \[`pycodestyle`\] Avoid `E275` if keyword is followed by a semicolon ([#12095](https://github.com/astral-sh/ruff/pull/12095))
- \[`pylint`\] Skip [dummy variables](https://docs.astral.sh/ruff/settings/#lint_dummy-variable-rgx) for `PLR1704` ([#12190](https://github.com/astral-sh/ruff/pull/12190))

### Performance

- Remove allocation in `parse_identifier` ([#12103](https://github.com/astral-sh/ruff/pull/12103))
- Use `CompactString` for `Identifier` AST node ([#12101](https://github.com/astral-sh/ruff/pull/12101))

## 0.5.0

Check out the [blog post](https://astral.sh/blog/ruff-v0.5.0) for a migration guide and overview of the changes!

### Breaking changes

See also, the "Remapped rules" section which may result in disabled rules.

- Follow the XDG specification to discover user-level configurations on macOS (same as on other Unix platforms)
- Selecting `ALL` now excludes deprecated rules
- The released archives now include an extra level of nesting, which can be removed with `--strip-components=1` when untarring.
- The release artifact's file name no longer includes the version tag. This enables users to install via `/latest` URLs on GitHub.
- The diagnostic ranges for some `flake8-bandit` rules were modified ([#10667](https://github.com/astral-sh/ruff/pull/10667)).

### Deprecations

The following rules are now deprecated:

- [`syntax-error`](https://docs.astral.sh/ruff/rules/syntax-error/) (`E999`): Syntax errors are now always shown

### Remapped rules

The following rules have been remapped to new rule codes:

- [`blocking-http-call-in-async-function`](https://docs.astral.sh/ruff/rules/blocking-http-call-in-async-function/): `ASYNC100` to `ASYNC210`
- [`open-sleep-or-subprocess-in-async-function`](https://docs.astral.sh/ruff/rules/open-sleep-or-subprocess-in-async-function/): `ASYNC101` split into `ASYNC220`, `ASYNC221`, `ASYNC230`, and `ASYNC251`
- [`blocking-os-call-in-async-function`](https://docs.astral.sh/ruff/rules/blocking-os-call-in-async-function/): `ASYNC102` has been merged into `ASYNC220` and `ASYNC221`
- [`trio-timeout-without-await`](https://docs.astral.sh/ruff/rules/trio-timeout-without-await/): `TRIO100` to `ASYNC100`
- [`trio-sync-call`](https://docs.astral.sh/ruff/rules/trio-sync-call/): `TRIO105` to `ASYNC105`
- [`trio-async-function-with-timeout`](https://docs.astral.sh/ruff/rules/trio-async-function-with-timeout/): `TRIO109` to `ASYNC109`
- [`trio-unneeded-sleep`](https://docs.astral.sh/ruff/rules/trio-unneeded-sleep/): `TRIO110` to `ASYNC110`
- [`trio-zero-sleep-call`](https://docs.astral.sh/ruff/rules/trio-zero-sleep-call/): `TRIO115` to `ASYNC115`
- [`repeated-isinstance-calls`](https://docs.astral.sh/ruff/rules/repeated-isinstance-calls/): `PLR1701` to `SIM101`

### Stabilization

The following rules have been stabilized and are no longer in preview:

- [`mutable-fromkeys-value`](https://docs.astral.sh/ruff/rules/mutable-fromkeys-value/) (`RUF024`)
- [`default-factory-kwarg`](https://docs.astral.sh/ruff/rules/default-factory-kwarg/) (`RUF026`)
- [`django-extra`](https://docs.astral.sh/ruff/rules/django-extra/) (`S610`)
- [`manual-dict-comprehension`](https://docs.astral.sh/ruff/rules/manual-dict-comprehension/) (`PERF403`)
- [`print-empty-string`](https://docs.astral.sh/ruff/rules/print-empty-string/) (`FURB105`)
- [`readlines-in-for`](https://docs.astral.sh/ruff/rules/readlines-in-for/) (`FURB129`)
- [`if-expr-min-max`](https://docs.astral.sh/ruff/rules/if-expr-min-max/) (`FURB136`)
- [`bit-count`](https://docs.astral.sh/ruff/rules/bit-count/) (`FURB161`)
- [`redundant-log-base`](https://docs.astral.sh/ruff/rules/redundant-log-base/) (`FURB163`)
- [`regex-flag-alias`](https://docs.astral.sh/ruff/rules/regex-flag-alias/) (`FURB167`)
- [`isinstance-type-none`](https://docs.astral.sh/ruff/rules/isinstance-type-none/) (`FURB168`)
- [`type-none-comparison`](https://docs.astral.sh/ruff/rules/type-none-comparison/) (`FURB169`)
- [`implicit-cwd`](https://docs.astral.sh/ruff/rules/implicit-cwd/) (`FURB177`)
- [`hashlib-digest-hex`](https://docs.astral.sh/ruff/rules/hashlib-digest-hex/) (`FURB181`)
- [`list-reverse-copy`](https://docs.astral.sh/ruff/rules/list-reverse-copy/) (`FURB187`)
- [`bad-open-mode`](https://docs.astral.sh/ruff/rules/bad-open-mode/) (`PLW1501`)
- [`empty-comment`](https://docs.astral.sh/ruff/rules/empty-comment/) (`PLR2044`)
- [`global-at-module-level`](https://docs.astral.sh/ruff/rules/global-at-module-level/) (`PLW0604`)
- [`misplaced-bare-raise`](https://docs.astral.sh/ruff/rules/misplaced-bare-raise/) (`PLE0744`)
- [`non-ascii-import-name`](https://docs.astral.sh/ruff/rules/non-ascii-import-name/) (`PLC2403`)
- [`non-ascii-name`](https://docs.astral.sh/ruff/rules/non-ascii-name/) (`PLC2401`)
- [`nonlocal-and-global`](https://docs.astral.sh/ruff/rules/nonlocal-and-global/) (`PLE0115`)
- [`potential-index-error`](https://docs.astral.sh/ruff/rules/potential-index-error/) (`PLE0643`)
- [`redeclared-assigned-name`](https://docs.astral.sh/ruff/rules/redeclared-assigned-name/) (`PLW0128`)
- [`redefined-argument-from-local`](https://docs.astral.sh/ruff/rules/redefined-argument-from-local/) (`PLR1704`)
- [`repeated-keyword-argument`](https://docs.astral.sh/ruff/rules/repeated-keyword-argument/) (`PLE1132`)
- [`super-without-brackets`](https://docs.astral.sh/ruff/rules/super-without-brackets/) (`PLW0245`)
- [`unnecessary-list-index-lookup`](https://docs.astral.sh/ruff/rules/unnecessary-list-index-lookup/) (`PLR1736`)
- [`useless-exception-statement`](https://docs.astral.sh/ruff/rules/useless-exception-statement/) (`PLW0133`)
- [`useless-with-lock`](https://docs.astral.sh/ruff/rules/useless-with-lock/) (`PLW2101`)

The following behaviors have been stabilized:

- [`is-literal`](https://docs.astral.sh/ruff/rules/is-literal/) (`F632`) now warns for identity checks against list, set or dictionary literals
- [`needless-bool`](https://docs.astral.sh/ruff/rules/needless-bool/) (`SIM103`) now detects `if` expressions with implicit `else` branches
- [`module-import-not-at-top-of-file`](https://docs.astral.sh/ruff/rules/module-import-not-at-top-of-file/) (`E402`) now allows `os.environ` modifications between import statements
- [`type-comparison`](https://docs.astral.sh/ruff/rules/type-comparison/) (`E721`) now allows idioms such as `type(x) is int`
- [`yoda-condition`](https://docs.astral.sh/ruff/rules/yoda-conditions/) (`SIM300`) now flags a wider range of expressions

### Removals

The following deprecated settings have been removed:

- `output-format=text`; use `output-format=concise` or `output-format=full`
- `tab-size`; use `indent-width`

The following deprecated CLI options have been removed:

- `--show-source`; use `--output-format=full`
- `--no-show-source`; use `--output-format=concise`

The following deprecated CLI commands have been removed:

- `ruff <path>`; use `ruff check <path>`
- `ruff --clean`; use `ruff clean`
- `ruff --generate-shell-completion`; use `ruff generate-shell-completion`

### Preview features

- \[`ruff`\] Add `assert-with-print-message` rule ([#11981](https://github.com/astral-sh/ruff/pull/11981))

### CLI

- Use rule name rather than message in `--statistics` ([#11697](https://github.com/astral-sh/ruff/pull/11697))
- Use the output format `full` by default ([#12010](https://github.com/astral-sh/ruff/pull/12010))
- Don't log syntax errors to the console ([#11902](https://github.com/astral-sh/ruff/pull/11902))

### Rule changes

- \[`ruff`\] Fix false positives if `gettext` is imported using an alias (`RUF027`) ([#12025](https://github.com/astral-sh/ruff/pull/12025))
- \[`numpy`\] Update `trapz` and `in1d` deprecation (`NPY201`) ([#11948](https://github.com/astral-sh/ruff/pull/11948))
- \[`flake8-bandit`\] Modify diagnostic ranges for shell-related rules ([#10667](https://github.com/astral-sh/ruff/pull/10667))

### Server

- Closing an untitled, unsaved notebook document no longer throws an error ([#11942](https://github.com/astral-sh/ruff/pull/11942))
- Support the usage of tildes and environment variables in `logFile` ([#11945](https://github.com/astral-sh/ruff/pull/11945))
- Add option to configure whether to show syntax errors ([#12059](https://github.com/astral-sh/ruff/pull/12059))

### Bug fixes

- \[`pycodestyle`\] Avoid `E203` for f-string debug expression ([#12024](https://github.com/astral-sh/ruff/pull/12024))
- \[`pep8-naming`\] Match import-name ignores against both name and alias (`N812`, `N817`) ([#12033](https://github.com/astral-sh/ruff/pull/12033))
- \[`pyflakes`\] Detect assignments that shadow definitions (`F811`) ([#11961](https://github.com/astral-sh/ruff/pull/11961))

### Parser

- Emit a syntax error for an empty type parameter list ([#12030](https://github.com/astral-sh/ruff/pull/12030))
- Avoid consuming the newline for unterminated strings ([#12067](https://github.com/astral-sh/ruff/pull/12067))
- Do not include the newline in the unterminated string range ([#12017](https://github.com/astral-sh/ruff/pull/12017))
- Use the correct range to highlight line continuation errors ([#12016](https://github.com/astral-sh/ruff/pull/12016))
- Consider 2-character EOL before line continuations ([#12035](https://github.com/astral-sh/ruff/pull/12035))
- Consider line continuation character for re-lexing ([#12008](https://github.com/astral-sh/ruff/pull/12008))

### Other changes

- Upgrade the Unicode table used for measuring the line-length ([#11194](https://github.com/astral-sh/ruff/pull/11194))
- Remove the deprecation error message for the nursery selector ([#10172](https://github.com/astral-sh/ruff/pull/10172))

## 0.4.10

### Parser

- Implement re-lexing logic for better error recovery ([#11845](https://github.com/astral-sh/ruff/pull/11845))

### Rule changes

- \[`flake8-copyright`\] Update `CPY001` to check the first 4096 bytes instead of 1024 ([#11927](https://github.com/astral-sh/ruff/pull/11927))
- \[`pycodestyle`\] Update `E999` to show all syntax errors instead of just the first one ([#11900](https://github.com/astral-sh/ruff/pull/11900))

### Server

- Add tracing setup guide to Helix documentation ([#11883](https://github.com/astral-sh/ruff/pull/11883))
- Add tracing setup guide to Neovim documentation ([#11884](https://github.com/astral-sh/ruff/pull/11884))
- Defer notebook cell deletion to avoid an error message ([#11864](https://github.com/astral-sh/ruff/pull/11864))

### Security

- Guard against malicious ecosystem comment artifacts ([#11879](https://github.com/astral-sh/ruff/pull/11879))

## 0.4.9

### Preview features

- \[`pylint`\] Implement `consider-dict-items` (`C0206`) ([#11688](https://github.com/astral-sh/ruff/pull/11688))
- \[`refurb`\] Implement `repeated-global` (`FURB154`) ([#11187](https://github.com/astral-sh/ruff/pull/11187))

### Rule changes

- \[`pycodestyle`\] Adapt fix for `E203` to work identical to `ruff format` ([#10999](https://github.com/astral-sh/ruff/pull/10999))

### Formatter

- Fix formatter instability for lines only consisting of zero-width characters ([#11748](https://github.com/astral-sh/ruff/pull/11748))

### Server

- Add supported commands in server capabilities ([#11850](https://github.com/astral-sh/ruff/pull/11850))
- Use real file path when available in `ruff server` ([#11800](https://github.com/astral-sh/ruff/pull/11800))
- Improve error message when a command is run on an unavailable document ([#11823](https://github.com/astral-sh/ruff/pull/11823))
- Introduce the `ruff.printDebugInformation` command ([#11831](https://github.com/astral-sh/ruff/pull/11831))
- Tracing system now respects log level and trace level, with options to log to a file ([#11747](https://github.com/astral-sh/ruff/pull/11747))

### CLI

- Handle non-printable characters in diff view ([#11687](https://github.com/astral-sh/ruff/pull/11687))

### Bug fixes

- \[`refurb`\] Avoid suggesting starmap when arguments are used outside call (`FURB140`) ([#11830](https://github.com/astral-sh/ruff/pull/11830))
- \[`flake8-bugbear`\] Avoid panic in `B909` when checking large loop blocks ([#11772](https://github.com/astral-sh/ruff/pull/11772))
- \[`refurb`\] Fix misbehavior of `operator.itemgetter` when getter param is a tuple (`FURB118`) ([#11774](https://github.com/astral-sh/ruff/pull/11774))

## 0.4.8

### Performance

- Linter performance has been improved by around 10% on some microbenchmarks by refactoring the lexer and parser to maintain synchronicity between them ([#11457](https://github.com/astral-sh/ruff/pull/11457))

### Preview features

- \[`flake8-bugbear`\] Implement `return-in-generator` (`B901`) ([#11644](https://github.com/astral-sh/ruff/pull/11644))
- \[`flake8-pyi`\] Implement `pep484-style-positional-only-parameter` (`PYI063`) ([#11699](https://github.com/astral-sh/ruff/pull/11699))
- \[`pygrep_hooks`\] Check blanket ignores via file-level pragmas (`PGH004`) ([#11540](https://github.com/astral-sh/ruff/pull/11540))

### Rule changes

- \[`pyupgrade`\] Update `UP035` for Python 3.13 and the latest version of `typing_extensions` ([#11693](https://github.com/astral-sh/ruff/pull/11693))
- \[`numpy`\] Update `NPY001` rule for NumPy 2.0 ([#11735](https://github.com/astral-sh/ruff/pull/11735))

### Server

- Formatting a document with syntax problems no longer spams a visible error popup ([#11745](https://github.com/astral-sh/ruff/pull/11745))

### CLI

- Add RDJson support for `--output-format` flag ([#11682](https://github.com/astral-sh/ruff/pull/11682))

### Bug fixes

- \[`pyupgrade`\] Write empty string in lieu of panic when fixing `UP032` ([#11696](https://github.com/astral-sh/ruff/pull/11696))
- \[`flake8-simplify`\] Simplify double negatives in `SIM103` ([#11684](https://github.com/astral-sh/ruff/pull/11684))
- Ensure the expression generator adds a newline before `type` statements ([#11720](https://github.com/astral-sh/ruff/pull/11720))
- Respect per-file ignores for blanket and redirected noqa rules ([#11728](https://github.com/astral-sh/ruff/pull/11728))

## 0.4.7

### Preview features

- \[`flake8-pyi`\] Implement `PYI064` ([#11325](https://github.com/astral-sh/ruff/pull/11325))
- \[`flake8-pyi`\] Implement `PYI066` ([#11541](https://github.com/astral-sh/ruff/pull/11541))
- \[`flake8-pyi`\] Implement `PYI057` ([#11486](https://github.com/astral-sh/ruff/pull/11486))
- \[`pyflakes`\] Enable `F822` in `__init__.py` files by default ([#11370](https://github.com/astral-sh/ruff/pull/11370))

### Formatter

- Fix incorrect placement of trailing stub function comments ([#11632](https://github.com/astral-sh/ruff/pull/11632))

### Server

- Respect file exclusions in `ruff server` ([#11590](https://github.com/astral-sh/ruff/pull/11590))
- Add support for documents not exist on disk ([#11588](https://github.com/astral-sh/ruff/pull/11588))
- Add Vim and Kate setup guide for `ruff server` ([#11615](https://github.com/astral-sh/ruff/pull/11615))

### Bug fixes

- Avoid removing newlines between docstring headers and rST blocks ([#11609](https://github.com/astral-sh/ruff/pull/11609))
- Infer indentation with imports when logical indent is absent ([#11608](https://github.com/astral-sh/ruff/pull/11608))
- Use char index rather than position for indent slice ([#11645](https://github.com/astral-sh/ruff/pull/11645))
- \[`flake8-comprehension`\] Strip parentheses around generators in `C400` ([#11607](https://github.com/astral-sh/ruff/pull/11607))
- Mark `repeated-isinstance-calls` as unsafe on Python 3.10 and later ([#11622](https://github.com/astral-sh/ruff/pull/11622))

## 0.4.6

### Breaking changes

- Use project-relative paths when calculating GitLab fingerprints ([#11532](https://github.com/astral-sh/ruff/pull/11532))
- Bump minimum supported Windows version to Windows 10 ([#11613](https://github.com/astral-sh/ruff/pull/11613))

### Preview features

- \[`flake8-async`\] Sleep with >24 hour interval should usually sleep forever (`ASYNC116`) ([#11498](https://github.com/astral-sh/ruff/pull/11498))

### Rule changes

- \[`numpy`\] Add missing functions to NumPy 2.0 migration rule ([#11528](https://github.com/astral-sh/ruff/pull/11528))
- \[`mccabe`\] Consider irrefutable pattern similar to `if .. else` for `C901` ([#11565](https://github.com/astral-sh/ruff/pull/11565))
- Consider `match`-`case` statements for `C901`, `PLR0912`, and `PLR0915` ([#11521](https://github.com/astral-sh/ruff/pull/11521))
- Remove empty strings when converting to f-string (`UP032`) ([#11524](https://github.com/astral-sh/ruff/pull/11524))
- \[`flake8-bandit`\] `request-without-timeout` should warn for `requests.request` ([#11548](https://github.com/astral-sh/ruff/pull/11548))
- \[`flake8-self`\] Ignore sunder accesses in `flake8-self` rules ([#11546](https://github.com/astral-sh/ruff/pull/11546))
- \[`pyupgrade`\] Lint for `TypeAliasType` usages (`UP040`) ([#11530](https://github.com/astral-sh/ruff/pull/11530))

### Server

- Respect excludes in `ruff server` configuration discovery ([#11551](https://github.com/astral-sh/ruff/pull/11551))
- Use default settings if initialization options is empty or not provided ([#11566](https://github.com/astral-sh/ruff/pull/11566))
- `ruff server` correctly treats `.pyi` files as stub files ([#11535](https://github.com/astral-sh/ruff/pull/11535))
- `ruff server` searches for configuration in parent directories ([#11537](https://github.com/astral-sh/ruff/pull/11537))
- `ruff server`: An empty code action filter no longer returns notebook source actions ([#11526](https://github.com/astral-sh/ruff/pull/11526))

### Bug fixes

- \[`flake8-logging-format`\] Fix autofix title in `logging-warn` (`G010`) ([#11514](https://github.com/astral-sh/ruff/pull/11514))
- \[`refurb`\] Avoid recommending `operator.itemgetter` with dependence on lambda arguments ([#11574](https://github.com/astral-sh/ruff/pull/11574))
- \[`flake8-simplify`\] Avoid recommending context manager in `__enter__` implementations ([#11575](https://github.com/astral-sh/ruff/pull/11575))
- Create intermediary directories for `--output-file` ([#11550](https://github.com/astral-sh/ruff/pull/11550))
- Propagate reads on global variables ([#11584](https://github.com/astral-sh/ruff/pull/11584))
- Treat all `singledispatch` arguments as runtime-required ([#11523](https://github.com/astral-sh/ruff/pull/11523))

## 0.4.5

### Ruff's language server is now in Beta

`v0.4.5` marks the official Beta release of `ruff server`, an integrated language server built into Ruff.
`ruff server` supports the same feature set as `ruff-lsp`, powering linting, formatting, and
code fixes in Ruff's editor integrations -- but with superior performance and
no installation required. We'd love your feedback!

You can enable `ruff server` in the [VS Code extension](https://github.com/astral-sh/ruff-vscode?tab=readme-ov-file#enabling-the-rust-based-language-server) today.

To read more about this exciting milestone, check out our [blog post](https://astral.sh/blog/ruff-v0.4.5)!

### Rule changes

- \[`flake8-future-annotations`\] Reword `future-rewritable-type-annotation` (`FA100`) message ([#11381](https://github.com/astral-sh/ruff/pull/11381))
- \[`isort`\] Expanded the set of standard-library modules to include `_string`, etc. ([#11374](https://github.com/astral-sh/ruff/pull/11374))
- \[`pycodestyle`\] Consider soft keywords for `E27` rules ([#11446](https://github.com/astral-sh/ruff/pull/11446))
- \[`pyflakes`\] Recommend adding unused import bindings to `__all__` ([#11314](https://github.com/astral-sh/ruff/pull/11314))
- \[`pyflakes`\] Update documentation and deprecate `ignore_init_module_imports` ([#11436](https://github.com/astral-sh/ruff/pull/11436))
- \[`pyupgrade`\] Mark quotes as unnecessary for non-evaluated annotations ([#11485](https://github.com/astral-sh/ruff/pull/11485))

### Formatter

- Avoid multiline quotes warning with `quote-style = preserve` ([#11490](https://github.com/astral-sh/ruff/pull/11490))

### Server

- Support Jupyter Notebook files ([#11206](https://github.com/astral-sh/ruff/pull/11206))
- Support `noqa` comment code actions ([#11276](https://github.com/astral-sh/ruff/pull/11276))
- Fix automatic configuration reloading ([#11492](https://github.com/astral-sh/ruff/pull/11492))
- Fix several issues with configuration in Neovim and Helix ([#11497](https://github.com/astral-sh/ruff/pull/11497))

### CLI

- Add `--output-format` as a CLI option for `ruff config` ([#11438](https://github.com/astral-sh/ruff/pull/11438))

### Bug fixes

- Avoid `PLE0237` for property with setter ([#11377](https://github.com/astral-sh/ruff/pull/11377))
- Avoid `TCH005` for `if` stmt with `elif`/`else` block ([#11376](https://github.com/astral-sh/ruff/pull/11376))
- Avoid flagging `__future__` annotations as required for non-evaluated type annotations ([#11414](https://github.com/astral-sh/ruff/pull/11414))
- Check for ruff executable in 'bin' directory as installed by 'pip install --target'. ([#11450](https://github.com/astral-sh/ruff/pull/11450))
- Sort edits prior to deduplicating in quotation fix ([#11452](https://github.com/astral-sh/ruff/pull/11452))
- Treat escaped newline as valid sequence ([#11465](https://github.com/astral-sh/ruff/pull/11465))
- \[`flake8-pie`\] Preserve parentheses in `unnecessary-dict-kwargs` ([#11372](https://github.com/astral-sh/ruff/pull/11372))
- \[`pylint`\] Ignore `__slots__` with dynamic values ([#11488](https://github.com/astral-sh/ruff/pull/11488))
- \[`pylint`\] Remove `try` body from branch counting ([#11487](https://github.com/astral-sh/ruff/pull/11487))
- \[`refurb`\] Respect operator precedence in `FURB110` ([#11464](https://github.com/astral-sh/ruff/pull/11464))

### Documentation

- Add `--preview` to the README ([#11395](https://github.com/astral-sh/ruff/pull/11395))
- Add Python 3.13 to list of allowed Python versions ([#11411](https://github.com/astral-sh/ruff/pull/11411))
- Simplify Neovim setup documentation ([#11489](https://github.com/astral-sh/ruff/pull/11489))
- Update CONTRIBUTING.md to reflect the new parser ([#11434](https://github.com/astral-sh/ruff/pull/11434))
- Update server documentation with new migration guide ([#11499](https://github.com/astral-sh/ruff/pull/11499))
- \[`pycodestyle`\] Clarify motivation for `E713` and `E714` ([#11483](https://github.com/astral-sh/ruff/pull/11483))
- \[`pyflakes`\] Update docs to describe WAI behavior (F541) ([#11362](https://github.com/astral-sh/ruff/pull/11362))
- \[`pylint`\] Clearly indicate what is counted as a branch ([#11423](https://github.com/astral-sh/ruff/pull/11423))

## 0.4.4

### Preview features

- \[`pycodestyle`\] Ignore end-of-line comments when determining blank line rules ([#11342](https://github.com/astral-sh/ruff/pull/11342))
- \[`pylint`\] Detect `pathlib.Path.open` calls in `unspecified-encoding` (`PLW1514`) ([#11288](https://github.com/astral-sh/ruff/pull/11288))
- \[`flake8-pyi`\] Implement `PYI059` (`generic-not-last-base-class`) ([#11233](https://github.com/astral-sh/ruff/pull/11233))
- \[`flake8-pyi`\] Implement `PYI062` (`duplicate-literal-member`) ([#11269](https://github.com/astral-sh/ruff/pull/11269))

### Rule changes

- \[`flake8-boolean-trap`\] Allow passing booleans as positional-only arguments in code such as `set(True)` ([#11287](https://github.com/astral-sh/ruff/pull/11287))
- \[`flake8-bugbear`\] Ignore enum classes in `cached-instance-method` (`B019`) ([#11312](https://github.com/astral-sh/ruff/pull/11312))

### Server

- Expand tildes when resolving Ruff server configuration file ([#11283](https://github.com/astral-sh/ruff/pull/11283))
- Fix `ruff server` hanging after Neovim closes ([#11291](https://github.com/astral-sh/ruff/pull/11291))
- Editor settings are used by default if no file-based configuration exists ([#11266](https://github.com/astral-sh/ruff/pull/11266))

### Bug fixes

- \[`pylint`\] Consider `with` statements for `too-many-branches` (`PLR0912`) ([#11321](https://github.com/astral-sh/ruff/pull/11321))
- \[`flake8-blind-except`, `tryceratops`\] Respect logged and re-raised expressions in nested statements (`BLE001`, `TRY201`) ([#11301](https://github.com/astral-sh/ruff/pull/11301))
- Recognise assignments such as `__all__ = builtins.list(["foo", "bar"])` as valid `__all__` definitions ([#11335](https://github.com/astral-sh/ruff/pull/11335))

## 0.4.3

### Enhancements

- Add support for PEP 696 syntax ([#11120](https://github.com/astral-sh/ruff/pull/11120))

### Preview features

- \[`refurb`\] Use function range for `reimplemented-operator` diagnostics ([#11271](https://github.com/astral-sh/ruff/pull/11271))
- \[`refurb`\] Ignore methods in `reimplemented-operator` (`FURB118`) ([#11270](https://github.com/astral-sh/ruff/pull/11270))
- \[`refurb`\] Implement `fstring-number-format` (`FURB116`) ([#10921](https://github.com/astral-sh/ruff/pull/10921))
- \[`ruff`\] Implement `redirected-noqa` (`RUF101`) ([#11052](https://github.com/astral-sh/ruff/pull/11052))
- \[`pyflakes`\] Distinguish between first-party and third-party imports for fix suggestions ([#11168](https://github.com/astral-sh/ruff/pull/11168))

### Rule changes

- \[`flake8-bugbear`\] Ignore non-abstract class attributes when enforcing `B024` ([#11210](https://github.com/astral-sh/ruff/pull/11210))
- \[`flake8-logging`\] Include inline instantiations when detecting loggers ([#11154](https://github.com/astral-sh/ruff/pull/11154))
- \[`pylint`\] Also emit `PLR0206` for properties with variadic parameters ([#11200](https://github.com/astral-sh/ruff/pull/11200))
- \[`ruff`\] Detect duplicate codes as part of `unused-noqa` (`RUF100`) ([#10850](https://github.com/astral-sh/ruff/pull/10850))

### Formatter

- Avoid multiline expression if format specifier is present ([#11123](https://github.com/astral-sh/ruff/pull/11123))

### LSP

- Write `ruff server` setup guide for Helix ([#11183](https://github.com/astral-sh/ruff/pull/11183))
- `ruff server` no longer hangs after shutdown ([#11222](https://github.com/astral-sh/ruff/pull/11222))
- `ruff server` reads from a configuration TOML file in the user configuration directory if no local configuration exists ([#11225](https://github.com/astral-sh/ruff/pull/11225))
- `ruff server` respects `per-file-ignores` configuration ([#11224](https://github.com/astral-sh/ruff/pull/11224))
- `ruff server`: Support a custom TOML configuration file ([#11140](https://github.com/astral-sh/ruff/pull/11140))
- `ruff server`: Support setting to prioritize project configuration over editor configuration ([#11086](https://github.com/astral-sh/ruff/pull/11086))

### Bug fixes

- Avoid debug assertion around NFKC renames ([#11249](https://github.com/astral-sh/ruff/pull/11249))
- \[`pyflakes`\] Prioritize `redefined-while-unused` over `unused-import` ([#11173](https://github.com/astral-sh/ruff/pull/11173))
- \[`ruff`\] Respect `async` expressions in comprehension bodies ([#11219](https://github.com/astral-sh/ruff/pull/11219))
- \[`pygrep_hooks`\] Fix `blanket-noqa` panic when last line has noqa with no newline (`PGH004`) ([#11108](https://github.com/astral-sh/ruff/pull/11108))
- \[`perflint`\] Ignore list-copy recommendations for async `for` loops ([#11250](https://github.com/astral-sh/ruff/pull/11250))
- \[`pyflakes`\] Improve `invalid-print-syntax` documentation ([#11171](https://github.com/astral-sh/ruff/pull/11171))

### Performance

- Avoid allocations for isort module names ([#11251](https://github.com/astral-sh/ruff/pull/11251))
- Build a separate ARM wheel for macOS ([#11149](https://github.com/astral-sh/ruff/pull/11149))

### Windows

- Increase the minimum requirement to Windows 10.

## 0.4.2

### Rule changes

- \[`flake8-pyi`\] Allow for overloaded `__exit__` and `__aexit__` definitions (`PYI036`) ([#11057](https://github.com/astral-sh/ruff/pull/11057))
- \[`pyupgrade`\] Catch usages of `"%s" % var` and provide an unsafe fix (`UP031`) ([#11019](https://github.com/astral-sh/ruff/pull/11019))
- \[`refurb`\] Implement new rule that suggests min/max over `sorted()` (`FURB192`) ([#10868](https://github.com/astral-sh/ruff/pull/10868))

### Server

- Fix an issue with missing diagnostics for Neovim and Helix ([#11092](https://github.com/astral-sh/ruff/pull/11092))
- Implement hover documentation for `noqa` codes ([#11096](https://github.com/astral-sh/ruff/pull/11096))
- Introduce common Ruff configuration options with new server settings ([#11062](https://github.com/astral-sh/ruff/pull/11062))

### Bug fixes

- Use `macos-12` for building release wheels to enable macOS 11 compatibility ([#11146](https://github.com/astral-sh/ruff/pull/11146))
- \[`flake8-blind-expect`\] Allow raise from in `BLE001` ([#11131](https://github.com/astral-sh/ruff/pull/11131))
- \[`flake8-pyi`\] Allow simple assignments to `None` in enum class scopes (`PYI026`) ([#11128](https://github.com/astral-sh/ruff/pull/11128))
- \[`flake8-simplify`\] Avoid raising `SIM911` for non-`zip` attribute calls ([#11126](https://github.com/astral-sh/ruff/pull/11126))
- \[`refurb`\] Avoid `operator.itemgetter` suggestion for single-item tuple ([#11095](https://github.com/astral-sh/ruff/pull/11095))
- \[`ruff`\] Respect per-file-ignores for `RUF100` with no other diagnostics ([#11058](https://github.com/astral-sh/ruff/pull/11058))
- \[`ruff`\] Fix async comprehension false positive (`RUF029`) ([#11070](https://github.com/astral-sh/ruff/pull/11070))

### Documentation

- \[`flake8-bugbear`\] Document explicitly disabling strict zip (`B905`) ([#11040](https://github.com/astral-sh/ruff/pull/11040))
- \[`flake8-type-checking`\] Mention `lint.typing-modules` in `TCH001`, `TCH002`, and `TCH003` ([#11144](https://github.com/astral-sh/ruff/pull/11144))
- \[`isort`\] Improve documentation around custom `isort` sections ([#11050](https://github.com/astral-sh/ruff/pull/11050))
- \[`pylint`\] Fix documentation oversight for `invalid-X-returns` ([#11094](https://github.com/astral-sh/ruff/pull/11094))

### Performance

- Use `matchit` to resolve per-file settings ([#11111](https://github.com/astral-sh/ruff/pull/11111))

## 0.4.1

### Preview features

- \[`pylint`\] Implement `invalid-hash-returned` (`PLE0309`) ([#10961](https://github.com/astral-sh/ruff/pull/10961))
- \[`pylint`\] Implement `invalid-index-returned` (`PLE0305`) ([#10962](https://github.com/astral-sh/ruff/pull/10962))

### Bug fixes

- \[`pylint`\] Allow `NoReturn`-like functions for `__str__`, `__len__`, etc. (`PLE0307`) ([#11017](https://github.com/astral-sh/ruff/pull/11017))
- Parser: Use empty range when there's "gap" in token source ([#11032](https://github.com/astral-sh/ruff/pull/11032))
- \[`ruff`\] Ignore stub functions in `unused-async` (`RUF029`) ([#11026](https://github.com/astral-sh/ruff/pull/11026))
- Parser: Expect indented case block instead of match stmt ([#11033](https://github.com/astral-sh/ruff/pull/11033))

## 0.4.0

### A new, hand-written parser

Ruff's new parser is **>2x faster**, which translates to a **20-40% speedup** for all linting and formatting invocations.
There's a lot to say about this exciting change, so check out the [blog post](https://astral.sh/blog/ruff-v0.4.0) for more details!

See [#10036](https://github.com/astral-sh/ruff/pull/10036) for implementation details.

### A new language server in Rust

With this release, we also want to highlight our new language server. `ruff server` is a Rust-powered language
server that comes built-in with Ruff. It can be used with any editor that supports the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/) (LSP).
It uses a multi-threaded, lock-free architecture inspired by `rust-analyzer` and it will open the door for a lot
of exciting features. It’s also faster than our previous [Python-based language server](https://github.com/astral-sh/ruff-lsp)
-- but you probably guessed that already.

`ruff server` is only in alpha, but it has a lot of features that you can try out today:

- Lints Python files automatically and shows quick-fixes when available
- Formats Python files, with support for range formatting
- Comes with commands for quickly performing actions: `ruff.applyAutofix`, `ruff.applyFormat`, and `ruff.applyOrganizeImports`
- Supports `source.fixAll` and `source.organizeImports` source actions
- Automatically reloads your project configuration when you change it

To setup `ruff server` with your editor, refer to the [README.md](https://github.com/astral-sh/ruff/blob/main/crates/ruff_server/README.md).

### Preview features

- \[`pycodestyle`\] Do not trigger `E3` rules on `def`s following a function/method with a dummy body ([#10704](https://github.com/astral-sh/ruff/pull/10704))
- \[`pylint`\] Implement `invalid-bytes-returned` (`E0308`) ([#10959](https://github.com/astral-sh/ruff/pull/10959))
- \[`pylint`\] Implement `invalid-length-returned` (`E0303`) ([#10963](https://github.com/astral-sh/ruff/pull/10963))
- \[`pylint`\] Implement `self-cls-assignment` (`W0642`) ([#9267](https://github.com/astral-sh/ruff/pull/9267))
- \[`pylint`\] Omit stubs from `invalid-bool` and `invalid-str-return-type` ([#11008](https://github.com/astral-sh/ruff/pull/11008))
- \[`ruff`\] New rule `unused-async` (`RUF029`) to detect unneeded `async` keywords on functions ([#9966](https://github.com/astral-sh/ruff/pull/9966))

### Rule changes

- \[`flake8-bandit`\] Allow `urllib.request.urlopen` calls with static `Request` argument (`S310`) ([#10964](https://github.com/astral-sh/ruff/pull/10964))
- \[`flake8-bugbear`\] Treat `raise NotImplemented`-only bodies as stub functions (`B006`) ([#10990](https://github.com/astral-sh/ruff/pull/10990))
- \[`flake8-slots`\] Respect same-file `Enum` subclasses (`SLOT000`) ([#11006](https://github.com/astral-sh/ruff/pull/11006))
- \[`pylint`\] Support inverted comparisons (`PLR1730`) ([#10920](https://github.com/astral-sh/ruff/pull/10920))

### Linter

- Improve handling of builtin symbols in linter rules ([#10919](https://github.com/astral-sh/ruff/pull/10919))
- Improve display of rules in `--show-settings` ([#11003](https://github.com/astral-sh/ruff/pull/11003))
- Improve inference capabilities of the `BuiltinTypeChecker` ([#10976](https://github.com/astral-sh/ruff/pull/10976))
- Resolve classes and functions relative to script name ([#10965](https://github.com/astral-sh/ruff/pull/10965))
- Improve performance of `RuleTable::any_enabled` ([#10971](https://github.com/astral-sh/ruff/pull/10971))

### Server

*This section is devoted to updates for our new language server, written in Rust.*

- Enable ruff-specific source actions ([#10916](https://github.com/astral-sh/ruff/pull/10916))
- Refreshes diagnostics for open files when file configuration is changed ([#10988](https://github.com/astral-sh/ruff/pull/10988))
- Important errors are now shown as popups ([#10951](https://github.com/astral-sh/ruff/pull/10951))
- Introduce settings for directly configuring the linter and formatter ([#10984](https://github.com/astral-sh/ruff/pull/10984))
- Resolve configuration for each document individually ([#10950](https://github.com/astral-sh/ruff/pull/10950))
- Write a setup guide for Neovim ([#10987](https://github.com/astral-sh/ruff/pull/10987))

### Configuration

- Add `RUFF_OUTPUT_FILE` environment variable support ([#10992](https://github.com/astral-sh/ruff/pull/10992))

### Bug fixes

- Avoid `non-augmented-assignment` for reversed, non-commutative operators (`PLR6104`) ([#10909](https://github.com/astral-sh/ruff/pull/10909))
- Limit commutative non-augmented-assignments to primitive data types (`PLR6104`) ([#10912](https://github.com/astral-sh/ruff/pull/10912))
- Respect `per-file-ignores` for `RUF100` on blanket `# noqa` ([#10908](https://github.com/astral-sh/ruff/pull/10908))
- Consider `if` expression for parenthesized with items parsing ([#11010](https://github.com/astral-sh/ruff/pull/11010))
- Consider binary expr for parenthesized with items parsing ([#11012](https://github.com/astral-sh/ruff/pull/11012))
- Reset `FOR_TARGET` context for all kinds of parentheses ([#11009](https://github.com/astral-sh/ruff/pull/11009))

## 0.3.7

### Preview features

- \[`flake8-bugbear`\] Implement `loop-iterator-mutation` (`B909`) ([#9578](https://github.com/astral-sh/ruff/pull/9578))
- \[`pylint`\] Implement rule to prefer augmented assignment (`PLR6104`) ([#9932](https://github.com/astral-sh/ruff/pull/9932))

### Bug fixes

- Avoid TOCTOU errors in cache initialization ([#10884](https://github.com/astral-sh/ruff/pull/10884))
- \[`pylint`\] Recode `nan-comparison` rule to `W0177` ([#10894](https://github.com/astral-sh/ruff/pull/10894))
- \[`pylint`\] Reverse min-max logic in `if-stmt-min-max` ([#10890](https://github.com/astral-sh/ruff/pull/10890))

## 0.3.6

### Preview features

- \[`pylint`\] Implement `bad-staticmethod-argument` (`PLW0211`) ([#10781](https://github.com/astral-sh/ruff/pull/10781))
- \[`pylint`\] Implement `if-stmt-min-max` (`PLR1730`, `PLR1731`) ([#10002](https://github.com/astral-sh/ruff/pull/10002))
- \[`pyupgrade`\] Replace `str,Enum` multiple inheritance with `StrEnum` `UP042` ([#10713](https://github.com/astral-sh/ruff/pull/10713))
- \[`refurb`\] Implement `if-expr-instead-of-or-operator` (`FURB110`) ([#10687](https://github.com/astral-sh/ruff/pull/10687))
- \[`refurb`\] Implement `int-on-sliced-str` (`FURB166`) ([#10650](https://github.com/astral-sh/ruff/pull/10650))
- \[`refurb`\] Implement `write-whole-file` (`FURB103`) ([#10802](https://github.com/astral-sh/ruff/pull/10802))
- \[`refurb`\] Support `itemgetter` in `reimplemented-operator` (`FURB118`) ([#10526](https://github.com/astral-sh/ruff/pull/10526))
- \[`flake8_comprehensions`\] Add `sum`/`min`/`max` to unnecessary comprehension check (`C419`) ([#10759](https://github.com/astral-sh/ruff/pull/10759))

### Rule changes

- \[`pydocstyle`\] Require capitalizing docstrings where the first sentence is a single word (`D403`) ([#10776](https://github.com/astral-sh/ruff/pull/10776))
- \[`pycodestyle`\] Ignore annotated lambdas in class scopes (`E731`) ([#10720](https://github.com/astral-sh/ruff/pull/10720))
- \[`flake8-pyi`\] Various improvements to PYI034 ([#10807](https://github.com/astral-sh/ruff/pull/10807))
- \[`flake8-slots`\] Flag subclasses of call-based `typing.NamedTuple`s as well as subclasses of `collections.namedtuple()` (`SLOT002`) ([#10808](https://github.com/astral-sh/ruff/pull/10808))
- \[`pyflakes`\] Allow forward references in class bases in stub files (`F821`) ([#10779](https://github.com/astral-sh/ruff/pull/10779))
- \[`pygrep-hooks`\] Improve `blanket-noqa` error message (`PGH004`) ([#10851](https://github.com/astral-sh/ruff/pull/10851))

### CLI

- Support `FORCE_COLOR` env var ([#10839](https://github.com/astral-sh/ruff/pull/10839))

### Configuration

- Support negated patterns in `[extend-]per-file-ignores` ([#10852](https://github.com/astral-sh/ruff/pull/10852))

### Bug fixes

- \[`flake8-import-conventions`\] Accept non-aliased (but correct) import in `unconventional-import-alias` (`ICN001`) ([#10729](https://github.com/astral-sh/ruff/pull/10729))
- \[`flake8-quotes`\] Add semantic model flag when inside f-string replacement field ([#10766](https://github.com/astral-sh/ruff/pull/10766))
- \[`pep8-naming`\] Recursively resolve `TypeDicts` for N815 violations ([#10719](https://github.com/astral-sh/ruff/pull/10719))
- \[`flake8-quotes`\] Respect `Q00*` ignores in `flake8-quotes` rules ([#10728](https://github.com/astral-sh/ruff/pull/10728))
- \[`flake8-simplify`\] Show negated condition in `needless-bool` diagnostics (`SIM103`) ([#10854](https://github.com/astral-sh/ruff/pull/10854))
- \[`ruff`\] Use within-scope shadowed bindings in `asyncio-dangling-task` (`RUF006`) ([#10793](https://github.com/astral-sh/ruff/pull/10793))
- \[`flake8-pytest-style`\] Fix single-tuple conversion in `pytest-parametrize-values-wrong-type` (`PT007`) ([#10862](https://github.com/astral-sh/ruff/pull/10862))
- \[`flake8-return`\] Ignore assignments to annotated variables in `unnecessary-assign` (`RET504`) ([#10741](https://github.com/astral-sh/ruff/pull/10741))
- \[`refurb`\] Do not allow any keyword arguments for `read-whole-file` in `rb` mode (`FURB101`) ([#10803](https://github.com/astral-sh/ruff/pull/10803))
- \[`pylint`\] Don't recommend decorating staticmethods with `@singledispatch` (`PLE1519`, `PLE1520`) ([#10637](https://github.com/astral-sh/ruff/pull/10637))
- \[`pydocstyle`\] Use section name range for all section-related docstring diagnostics ([#10740](https://github.com/astral-sh/ruff/pull/10740))
- Respect `# noqa` directives on `__all__` openers ([#10798](https://github.com/astral-sh/ruff/pull/10798))

## 0.3.5

### Preview features

- \[`pylint`\] Implement `modified-iterating-set` (`E4703`) ([#10473](https://github.com/astral-sh/ruff/pull/10473))
- \[`refurb`\] Implement `for-loop-set-mutations` (`FURB142`) ([#10583](https://github.com/astral-sh/ruff/pull/10583))
- \[`refurb`\] Implement `unnecessary-from-float` (`FURB164`) ([#10647](https://github.com/astral-sh/ruff/pull/10647))
- \[`refurb`\] Implement `verbose-decimal-constructor` (`FURB157`) ([#10533](https://github.com/astral-sh/ruff/pull/10533))

### Rule changes

- \[`flake8-comprehensions`\] Handled special case for `C401` which also matches `C416` ([#10596](https://github.com/astral-sh/ruff/pull/10596))
- \[`flake8-pyi`\] Mark `unaliased-collections-abc-set-import` fix as "safe" for more cases in stub files (`PYI025`) ([#10547](https://github.com/astral-sh/ruff/pull/10547))
- \[`numpy`\] Add `row_stack` to NumPy 2.0 migration rule ([#10646](https://github.com/astral-sh/ruff/pull/10646))
- \[`pycodestyle`\] Allow cell magics before an import (`E402`) ([#10545](https://github.com/astral-sh/ruff/pull/10545))
- \[`pycodestyle`\] Avoid blank line rules for the first logical line in cell ([#10291](https://github.com/astral-sh/ruff/pull/10291))

### Configuration

- Respected nested namespace packages ([#10541](https://github.com/astral-sh/ruff/pull/10541))
- \[`flake8-boolean-trap`\] Add setting for user defined allowed boolean trap ([#10531](https://github.com/astral-sh/ruff/pull/10531))

### Bug fixes

- Correctly handle references in `__all__` definitions when renaming symbols in autofixes ([#10527](https://github.com/astral-sh/ruff/pull/10527))
- Track ranges of names inside `__all__` definitions ([#10525](https://github.com/astral-sh/ruff/pull/10525))
- \[`flake8-bugbear`\] Avoid false positive for usage after `continue` (`B031`) ([#10539](https://github.com/astral-sh/ruff/pull/10539))
- \[`flake8-copyright`\] Accept commas in default copyright pattern ([#9498](https://github.com/astral-sh/ruff/pull/9498))
- \[`flake8-datetimez`\] Allow f-strings with `%z` for `DTZ007` ([#10651](https://github.com/astral-sh/ruff/pull/10651))
- \[`flake8-pytest-style`\] Fix `PT014` autofix for last item in list ([#10532](https://github.com/astral-sh/ruff/pull/10532))
- \[`flake8-quotes`\] Ignore `Q000`, `Q001` when string is inside forward ref ([#10585](https://github.com/astral-sh/ruff/pull/10585))
- \[`isort`\] Always place non-relative imports after relative imports ([#10669](https://github.com/astral-sh/ruff/pull/10669))
- \[`isort`\] Respect Unicode characters in import sorting ([#10529](https://github.com/astral-sh/ruff/pull/10529))
- \[`pyflakes`\] Fix F821 false negatives when `from __future__ import annotations` is active (attempt 2) ([#10524](https://github.com/astral-sh/ruff/pull/10524))
- \[`pyflakes`\] Make `unnecessary-lambda` an always-unsafe fix ([#10668](https://github.com/astral-sh/ruff/pull/10668))
- \[`pylint`\] Fixed false-positive on the rule `PLW1641` (`eq-without-hash`) ([#10566](https://github.com/astral-sh/ruff/pull/10566))
- \[`ruff`\] Fix panic in unused `# noqa` removal with multi-byte space (`RUF100`) ([#10682](https://github.com/astral-sh/ruff/pull/10682))

### Documentation

- Add PR title format to `CONTRIBUTING.md` ([#10665](https://github.com/astral-sh/ruff/pull/10665))
- Fix list markup to include blank lines required ([#10591](https://github.com/astral-sh/ruff/pull/10591))
- Put `flake8-logging` next to the other flake8 plugins in registry ([#10587](https://github.com/astral-sh/ruff/pull/10587))
- \[`flake8-bandit`\] Update warning message for rule `S305` to address insecure block cipher mode use ([#10602](https://github.com/astral-sh/ruff/pull/10602))
- \[`flake8-bugbear`\] Document use of anonymous assignment in `useless-expression` ([#10551](https://github.com/astral-sh/ruff/pull/10551))
- \[`flake8-datetimez`\] Clarify error messages and docs for `DTZ` rules ([#10621](https://github.com/astral-sh/ruff/pull/10621))
- \[`pycodestyle`\] Use same before vs. after numbers for `space-around-operator` ([#10640](https://github.com/astral-sh/ruff/pull/10640))
- \[`ruff`\] Change `quadratic-list-summation` docs to use `iadd` consistently ([#10666](https://github.com/astral-sh/ruff/pull/10666))

## 0.3.4

### Preview features

- \[`flake8-simplify`\] Detect implicit `else` cases in `needless-bool` (`SIM103`) ([#10414](https://github.com/astral-sh/ruff/pull/10414))
- \[`pylint`\] Implement `nan-comparison` (`PLW0117`) ([#10401](https://github.com/astral-sh/ruff/pull/10401))
- \[`pylint`\] Implement `nonlocal-and-global` (`E115`) ([#10407](https://github.com/astral-sh/ruff/pull/10407))
- \[`pylint`\] Implement `singledispatchmethod-function` (`PLE5120`) ([#10428](https://github.com/astral-sh/ruff/pull/10428))
- \[`refurb`\] Implement `list-reverse-copy` (`FURB187`) ([#10212](https://github.com/astral-sh/ruff/pull/10212))

### Rule changes

- \[`flake8-pytest-style`\] Add automatic fix for `pytest-parametrize-values-wrong-type` (`PT007`) ([#10461](https://github.com/astral-sh/ruff/pull/10461))
- \[`pycodestyle`\] Allow SPDX license headers to exceed the line length (`E501`) ([#10481](https://github.com/astral-sh/ruff/pull/10481))

### Formatter

- Fix unstable formatting for trailing subscript end-of-line comment ([#10492](https://github.com/astral-sh/ruff/pull/10492))

### Bug fixes

- Avoid code comment detection in PEP 723 script tags ([#10464](https://github.com/astral-sh/ruff/pull/10464))
- Avoid incorrect tuple transformation in single-element case (`C409`) ([#10491](https://github.com/astral-sh/ruff/pull/10491))
- Bug fix: Prevent fully defined links [`name`](link) from being reformatted ([#10442](https://github.com/astral-sh/ruff/pull/10442))
- Consider raw source code for `W605` ([#10480](https://github.com/astral-sh/ruff/pull/10480))
- Docs: Link inline settings when not part of options section ([#10499](https://github.com/astral-sh/ruff/pull/10499))
- Don't treat annotations as redefinitions in `.pyi` files ([#10512](https://github.com/astral-sh/ruff/pull/10512))
- Fix `E231` bug: Inconsistent catch compared to pycodestyle, such as when dict nested in list ([#10469](https://github.com/astral-sh/ruff/pull/10469))
- Fix pylint upstream categories not showing in docs ([#10441](https://github.com/astral-sh/ruff/pull/10441))
- Add missing `Options` references to blank line docs ([#10498](https://github.com/astral-sh/ruff/pull/10498))
- 'Revert "F821: Fix false negatives in .py files when `from __future__ import annotations` is active (#10362)"' ([#10513](https://github.com/astral-sh/ruff/pull/10513))
- Apply NFKC normalization to unicode identifiers in the lexer ([#10412](https://github.com/astral-sh/ruff/pull/10412))
- Avoid failures due to non-deterministic binding ordering ([#10478](https://github.com/astral-sh/ruff/pull/10478))
- \[`flake8-bugbear`\] Allow tuples of exceptions (`B030`) ([#10437](https://github.com/astral-sh/ruff/pull/10437))
- \[`flake8-quotes`\] Avoid syntax errors due to invalid quotes (`Q000, Q002`) ([#10199](https://github.com/astral-sh/ruff/pull/10199))

## 0.3.3

### Preview features

- \[`flake8-bandit`\]: Implement `S610` rule ([#10316](https://github.com/astral-sh/ruff/pull/10316))
- \[`pycodestyle`\] Implement `blank-line-at-end-of-file` (`W391`) ([#10243](https://github.com/astral-sh/ruff/pull/10243))
- \[`pycodestyle`\] Implement `redundant-backslash` (`E502`) ([#10292](https://github.com/astral-sh/ruff/pull/10292))
- \[`pylint`\] - implement `redeclared-assigned-name` (`W0128`) ([#9268](https://github.com/astral-sh/ruff/pull/9268))

### Rule changes

- \[`flake8_comprehensions`\] Handled special case for `C400` which also matches `C416` ([#10419](https://github.com/astral-sh/ruff/pull/10419))
- \[`flake8-bandit`\] Implement upstream updates for `S311`, `S324` and `S605` ([#10313](https://github.com/astral-sh/ruff/pull/10313))
- \[`pyflakes`\] Remove `F401` fix for `__init__` imports by default and allow opt-in to unsafe fix ([#10365](https://github.com/astral-sh/ruff/pull/10365))
- \[`pylint`\] Implement `invalid-bool-return-type` (`E304`) ([#10377](https://github.com/astral-sh/ruff/pull/10377))
- \[`pylint`\] Include builtin warnings in useless-exception-statement (`PLW0133`) ([#10394](https://github.com/astral-sh/ruff/pull/10394))

### CLI

- Add message on success to `ruff check` ([#8631](https://github.com/astral-sh/ruff/pull/8631))

### Bug fixes

- \[`PIE970`\] Allow trailing ellipsis in `typing.TYPE_CHECKING` ([#10413](https://github.com/astral-sh/ruff/pull/10413))
- Avoid `TRIO115` if the argument is a variable ([#10376](https://github.com/astral-sh/ruff/pull/10376))
- \[`F811`\] Avoid removing shadowed imports that point to different symbols ([#10387](https://github.com/astral-sh/ruff/pull/10387))
- Fix `F821` and `F822` false positives in `.pyi` files ([#10341](https://github.com/astral-sh/ruff/pull/10341))
- Fix `F821` false negatives in `.py` files when `from __future__ import annotations` is active ([#10362](https://github.com/astral-sh/ruff/pull/10362))
- Fix case where `Indexer` fails to identify continuation preceded by newline #10351 ([#10354](https://github.com/astral-sh/ruff/pull/10354))
- Sort hash maps in `Settings` display ([#10370](https://github.com/astral-sh/ruff/pull/10370))
- Track conditional deletions in the semantic model ([#10415](https://github.com/astral-sh/ruff/pull/10415))
- \[`C413`\] Wrap expressions in parentheses when negating ([#10346](https://github.com/astral-sh/ruff/pull/10346))
- \[`pycodestyle`\] Do not ignore lines before the first logical line in blank lines rules. ([#10382](https://github.com/astral-sh/ruff/pull/10382))
- \[`pycodestyle`\] Do not trigger `E225` and `E275` when the next token is a ')' ([#10315](https://github.com/astral-sh/ruff/pull/10315))
- \[`pylint`\] Avoid false-positive slot non-assignment for `__dict__` (`PLE0237`) ([#10348](https://github.com/astral-sh/ruff/pull/10348))
- Gate f-string struct size test for Rustc < 1.76 ([#10371](https://github.com/astral-sh/ruff/pull/10371))

### Documentation

- Use `ruff.toml` format in README ([#10393](https://github.com/astral-sh/ruff/pull/10393))
- \[`RUF008`\] Make it clearer that a mutable default in a dataclass is only valid if it is typed as a ClassVar ([#10395](https://github.com/astral-sh/ruff/pull/10395))
- \[`pylint`\] Extend docs and test in `invalid-str-return-type` (`E307`) ([#10400](https://github.com/astral-sh/ruff/pull/10400))
- Remove `.` from `check` and `format` commands ([#10217](https://github.com/astral-sh/ruff/pull/10217))

## 0.3.2

### Preview features

- Improve single-`with` item formatting for Python 3.8 or older ([#10276](https://github.com/astral-sh/ruff/pull/10276))

### Rule changes

- \[`pyupgrade`\] Allow fixes for f-string rule regardless of line length (`UP032`) ([#10263](https://github.com/astral-sh/ruff/pull/10263))
- \[`pycodestyle`\] Include actual conditions in E712 diagnostics ([#10254](https://github.com/astral-sh/ruff/pull/10254))

### Bug fixes

- Fix trailing kwargs end of line comment after slash ([#10297](https://github.com/astral-sh/ruff/pull/10297))
- Fix unstable `with` items formatting ([#10274](https://github.com/astral-sh/ruff/pull/10274))
- Avoid repeating function calls in f-string conversions ([#10265](https://github.com/astral-sh/ruff/pull/10265))
- Fix E203 false positive for slices in format strings ([#10280](https://github.com/astral-sh/ruff/pull/10280))
- Fix incorrect `Parameter` range for `*args` and `**kwargs` ([#10283](https://github.com/astral-sh/ruff/pull/10283))
- Treat `typing.Annotated` subscripts as type definitions ([#10285](https://github.com/astral-sh/ruff/pull/10285))

## 0.3.1

### Preview features

- \[`pycodestyle`\] Fix E301 not triggering on decorated methods. ([#10117](https://github.com/astral-sh/ruff/pull/10117))
- \[`pycodestyle`\] Respect `isort` settings in blank line rules (`E3*`) ([#10096](https://github.com/astral-sh/ruff/pull/10096))
- \[`pycodestyle`\] Make blank lines in typing stub files optional (`E3*`) ([#10098](https://github.com/astral-sh/ruff/pull/10098))
- \[`pylint`\] Implement `singledispatch-method` (`E1519`) ([#10140](https://github.com/astral-sh/ruff/pull/10140))
- \[`pylint`\] Implement `useless-exception-statement` (`W0133`) ([#10176](https://github.com/astral-sh/ruff/pull/10176))

### Rule changes

- \[`flake8-debugger`\] Check for use of `debugpy` and `ptvsd` debug modules (#10177) ([#10194](https://github.com/astral-sh/ruff/pull/10194))
- \[`pyupgrade`\] Generate diagnostic for all valid f-string conversions regardless of line length (`UP032`) ([#10238](https://github.com/astral-sh/ruff/pull/10238))
- \[`pep8_naming`\] Add fixes for `N804` and `N805` ([#10215](https://github.com/astral-sh/ruff/pull/10215))

### CLI

- Colorize the output of `ruff format --diff` ([#10110](https://github.com/astral-sh/ruff/pull/10110))
- Make `--config` and `--isolated` global flags ([#10150](https://github.com/astral-sh/ruff/pull/10150))
- Correctly expand tildes and environment variables in paths passed to `--config` ([#10219](https://github.com/astral-sh/ruff/pull/10219))

### Configuration

- Accept a PEP 440 version specifier for `required-version` ([#10216](https://github.com/astral-sh/ruff/pull/10216))
- Implement isort's `default-section` setting ([#10149](https://github.com/astral-sh/ruff/pull/10149))

### Bug fixes

- Remove trailing space from `CapWords` message ([#10220](https://github.com/astral-sh/ruff/pull/10220))
- Respect external codes in file-level exemptions ([#10203](https://github.com/astral-sh/ruff/pull/10203))
- \[`flake8-raise`\] Avoid false-positives for parens-on-raise with `future.exception()` (`RSE102`) ([#10206](https://github.com/astral-sh/ruff/pull/10206))
- \[`pylint`\] Add fix for unary expressions in `PLC2801` ([#9587](https://github.com/astral-sh/ruff/pull/9587))
- \[`ruff`\] Fix RUF028 not allowing `# fmt: skip` on match cases ([#10178](https://github.com/astral-sh/ruff/pull/10178))

## 0.3.0

This release introduces the new Ruff formatter 2024.2 style and adds a new lint rule to
detect invalid formatter suppression comments.

### Preview features

- \[`flake8-bandit`\] Remove suspicious-lxml-import (`S410`) ([#10154](https://github.com/astral-sh/ruff/pull/10154))
- \[`pycodestyle`\] Allow `os.environ` modifications between imports (`E402`) ([#10066](https://github.com/astral-sh/ruff/pull/10066))
- \[`pycodestyle`\] Don't warn about a single whitespace character before a comma in a tuple (`E203`) ([#10094](https://github.com/astral-sh/ruff/pull/10094))

### Rule changes

- \[`eradicate`\] Detect commented out `case` statements (`ERA001`) ([#10055](https://github.com/astral-sh/ruff/pull/10055))
- \[`eradicate`\] Detect single-line code for `try:`, `except:`, etc. (`ERA001`) ([#10057](https://github.com/astral-sh/ruff/pull/10057))
- \[`flake8-boolean-trap`\] Allow boolean positionals in `__post_init__` ([#10027](https://github.com/astral-sh/ruff/pull/10027))
- \[`flake8-copyright`\] Allow © in copyright notices ([#10065](https://github.com/astral-sh/ruff/pull/10065))
- \[`isort`\]: Use one blank line after imports in typing stub files ([#9971](https://github.com/astral-sh/ruff/pull/9971))
- \[`pylint`\] New Rule `dict-iter-missing-items` (`PLE1141`) ([#9845](https://github.com/astral-sh/ruff/pull/9845))
- \[`pylint`\] Ignore `sys.version` and `sys.platform` (`PLR1714`) ([#10054](https://github.com/astral-sh/ruff/pull/10054))
- \[`pyupgrade`\] Detect literals with unary operators (`UP018`) ([#10060](https://github.com/astral-sh/ruff/pull/10060))
- \[`ruff`\] Expand rule for `list(iterable).pop(0)` idiom (`RUF015`) ([#10148](https://github.com/astral-sh/ruff/pull/10148))

### Formatter

This release introduces the Ruff 2024.2 style, stabilizing the following changes:

- Prefer splitting the assignment's value over the target or type annotation ([#8943](https://github.com/astral-sh/ruff/pull/8943))
- Remove blank lines before class docstrings ([#9154](https://github.com/astral-sh/ruff/pull/9154))
- Wrap multiple context managers in `with` parentheses when targeting Python 3.9 or newer ([#9222](https://github.com/astral-sh/ruff/pull/9222))
- Add a blank line after nested classes with a dummy body (`...`) in typing stub files ([#9155](https://github.com/astral-sh/ruff/pull/9155))
- Reduce vertical spacing for classes and functions with a dummy (`...`) body ([#7440](https://github.com/astral-sh/ruff/issues/7440), [#9240](https://github.com/astral-sh/ruff/pull/9240))
- Add a blank line after the module docstring ([#8283](https://github.com/astral-sh/ruff/pull/8283))
- Parenthesize long type hints in assignments ([#9210](https://github.com/astral-sh/ruff/pull/9210))
- Preserve indent for single multiline-string call-expressions ([#9673](https://github.com/astral-sh/ruff/pull/9637))
- Normalize hex escape and unicode escape sequences ([#9280](https://github.com/astral-sh/ruff/pull/9280))
- Format module docstrings ([#9725](https://github.com/astral-sh/ruff/pull/9725))

### CLI

- Explicitly disallow `extend` as part of a `--config` flag ([#10135](https://github.com/astral-sh/ruff/pull/10135))
- Remove `build` from the default exclusion list ([#10093](https://github.com/astral-sh/ruff/pull/10093))
- Deprecate `ruff <path>`, `ruff --explain`, `ruff --clean`, and `ruff --generate-shell-completion` in favor of `ruff check <path>`, `ruff rule`, `ruff clean`, and `ruff generate-shell-completion` ([#10169](https://github.com/astral-sh/ruff/pull/10169))
- Remove the deprecated CLI option `--format` from `ruff rule` and `ruff linter` ([#10170](https://github.com/astral-sh/ruff/pull/10170))

### Bug fixes

- \[`flake8-bugbear`\] Avoid adding default initializers to stubs (`B006`) ([#10152](https://github.com/astral-sh/ruff/pull/10152))
- \[`flake8-type-checking`\] Respect runtime-required decorators for function signatures ([#10091](https://github.com/astral-sh/ruff/pull/10091))
- \[`pycodestyle`\] Mark fixes overlapping with a multiline string as unsafe (`W293`) ([#10049](https://github.com/astral-sh/ruff/pull/10049))
- \[`pydocstyle`\] Trim whitespace when removing blank lines after section (`D413`) ([#10162](https://github.com/astral-sh/ruff/pull/10162))
- \[`pylint`\] Delete entire statement, including semicolons (`PLR0203`) ([#10074](https://github.com/astral-sh/ruff/pull/10074))
- \[`ruff`\] Avoid f-string false positives in `gettext` calls (`RUF027`) ([#10118](https://github.com/astral-sh/ruff/pull/10118))
- Fix `ruff` crashing on PowerPC systems because of too small page size ([#10080](https://github.com/astral-sh/ruff/pull/10080))

### Performance

- Add cold attribute to less likely printer queue branches in the formatter ([#10121](https://github.com/astral-sh/ruff/pull/10121))
- Skip unnecessary string normalization in the formatter ([#10116](https://github.com/astral-sh/ruff/pull/10116))

### Documentation

- Remove "Beta" Label from formatter documentation ([#10144](https://github.com/astral-sh/ruff/pull/10144))
- `line-length` option: fix link to `pycodestyle.max-line-length` ([#10136](https://github.com/astral-sh/ruff/pull/10136))

## 0.2.2

Highlights include:

- Initial support formatting f-strings (in `--preview`).
- Support for overriding arbitrary configuration options via the CLI through an expanded `--config` argument (e.g., `--config "lint.isort.combine-as-imports=false"`).
- Significant performance improvements in Ruff's lexer, parser, and lint rules.

### Preview features

- Implement minimal f-string formatting ([#9642](https://github.com/astral-sh/ruff/pull/9642))
- \[`pycodestyle`\] Add blank line(s) rules (`E301`, `E302`, `E303`, `E304`, `E305`, `E306`) ([#9266](https://github.com/astral-sh/ruff/pull/9266))
- \[`refurb`\] Implement `readlines_in_for` (`FURB129`) ([#9880](https://github.com/astral-sh/ruff/pull/9880))

### Rule changes

- \[`ruff`\] Ensure closing parentheses for multiline sequences are always on their own line (`RUF022`, `RUF023`) ([#9793](https://github.com/astral-sh/ruff/pull/9793))
- \[`numpy`\] Add missing deprecation violations (`NPY002`) ([#9862](https://github.com/astral-sh/ruff/pull/9862))
- \[`flake8-bandit`\] Detect `mark_safe` usages in decorators ([#9887](https://github.com/astral-sh/ruff/pull/9887))
- \[`ruff`\] Expand `asyncio-dangling-task` (`RUF006`) to include `new_event_loop` ([#9976](https://github.com/astral-sh/ruff/pull/9976))
- \[`flake8-pyi`\] Ignore 'unused' private type dicts in class scopes ([#9952](https://github.com/astral-sh/ruff/pull/9952))

### Formatter

- Docstring formatting: Preserve tab indentation when using `indent-style=tabs` ([#9915](https://github.com/astral-sh/ruff/pull/9915))
- Disable top-level docstring formatting for notebooks ([#9957](https://github.com/astral-sh/ruff/pull/9957))
- Stabilize quote-style's `preserve` mode ([#9922](https://github.com/astral-sh/ruff/pull/9922))

### CLI

- Allow arbitrary configuration options to be overridden via the CLI ([#9599](https://github.com/astral-sh/ruff/pull/9599))

### Bug fixes

- Make `show-settings` filters directory-agnostic ([#9866](https://github.com/astral-sh/ruff/pull/9866))
- Respect duplicates when rewriting type aliases ([#9905](https://github.com/astral-sh/ruff/pull/9905))
- Respect tuple assignments in typing analyzer ([#9969](https://github.com/astral-sh/ruff/pull/9969))
- Use atomic write when persisting cache ([#9981](https://github.com/astral-sh/ruff/pull/9981))
- Use non-parenthesized range for `DebugText` ([#9953](https://github.com/astral-sh/ruff/pull/9953))
- \[`flake8-simplify`\] Avoid false positive with `async` for loops (`SIM113`) ([#9996](https://github.com/astral-sh/ruff/pull/9996))
- \[`flake8-trio`\] Respect `async with` in `timeout-without-await` ([#9859](https://github.com/astral-sh/ruff/pull/9859))
- \[`perflint`\] Catch a wider range of mutations in `PERF101` ([#9955](https://github.com/astral-sh/ruff/pull/9955))
- \[`pycodestyle`\] Fix `E30X` panics on blank lines with trailing white spaces ([#9907](https://github.com/astral-sh/ruff/pull/9907))
- \[`pydocstyle`\] Allow using `parameters` as a subsection header (`D405`) ([#9894](https://github.com/astral-sh/ruff/pull/9894))
- \[`pydocstyle`\] Fix blank-line docstring rules for module-level docstrings ([#9878](https://github.com/astral-sh/ruff/pull/9878))
- \[`pylint`\] Accept 0.0 and 1.0 as common magic values (`PLR2004`) ([#9964](https://github.com/astral-sh/ruff/pull/9964))
- \[`pylint`\] Avoid suggesting set rewrites for non-hashable types ([#9956](https://github.com/astral-sh/ruff/pull/9956))
- \[`ruff`\] Avoid false negatives with string literals inside of method calls (`RUF027`) ([#9865](https://github.com/astral-sh/ruff/pull/9865))
- \[`ruff`\] Fix panic on with f-string detection (`RUF027`) ([#9990](https://github.com/astral-sh/ruff/pull/9990))
- \[`ruff`\] Ignore builtins when detecting missing f-strings ([#9849](https://github.com/astral-sh/ruff/pull/9849))

### Performance

- Use `memchr` for string lexing ([#9888](https://github.com/astral-sh/ruff/pull/9888))
- Use `memchr` for tab-indentation detection ([#9853](https://github.com/astral-sh/ruff/pull/9853))
- Reduce `Result<Tok, LexicalError>` size by using `Box<str>` instead of `String` ([#9885](https://github.com/astral-sh/ruff/pull/9885))
- Reduce size of `Expr` from 80 to 64 bytes ([#9900](https://github.com/astral-sh/ruff/pull/9900))
- Improve trailing comma rule performance ([#9867](https://github.com/astral-sh/ruff/pull/9867))
- Remove unnecessary string cloning from the parser ([#9884](https://github.com/astral-sh/ruff/pull/9884))

## 0.2.1

This release includes support for range formatting (i.e., the ability to format specific lines
within a source file).

### Preview features

- \[`refurb`\] Implement `missing-f-string-syntax` (`RUF027`) ([#9728](https://github.com/astral-sh/ruff/pull/9728))
- Format module-level docstrings ([#9725](https://github.com/astral-sh/ruff/pull/9725))

### Formatter

- Add `--range` option to `ruff format` ([#9733](https://github.com/astral-sh/ruff/pull/9733))
- Don't trim last empty line in docstrings ([#9813](https://github.com/astral-sh/ruff/pull/9813))

### Bug fixes

- Skip empty lines when determining base indentation ([#9795](https://github.com/astral-sh/ruff/pull/9795))
- Drop `__get__` and `__set__` from `unnecessary-dunder-call` ([#9791](https://github.com/astral-sh/ruff/pull/9791))
- Respect generic `Protocol` in ellipsis removal ([#9841](https://github.com/astral-sh/ruff/pull/9841))
- Revert "Use publicly available Apple Silicon runners (#9726)" ([#9834](https://github.com/astral-sh/ruff/pull/9834))

### Performance

- Skip LibCST parsing for standard dedent adjustments ([#9769](https://github.com/astral-sh/ruff/pull/9769))
- Remove CST-based fixer for `C408` ([#9822](https://github.com/astral-sh/ruff/pull/9822))
- Add our own ignored-names abstractions ([#9802](https://github.com/astral-sh/ruff/pull/9802))
- Remove CST-based fixers for `C400`, `C401`, `C410`, and `C418` ([#9819](https://github.com/astral-sh/ruff/pull/9819))
- Use `AhoCorasick` to speed up quote match ([#9773](https://github.com/astral-sh/ruff/pull/9773))
- Remove CST-based fixers for `C405` and `C409` ([#9821](https://github.com/astral-sh/ruff/pull/9821))
- Add fast-path for comment detection ([#9808](https://github.com/astral-sh/ruff/pull/9808))
- Invert order of checks in `zero-sleep-call` ([#9766](https://github.com/astral-sh/ruff/pull/9766))
- Short-circuit typing matches based on imports ([#9800](https://github.com/astral-sh/ruff/pull/9800))
- Run dunder method rule on methods directly ([#9815](https://github.com/astral-sh/ruff/pull/9815))
- Track top-level module imports in the semantic model ([#9775](https://github.com/astral-sh/ruff/pull/9775))
- Slight speed-up for lowercase and uppercase identifier checks ([#9798](https://github.com/astral-sh/ruff/pull/9798))
- Remove LibCST-based fixer for `C403` ([#9818](https://github.com/astral-sh/ruff/pull/9818))

### Documentation

- Update `max-pos-args` example to `max-positional-args` ([#9797](https://github.com/astral-sh/ruff/pull/9797))
- Fixed example code in `weak_cryptographic_key.rs` ([#9774](https://github.com/astral-sh/ruff/pull/9774))
- Fix references to deprecated `ANN` rules in changelog ([#9771](https://github.com/astral-sh/ruff/pull/9771))
- Fix default for `max-positional-args` ([#9838](https://github.com/astral-sh/ruff/pull/9838))

## 0.2.0

### Breaking changes

- The `NURSERY` selector cannot be used anymore
- Legacy selection of nursery rules by exact codes is no longer allowed without preview enabled

See also, the "Remapped rules" section which may result in disabled rules.

### Deprecations

The following rules are now deprecated:

- [`missing-type-self`](https://docs.astral.sh/ruff/rules/missing-type-self/) (`ANN101`)
- [`missing-type-cls`](https://docs.astral.sh/ruff/rules/missing-type-cls/) (`ANN102`)

The following command line options are now deprecated:

- `--show-source`; use `--output-format full` instead
- `--no-show-source`; use `--output-format concise` instead
- `--output-format text`; use `full` or `concise` instead

The following settings have moved and the previous name is deprecated:

- `ruff.allowed-confusables` → [`ruff.lint.allowed-confusables`](https://docs.astral.sh//ruff/settings/#lint_allowed-confusables)
- `ruff.dummy-variable-rgx` → [`ruff.lint.dummy-variable-rgx`](https://docs.astral.sh//ruff/settings/#lint_dummy-variable-rgx)
- `ruff.explicit-preview-rules` → [`ruff.lint.explicit-preview-rules`](https://docs.astral.sh//ruff/settings/#lint_explicit-preview-rules)
- `ruff.extend-fixable` → [`ruff.lint.extend-fixable`](https://docs.astral.sh//ruff/settings/#lint_extend-fixable)
- `ruff.extend-ignore` → [`ruff.lint.extend-ignore`](https://docs.astral.sh//ruff/settings/#lint_extend-ignore)
- `ruff.extend-per-file-ignores` → [`ruff.lint.extend-per-file-ignores`](https://docs.astral.sh//ruff/settings/#lint_extend-per-file-ignores)
- `ruff.extend-safe-fixes` → [`ruff.lint.extend-safe-fixes`](https://docs.astral.sh//ruff/settings/#lint_extend-safe-fixes)
- `ruff.extend-select` → [`ruff.lint.extend-select`](https://docs.astral.sh//ruff/settings/#lint_extend-select)
- `ruff.extend-unfixable` → [`ruff.lint.extend-unfixable`](https://docs.astral.sh//ruff/settings/#lint_extend-unfixable)
- `ruff.extend-unsafe-fixes` → [`ruff.lint.extend-unsafe-fixes`](https://docs.astral.sh//ruff/settings/#lint_extend-unsafe-fixes)
- `ruff.external` → [`ruff.lint.external`](https://docs.astral.sh//ruff/settings/#lint_external)
- `ruff.fixable` → [`ruff.lint.fixable`](https://docs.astral.sh//ruff/settings/#lint_fixable)
- `ruff.flake8-annotations` → [`ruff.lint.flake8-annotations`](https://docs.astral.sh//ruff/settings/#lint_flake8-annotations)
- `ruff.flake8-bandit` → [`ruff.lint.flake8-bandit`](https://docs.astral.sh//ruff/settings/#lint_flake8-bandit)
- `ruff.flake8-bugbear` → [`ruff.lint.flake8-bugbear`](https://docs.astral.sh//ruff/settings/#lint_flake8-bugbear)
- `ruff.flake8-builtins` → [`ruff.lint.flake8-builtins`](https://docs.astral.sh//ruff/settings/#lint_flake8-builtins)
- `ruff.flake8-comprehensions` → [`ruff.lint.flake8-comprehensions`](https://docs.astral.sh//ruff/settings/#lint_flake8-comprehensions)
- `ruff.flake8-copyright` → [`ruff.lint.flake8-copyright`](https://docs.astral.sh//ruff/settings/#lint_flake8-copyright)
- `ruff.flake8-errmsg` → [`ruff.lint.flake8-errmsg`](https://docs.astral.sh//ruff/settings/#lint_flake8-errmsg)
- `ruff.flake8-gettext` → [`ruff.lint.flake8-gettext`](https://docs.astral.sh//ruff/settings/#lint_flake8-gettext)
- `ruff.flake8-implicit-str-concat` → [`ruff.lint.flake8-implicit-str-concat`](https://docs.astral.sh//ruff/settings/#lint_flake8-implicit-str-concat)
- `ruff.flake8-import-conventions` → [`ruff.lint.flake8-import-conventions`](https://docs.astral.sh//ruff/settings/#lint_flake8-import-conventions)
- `ruff.flake8-pytest-style` → [`ruff.lint.flake8-pytest-style`](https://docs.astral.sh//ruff/settings/#lint_flake8-pytest-style)
- `ruff.flake8-quotes` → [`ruff.lint.flake8-quotes`](https://docs.astral.sh//ruff/settings/#lint_flake8-quotes)
- `ruff.flake8-self` → [`ruff.lint.flake8-self`](https://docs.astral.sh//ruff/settings/#lint_flake8-self)
- `ruff.flake8-tidy-imports` → [`ruff.lint.flake8-tidy-imports`](https://docs.astral.sh//ruff/settings/#lint_flake8-tidy-imports)
- `ruff.flake8-type-checking` → [`ruff.lint.flake8-type-checking`](https://docs.astral.sh//ruff/settings/#lint_flake8-type-checking)
- `ruff.flake8-unused-arguments` → [`ruff.lint.flake8-unused-arguments`](https://docs.astral.sh//ruff/settings/#lint_flake8-unused-arguments)
- `ruff.ignore` → [`ruff.lint.ignore`](https://docs.astral.sh//ruff/settings/#lint_ignore)
- `ruff.ignore-init-module-imports` → [`ruff.lint.ignore-init-module-imports`](https://docs.astral.sh//ruff/settings/#lint_ignore-init-module-imports)
- `ruff.isort` → [`ruff.lint.isort`](https://docs.astral.sh//ruff/settings/#lint_isort)
- `ruff.logger-objects` → [`ruff.lint.logger-objects`](https://docs.astral.sh//ruff/settings/#lint_logger-objects)
- `ruff.mccabe` → [`ruff.lint.mccabe`](https://docs.astral.sh//ruff/settings/#lint_mccabe)
- `ruff.pep8-naming` → [`ruff.lint.pep8-naming`](https://docs.astral.sh//ruff/settings/#lint_pep8-naming)
- `ruff.per-file-ignores` → [`ruff.lint.per-file-ignores`](https://docs.astral.sh//ruff/settings/#lint_per-file-ignores)
- `ruff.pycodestyle` → [`ruff.lint.pycodestyle`](https://docs.astral.sh//ruff/settings/#lint_pycodestyle)
- `ruff.pydocstyle` → [`ruff.lint.pydocstyle`](https://docs.astral.sh//ruff/settings/#lint_pydocstyle)
- `ruff.pyflakes` → [`ruff.lint.pyflakes`](https://docs.astral.sh//ruff/settings/#lint_pyflakes)
- `ruff.pylint` → [`ruff.lint.pylint`](https://docs.astral.sh//ruff/settings/#lint_pylint)
- `ruff.pyupgrade` → [`ruff.lint.pyupgrade`](https://docs.astral.sh//ruff/settings/#lint_pyupgrade)
- `ruff.select` → [`ruff.lint.select`](https://docs.astral.sh//ruff/settings/#lint_select)
- `ruff.task-tags` → [`ruff.lint.task-tags`](https://docs.astral.sh//ruff/settings/#lint_task-tags)
- `ruff.typing-modules` → [`ruff.lint.typing-modules`](https://docs.astral.sh//ruff/settings/#lint_typing-modules)
- `ruff.unfixable` → [`ruff.lint.unfixable`](https://docs.astral.sh//ruff/settings/#lint_unfixable)

### Remapped rules

The following rules have been remapped to new codes:

- [`raise-without-from-inside-except`](https://docs.astral.sh/ruff/rules/raise-without-from-inside-except/): `TRY200` to `B904`
- [`suspicious-eval-usage`](https://docs.astral.sh/ruff/rules/suspicious-eval-usage/): `PGH001` to `S307`
- [`logging-warn`](https://docs.astral.sh/ruff/rules/logging-warn/): `PGH002` to `G010`
- [`static-key-dict-comprehension`](https://docs.astral.sh/ruff/rules/static-key-dict-comprehension): `RUF011` to `B035`
- [`runtime-string-union`](https://docs.astral.sh/ruff/rules/runtime-string-union): `TCH006` to `TCH010`

### Stabilizations

The following rules have been stabilized and are no longer in preview:

- [`trio-timeout-without-await`](https://docs.astral.sh/ruff/rules/trio-timeout-without-await) (`TRIO100`)
- [`trio-sync-call`](https://docs.astral.sh/ruff/rules/trio-sync-call) (`TRIO105`)
- [`trio-async-function-with-timeout`](https://docs.astral.sh/ruff/rules/trio-async-function-with-timeout) (`TRIO109`)
- [`trio-unneeded-sleep`](https://docs.astral.sh/ruff/rules/trio-unneeded-sleep) (`TRIO110`)
- [`trio-zero-sleep-call`](https://docs.astral.sh/ruff/rules/trio-zero-sleep-call) (`TRIO115`)
- [`unnecessary-escaped-quote`](https://docs.astral.sh/ruff/rules/unnecessary-escaped-quote) (`Q004`)
- [`enumerate-for-loop`](https://docs.astral.sh/ruff/rules/enumerate-for-loop) (`SIM113`)
- [`zip-dict-keys-and-values`](https://docs.astral.sh/ruff/rules/zip-dict-keys-and-values) (`SIM911`)
- [`timeout-error-alias`](https://docs.astral.sh/ruff/rules/timeout-error-alias) (`UP041`)
- [`flask-debug-true`](https://docs.astral.sh/ruff/rules/flask-debug-true) (`S201`)
- [`tarfile-unsafe-members`](https://docs.astral.sh/ruff/rules/tarfile-unsafe-members) (`S202`)
- [`ssl-insecure-version`](https://docs.astral.sh/ruff/rules/ssl-insecure-version) (`S502`)
- [`ssl-with-bad-defaults`](https://docs.astral.sh/ruff/rules/ssl-with-bad-defaults) (`S503`)
- [`ssl-with-no-version`](https://docs.astral.sh/ruff/rules/ssl-with-no-version) (`S504`)
- [`weak-cryptographic-key`](https://docs.astral.sh/ruff/rules/weak-cryptographic-key) (`S505`)
- [`ssh-no-host-key-verification`](https://docs.astral.sh/ruff/rules/ssh-no-host-key-verification) (`S507`)
- [`django-raw-sql`](https://docs.astral.sh/ruff/rules/django-raw-sql) (`S611`)
- [`mako-templates`](https://docs.astral.sh/ruff/rules/mako-templates) (`S702`)
- [`generator-return-from-iter-method`](https://docs.astral.sh/ruff/rules/generator-return-from-iter-method) (`PYI058`)
- [`runtime-string-union`](https://docs.astral.sh/ruff/rules/runtime-string-union) (`TCH006`)
- [`numpy2-deprecation`](https://docs.astral.sh/ruff/rules/numpy2-deprecation) (`NPY201`)
- [`quadratic-list-summation`](https://docs.astral.sh/ruff/rules/quadratic-list-summation) (`RUF017`)
- [`assignment-in-assert`](https://docs.astral.sh/ruff/rules/assignment-in-assert) (`RUF018`)
- [`unnecessary-key-check`](https://docs.astral.sh/ruff/rules/unnecessary-key-check) (`RUF019`)
- [`never-union`](https://docs.astral.sh/ruff/rules/never-union) (`RUF020`)
- [`direct-logger-instantiation`](https://docs.astral.sh/ruff/rules/direct-logger-instantiation) (`LOG001`)
- [`invalid-get-logger-argument`](https://docs.astral.sh/ruff/rules/invalid-get-logger-argument) (`LOG002`)
- [`exception-without-exc-info`](https://docs.astral.sh/ruff/rules/exception-without-exc-info) (`LOG007`)
- [`undocumented-warn`](https://docs.astral.sh/ruff/rules/undocumented-warn) (`LOG009`)

Fixes for the following rules have been stabilized and are now available without preview:

- [`triple-single-quotes`](https://docs.astral.sh/ruff/rules/triple-single-quotes) (`D300`)
- [`non-pep604-annotation`](https://docs.astral.sh/ruff/rules/non-pep604-annotation) (`UP007`)
- [`dict-get-with-none-default`](https://docs.astral.sh/ruff/rules/dict-get-with-none-default) (`SIM910`)
- [`in-dict-keys`](https://docs.astral.sh/ruff/rules/in-dict-keys) (`SIM118`)
- [`collapsible-else-if`](https://docs.astral.sh/ruff/rules/collapsible-else-if) (`PLR5501`)
- [`if-with-same-arms`](https://docs.astral.sh/ruff/rules/if-with-same-arms) (`SIM114`)
- [`useless-else-on-loop`](https://docs.astral.sh/ruff/rules/useless-else-on-loop) (`PLW0120`)
- [`unnecessary-literal-union`](https://docs.astral.sh/ruff/rules/unnecessary-literal-union) (`PYI030`)
- [`unnecessary-spread`](https://docs.astral.sh/ruff/rules/unnecessary-spread) (`PIE800`)
- [`error-instead-of-exception`](https://docs.astral.sh/ruff/rules/error-instead-of-exception) (`TRY400`)
- [`redefined-while-unused`](https://docs.astral.sh/ruff/rules/redefined-while-unused) (`F811`)
- [`duplicate-value`](https://docs.astral.sh/ruff/rules/duplicate-value) (`B033`)
- [`multiple-imports-on-one-line`](https://docs.astral.sh/ruff/rules/multiple-imports-on-one-line) (`E401`)
- [`non-pep585-annotation`](https://docs.astral.sh/ruff/rules/non-pep585-annotation) (`UP006`)

Fixes for the following rules have been promoted from unsafe to safe:

- [`unaliased-collections-abc-set-import`](https://docs.astral.sh/ruff/rules/unaliased-collections-abc-set-import) (`PYI025`)

The following behaviors have been stabilized:

- [`module-import-not-at-top-of-file`](https://docs.astral.sh/ruff/rules/module-import-not-at-top-of-file/) (`E402`) allows `sys.path` modifications between imports
- [`reimplemented-container-builtin`](https://docs.astral.sh/ruff/rules/reimplemented-container-builtin/) (`PIE807`) includes lambdas that can be replaced with `dict`
- [`unnecessary-placeholder`](https://docs.astral.sh/ruff/rules/unnecessary-placeholder/) (`PIE790`) applies to unnecessary ellipses (`...`)
- [`if-else-block-instead-of-dict-get`](https://docs.astral.sh/ruff/rules/if-else-block-instead-of-dict-get/) (`SIM401`) applies to `if-else` expressions

### Preview features

- \[`refurb`\] Implement `metaclass_abcmeta` (`FURB180`) ([#9658](https://github.com/astral-sh/ruff/pull/9658))
- Implement `blank_line_after_nested_stub_class` preview style ([#9155](https://github.com/astral-sh/ruff/pull/9155))
- The preview rule [`and-or-ternary`](https://docs.astral.sh/ruff/rules/and-or-ternary) (`PLR1706`) was removed

### Bug fixes

- \[`flake8-async`\] Take `pathlib.Path` into account when analyzing async functions ([#9703](https://github.com/astral-sh/ruff/pull/9703))
- \[`flake8-return`\] - fix indentation syntax error (`RET505`) ([#9705](https://github.com/astral-sh/ruff/pull/9705))
- Detect multi-statement lines in else removal ([#9748](https://github.com/astral-sh/ruff/pull/9748))
- `RUF022`, `RUF023`: never add two trailing commas to the end of a sequence ([#9698](https://github.com/astral-sh/ruff/pull/9698))
- `RUF023`: Don't sort `__match_args__`, only `__slots__` ([#9724](https://github.com/astral-sh/ruff/pull/9724))
- \[`flake8-simplify`\] - Fix syntax error in autofix (`SIM114`) ([#9704](https://github.com/astral-sh/ruff/pull/9704))
- \[`pylint`\] Show verbatim constant in `magic-value-comparison` (`PLR2004`) ([#9694](https://github.com/astral-sh/ruff/pull/9694))
- Removing trailing whitespace inside multiline strings is unsafe ([#9744](https://github.com/astral-sh/ruff/pull/9744))
- Support `IfExp` with dual string arms in `invalid-envvar-default` ([#9734](https://github.com/astral-sh/ruff/pull/9734))
- \[`pylint`\] Add `__mro_entries__` to known dunder methods (`PLW3201`) ([#9706](https://github.com/astral-sh/ruff/pull/9706))

### Documentation

- Removed rules are now retained in the documentation ([#9691](https://github.com/astral-sh/ruff/pull/9691))
- Deprecated rules are now indicated in the documentation ([#9689](https://github.com/astral-sh/ruff/pull/9689))

## 0.1.15

### Preview features

- Error when `NURSERY` selector is used with `--preview` ([#9682](https://github.com/astral-sh/ruff/pull/9682))
- Preserve indentation around multiline strings in formatter ([#9637](https://github.com/astral-sh/ruff/pull/9637))
- \[`flake8-return`\] Add fixes for all rules (`RET505`, `RET506`, `RET507`, `RET508`) ([#9595](https://github.com/astral-sh/ruff/pull/9595))
- \[`flake8-simplify`\] Add fix for `if-with-same-arms` (`SIM114`) ([#9591](https://github.com/astral-sh/ruff/pull/9591))
- \[`pycodestyle`\] Add fix for `multiple-imports-on-one-line` (`E401`) ([#9518](https://github.com/astral-sh/ruff/pull/9518))
- \[`pylint`\] Add fix for `collapsible-else-if` (`PLR5501`) ([#9594](https://github.com/astral-sh/ruff/pull/9594))
- \[`pylint`\] Add fix for `useless-else-on-loop` (`PLW0120`) ([#9590](https://github.com/astral-sh/ruff/pull/9590))
- \[`pylint`\] Implement `assigning-non-slot` (`E0237`) ([#9623](https://github.com/astral-sh/ruff/pull/9623))
- \[`pylint`\] Implement `potential-index-error` (`PLE0643`) ([#9545](https://github.com/astral-sh/ruff/pull/9545))
- \[`pylint`\] Implement `too-many-nested-blocks` (`PLR1702`) ([#9172](https://github.com/astral-sh/ruff/pull/9172))
- \[`ruff`\] Add rule to sort `__slots__` and `__match_args__` ([#9564](https://github.com/astral-sh/ruff/pull/9564))
- \[`ruff`\] Detect unnecessary `dict` comprehensions for iterables (`RUF025`) ([#9613](https://github.com/astral-sh/ruff/pull/9613))
- \[`ruff`\] Guard against use of `default_factory` as a keyword argument (`RUF026`) ([#9651](https://github.com/astral-sh/ruff/pull/9651))
- \[`ruff`\] Implement `mutable-fromkeys-value` (`RUF024`) ([#9597](https://github.com/astral-sh/ruff/pull/9597))

### CLI

- Enable auto-wrapping of `--help` output ([#9633](https://github.com/astral-sh/ruff/pull/9633))

### Bug fixes

- Avoid rendering display-only rules as fixable ([#9649](https://github.com/astral-sh/ruff/pull/9649))
- Detect automagic-like assignments in notebooks ([#9653](https://github.com/astral-sh/ruff/pull/9653))
- Generate custom JSON schema for dynamic setting ([#9632](https://github.com/astral-sh/ruff/pull/9632))
- \[`flake8-no-pep420`\] Include global `--config` when determining namespace packages ([#9603](https://github.com/astral-sh/ruff/pull/9603))
- \[`flake8-pie`\] Omit bound tuples passed to `.startswith` or `.endswith` ([#9661](https://github.com/astral-sh/ruff/pull/9661))
- \[`flake8-return`\] Avoid panic when fixing inlined else blocks ([#9657](https://github.com/astral-sh/ruff/pull/9657))
- \[`flake8-return`\] Consider exception suppression in unnecessary assignment ([#9673](https://github.com/astral-sh/ruff/pull/9673))
- \[`flake8-return`\] Take `NoReturn` annotation into account when analyzing implicit returns ([#9636](https://github.com/astral-sh/ruff/pull/9636))
- \[`flake8-simplify`\] Support inverted returns in `needless-bool` (`SIM103`) ([#9619](https://github.com/astral-sh/ruff/pull/9619))
- \[`flake8-type-checking`\] Add Pydantic's `BaseConfig` to default-copy list ([#9650](https://github.com/astral-sh/ruff/pull/9650))
- \[`flake8-type-checking`\] Avoid marking `InitVar` as a typing-only annotation ([#9688](https://github.com/astral-sh/ruff/pull/9688))
- \[`pycodestyle`\] Allow `dtype` comparisons in `type-comparison` ([#9676](https://github.com/astral-sh/ruff/pull/9676))
- \[`pydocstyle`\] Re-implement `last-line-after-section` (`D413`) ([#9654](https://github.com/astral-sh/ruff/pull/9654))

### Documentation

- \[`flake8-pytest-style`\] Add fix safety documentation for `duplicate-parameterize-test-cases` ([#9678](https://github.com/astral-sh/ruff/pull/9678))
- \[`pylint`\] Document `literal-membership` fix safety conditions ([#9677](https://github.com/astral-sh/ruff/pull/9677))
- \[`isort`\] Fix reference to `isort` rule code ([#9598](https://github.com/astral-sh/ruff/pull/9598))

## 0.1.14

### Preview features

- \[`flake8-bugbear`\] Add fix for `duplicate-value` (`B033`) ([#9510](https://github.com/astral-sh/ruff/pull/9510))
- \[`flake8-simplify`\] Implement `enumerate-for-loop` (`SIM113`) ([#7777](https://github.com/astral-sh/ruff/pull/7777))
- \[`pygrep_hooks`\] Add fix for `deprecated-log-warn` (`PGH002`) ([#9519](https://github.com/astral-sh/ruff/pull/9519))
- \[`pylint`\] Implement `import-private-name` (`C2701`) ([#5920](https://github.com/astral-sh/ruff/pull/5920))
- \[`refurb`\] Implement `regex-flag-alias` with fix (`FURB167`) ([#9516](https://github.com/astral-sh/ruff/pull/9516))
- \[`ruff`\] Add rule and fix to sort contents of `__all__` (`RUF022`) ([#9474](https://github.com/astral-sh/ruff/pull/9474))
- \[`tryceratops`\] Add fix for `error-instead-of-exception` (`TRY400`) ([#9520](https://github.com/astral-sh/ruff/pull/9520))

### Rule changes

- \[`flake8-pyi`\] Fix `PYI047` false negatives on PEP-695 type aliases ([#9566](https://github.com/astral-sh/ruff/pull/9566))
- \[`flake8-pyi`\] Fix `PYI049` false negatives on call-based `TypedDict`s ([#9567](https://github.com/astral-sh/ruff/pull/9567))
- \[`pylint`\] Exclude `self` and `cls` when counting method arguments (`PLR0917`) ([#9563](https://github.com/astral-sh/ruff/pull/9563))

### CLI

- `--show-settings` displays active settings in a far more readable format ([#9464](https://github.com/astral-sh/ruff/pull/9464))
- Add `--extension` support to the formatter ([#9483](https://github.com/astral-sh/ruff/pull/9483))

### Configuration

- Ignore preview status for fixable and unfixable selectors ([#9538](https://github.com/astral-sh/ruff/pull/9538))
- \[`pycodestyle`\] Use the configured tab size when expanding indents ([#9506](https://github.com/astral-sh/ruff/pull/9506))

### Bug fixes

- Recursively visit deferred AST nodes ([#9541](https://github.com/astral-sh/ruff/pull/9541))
- Visit deferred lambdas before type definitions ([#9540](https://github.com/astral-sh/ruff/pull/9540))
- \[`flake8-simplify`\] Avoid some more `enumerate-for-loop` false positives (`SIM113`) ([#9515](https://github.com/astral-sh/ruff/pull/9515))
- \[`pandas-vet`\] Limit inplace diagnostics to methods that accept inplace ([#9495](https://github.com/astral-sh/ruff/pull/9495))
- \[`pylint`\] Add the `__prepare__` method to the list of recognized dunder method ([#9529](https://github.com/astral-sh/ruff/pull/9529))
- \[`pylint`\] Ignore unnecessary dunder calls within dunder definitions ([#9496](https://github.com/astral-sh/ruff/pull/9496))
- \[`refurb`\] Avoid bailing when `reimplemented-operator` is called on function (`FURB118`) ([#9556](https://github.com/astral-sh/ruff/pull/9556))
- \[`ruff`\] Avoid treating named expressions as static keys (`RUF011`) ([#9494](https://github.com/astral-sh/ruff/pull/9494))

### Documentation

- Add instructions on using `noqa` with isort rules ([#9555](https://github.com/astral-sh/ruff/pull/9555))
- Documentation update for URL giving 'page not found' ([#9565](https://github.com/astral-sh/ruff/pull/9565))
- Fix admonition in dark mode ([#9502](https://github.com/astral-sh/ruff/pull/9502))
- Update contributing docs to use `cargo bench -p ruff_benchmark` ([#9535](https://github.com/astral-sh/ruff/pull/9535))
- Update emacs integration section to include `emacs-ruff-format` ([#9403](https://github.com/astral-sh/ruff/pull/9403))
- \[`flake8-blind-except`\] Document exceptions to `blind-except` rule ([#9580](https://github.com/astral-sh/ruff/pull/9580))

## 0.1.13

### Bug fixes

- Include base pyproject when initializing cache settings ([#9480](https://github.com/astral-sh/ruff/pull/9480))
- \[`flake8-simplify`\] Account for possibly-empty f-string values in truthiness logic ([#9484](https://github.com/astral-sh/ruff/pull/9484))
- \[`pylint`\] Add the missing period in `unnecessary-dunder-call` ([#9485](https://github.com/astral-sh/ruff/pull/9485))
- \[`pylint`\] Fix `__aenter__` message in `unnecessary-dunder-call` ([#9492](https://github.com/astral-sh/ruff/pull/9492))

## 0.1.12

### Preview features

- Formatter: Hug multiline-strings in preview style ([#9243](https://github.com/astral-sh/ruff/pull/9243))
- \[`flake8-bandit`\] Add `ssl-with-no-version` (`S504`) ([#9384](https://github.com/astral-sh/ruff/pull/9384))
- \[`flake8-bandit`\] Implement `ssl-insecure-version` (`S502`) ([#9390](https://github.com/astral-sh/ruff/pull/9390))
- \[`flake8-bandit`\] Implement `ssl-with-bad-defaults` (`S503`) ([#9391](https://github.com/astral-sh/ruff/pull/9391))
- \[`flake8-bandit`\] Implement suspicious import rules (`S4XX`) ([#8831](https://github.com/astral-sh/ruff/pull/8831))
- \[`flake8-simplify`\] Implement `zip-dict-keys-and-values` (`SIM911`) ([#9460](https://github.com/astral-sh/ruff/pull/9460))
- \[`pyflakes`\] Add a fix for `redefined-while-unused` (`F811`) ([#9419](https://github.com/astral-sh/ruff/pull/9419))
- \[`pylint`\] Implement `unnecessary-dunder-call` (`C2801`) ([#9166](https://github.com/astral-sh/ruff/pull/9166))
- \[`ruff`\] Add `parenthesize-chained-operators` (`RUF021`) to enforce parentheses in `a or b and c` ([#9440](https://github.com/astral-sh/ruff/pull/9440))

### Rule changes

- \[`flake8-boolean-trap`\] Allow Boolean positional arguments in setters ([#9429](https://github.com/astral-sh/ruff/pull/9429))
- \[`flake8-builtins`\] Restrict `builtin-attribute-shadowing` (`A003`) to actual shadowed references ([#9462](https://github.com/astral-sh/ruff/pull/9462))
- \[`flake8-pyi`\] Add fix for `generator-return-from-iter-method` (`PYI058`) ([#9355](https://github.com/astral-sh/ruff/pull/9355))
- \[`pyflakes`\] Don't flag `redefined-while-unused` (`F811`) in `if` branches ([#9418](https://github.com/astral-sh/ruff/pull/9418))
- \[`pyupgrade`\] Add some additional Python 3.12 typing members to `deprecated-import` ([#9445](https://github.com/astral-sh/ruff/pull/9445))
- \[`ruff`\] Add fix for `parenthesize-chained-operators` (`RUF021`) ([#9449](https://github.com/astral-sh/ruff/pull/9449))
- \[`ruff`\] Include subscripts and attributes in static key rule (`RUF011`) ([#9416](https://github.com/astral-sh/ruff/pull/9416))
- \[`ruff`\] Support variable keys in static dictionary key rule (`RUF011`) ([#9411](https://github.com/astral-sh/ruff/pull/9411))

### Formatter

- Generate deterministic IDs when formatting notebooks ([#9359](https://github.com/astral-sh/ruff/pull/9359))
- Allow `# fmt: skip` with interspersed same-line comments ([#9395](https://github.com/astral-sh/ruff/pull/9395))
- Parenthesize breaking named expressions in match guards ([#9396](https://github.com/astral-sh/ruff/pull/9396))

### Bug fixes

- Add cell indexes to all diagnostics ([#9387](https://github.com/astral-sh/ruff/pull/9387))
- Avoid infinite loop in constant vs. `None` comparisons ([#9376](https://github.com/astral-sh/ruff/pull/9376))
- Handle raises with implicit alternate branches ([#9377](https://github.com/astral-sh/ruff/pull/9377))
- Ignore trailing quotes for unclosed l-brace errors ([#9388](https://github.com/astral-sh/ruff/pull/9388))
- Respect multi-segment submodule imports when resolving qualified names ([#9382](https://github.com/astral-sh/ruff/pull/9382))
- Use `DisplayParseError` for stdin parser errors ([#9409](https://github.com/astral-sh/ruff/pull/9409))
- Use `comment_ranges` for isort directive extraction ([#9414](https://github.com/astral-sh/ruff/pull/9414))
- Use transformed source code for diagnostic locations ([#9408](https://github.com/astral-sh/ruff/pull/9408))
- \[`flake8-pyi`\] Exclude `warnings.deprecated` and `typing_extensions.deprecated` arguments ([#9423](https://github.com/astral-sh/ruff/pull/9423))
- \[`flake8-pyi`\] Fix false negative for `unused-private-protocol` (`PYI046`) with unused generic protocols ([#9405](https://github.com/astral-sh/ruff/pull/9405))
- \[`pydocstyle`\] Disambiguate argument descriptors from section headers ([#9427](https://github.com/astral-sh/ruff/pull/9427))
- \[`pylint`\] Homogenize `PLR0914` message to match other `PLR09XX` rules ([#9399](https://github.com/astral-sh/ruff/pull/9399))
- \[`ruff`\] Allow `Hashable = None` in type annotations (`RUF013`) ([#9442](https://github.com/astral-sh/ruff/pull/9442))

### Documentation

- Fix admonition hyperlink colouring ([#9385](https://github.com/astral-sh/ruff/pull/9385))
- Add missing preview link ([#9386](https://github.com/astral-sh/ruff/pull/9386))

## 0.1.11

### Preview features

- \[`pylint`\] Implement `super-without-brackets` (`W0245`) ([#9257](https://github.com/astral-sh/ruff/pull/9257))

### Bug fixes

- Check path string properly in `python -m ruff` invocations ([#9367](https://github.com/astral-sh/ruff/pull/9367))

### Documentation

- Tweak `relative-imports` message ([#9365](https://github.com/astral-sh/ruff/pull/9365))
- Add fix safety note for `yield-in-for-loop` ([#9364](https://github.com/astral-sh/ruff/pull/9364))

## 0.1.10

### Preview features

- Improve `dummy_implementations` preview style formatting ([#9240](https://github.com/astral-sh/ruff/pull/9240))
- Normalise Hex and unicode escape sequences in strings ([#9280](https://github.com/astral-sh/ruff/pull/9280))
- Parenthesize long type annotations in annotated assignments ([#9210](https://github.com/astral-sh/ruff/pull/9210))
- Parenthesize multi-context managers in `with` statements ([#9222](https://github.com/astral-sh/ruff/pull/9222))
- \[`flake8-pyi`\] Implement `generator-return-from-iter-method` (`PYI058`) ([#9313](https://github.com/astral-sh/ruff/pull/9313))
- \[`pylint`\] Implement `empty-comment` (`PLR2044`) ([#9174](https://github.com/astral-sh/ruff/pull/9174))
- \[`refurb`\] Implement `bit-count` (`FURB161`) ([#9265](https://github.com/astral-sh/ruff/pull/9265))
- \[`ruff`\] Add `never-union` rule to detect redundant `typing.NoReturn` and `typing.Never` ([#9217](https://github.com/astral-sh/ruff/pull/9217))

### CLI

- Add paths to TOML parse errors ([#9358](https://github.com/astral-sh/ruff/pull/9358))
- Add row and column numbers to formatter parse errors ([#9321](https://github.com/astral-sh/ruff/pull/9321))
- Improve responsiveness when invoked via Python ([#9315](https://github.com/astral-sh/ruff/pull/9315))
- Short rule messages should not end with a period ([#9345](https://github.com/astral-sh/ruff/pull/9345))

### Configuration

- Respect runtime-required decorators on functions ([#9317](https://github.com/astral-sh/ruff/pull/9317))

### Bug fixes

- Avoid `asyncio-dangling-task` for nonlocal and global bindings ([#9263](https://github.com/astral-sh/ruff/pull/9263))
- Escape trailing placeholders in rule documentation ([#9301](https://github.com/astral-sh/ruff/pull/9301))
- Fix continuation detection following multi-line strings ([#9332](https://github.com/astral-sh/ruff/pull/9332))
- Fix scoping for generators in named expressions in classes ([#9248](https://github.com/astral-sh/ruff/pull/9248))
- Port from obsolete wsl crate to is-wsl ([#9356](https://github.com/astral-sh/ruff/pull/9356))
- Remove special pre-visit for module docstrings ([#9261](https://github.com/astral-sh/ruff/pull/9261))
- Respect `__str__` definitions from super classes ([#9338](https://github.com/astral-sh/ruff/pull/9338))
- Respect `unused-noqa` via `per-file-ignores` ([#9300](https://github.com/astral-sh/ruff/pull/9300))
- Respect attribute chains when resolving builtin call paths ([#9309](https://github.com/astral-sh/ruff/pull/9309))
- Treat all `typing_extensions` members as typing aliases ([#9335](https://github.com/astral-sh/ruff/pull/9335))
- Use `Display` for formatter parse errors ([#9316](https://github.com/astral-sh/ruff/pull/9316))
- Wrap subscripted dicts in parens for f-string conversion ([#9238](https://github.com/astral-sh/ruff/pull/9238))
- \[`flake8-annotations`\] Avoid adding return types to stub methods ([#9277](https://github.com/astral-sh/ruff/pull/9277))
- \[`flake8-annotations`\] Respect mixed `return` and `raise` cases in return-type analysis ([#9310](https://github.com/astral-sh/ruff/pull/9310))
- \[`flake8-bandit`\] Don't report violations when `SafeLoader` is imported from `yaml.loader` (`S506`) ([#9299](https://github.com/astral-sh/ruff/pull/9299))
- \[`pylint`\] Avoid panic when comment is preceded by Unicode ([#9331](https://github.com/astral-sh/ruff/pull/9331))
- \[`pylint`\] Change `PLR0917` error message to match other `PLR09XX` messages ([#9308](https://github.com/astral-sh/ruff/pull/9308))
- \[`refurb`\] Avoid false positives for `math-constant` (`FURB152`) ([#9290](https://github.com/astral-sh/ruff/pull/9290))

### Documentation

- Expand target name for better rule documentation ([#9302](https://github.com/astral-sh/ruff/pull/9302))
- Fix typos found by codespell ([#9346](https://github.com/astral-sh/ruff/pull/9346))
- \[`perflint`\] Document `PERF102` fix un-safety ([#9351](https://github.com/astral-sh/ruff/pull/9351))
- \[`pyupgrade`\] Document `UP007` fix un-safety ([#9306](https://github.com/astral-sh/ruff/pull/9306))

## 0.1.9

### Breaking changes

- Add site-packages to default exclusions ([#9188](https://github.com/astral-sh/ruff/pull/9188))

### Preview features

- Fix: Avoid parenthesizing subscript targets and values ([#9209](https://github.com/astral-sh/ruff/pull/9209))
- \[`pylint`\] Implement `too-many-locals` (`PLR0914`) ([#9163](https://github.com/astral-sh/ruff/pull/9163))
- Implement `reimplemented_operator` (FURB118) ([#9171](https://github.com/astral-sh/ruff/pull/9171))
- Add a rule to detect string members in runtime-evaluated unions ([#9143](https://github.com/astral-sh/ruff/pull/9143))
- Implement `no_blank_line_before_class_docstring` preview style ([#9154](https://github.com/astral-sh/ruff/pull/9154))

### Rule changes

- `CONSTANT_CASE` variables are improperly flagged for yoda violation (`SIM300`) ([#9164](https://github.com/astral-sh/ruff/pull/9164))
- \[`flake8-pyi`\] Cover ParamSpecs and TypeVarTuples (`PYI018`) ([#9198](https://github.com/astral-sh/ruff/pull/9198))
- \[`flake8-bugbear`\] Add fix for `zip-without-explicit-strict` (`B905`) ([#9176](https://github.com/astral-sh/ruff/pull/9176))
- Add fix to automatically remove `print` and `pprint` statements (`T201`, `T203`) ([#9208](https://github.com/astral-sh/ruff/pull/9208))
- Prefer `Never` to `NoReturn` in auto-typing in Python >= 3.11 (`ANN201`) ([#9213](https://github.com/astral-sh/ruff/pull/9213))

### Formatter

- `can_omit_optional_parentheses`: Exit early for unparenthesized expressions ([#9125](https://github.com/astral-sh/ruff/pull/9125))
- Fix `dynamic` mode with doctests so that it doesn't exceed configured line width ([#9129](https://github.com/astral-sh/ruff/pull/9129))
- Fix `can_omit_optional_parentheses` for expressions with a right most fstring ([#9124](https://github.com/astral-sh/ruff/pull/9124))
- Add `target_version` to formatter options ([#9220](https://github.com/astral-sh/ruff/pull/9220))

### CLI

- Update `ruff format --check` to display message for already formatted files ([#9153](https://github.com/astral-sh/ruff/pull/9153))

### Bug fixes

- Reverse order of arguments for `operator.contains` ([#9192](https://github.com/astral-sh/ruff/pull/9192))
- Iterate over lambdas in deferred type annotations ([#9175](https://github.com/astral-sh/ruff/pull/9175))
- Fix panic in `D208` with multibyte indent ([#9147](https://github.com/astral-sh/ruff/pull/9147))
- Add support for `NoReturn` in auto-return-typing ([#9206](https://github.com/astral-sh/ruff/pull/9206))
- Allow removal of `typing` from `exempt-modules` ([#9214](https://github.com/astral-sh/ruff/pull/9214))
- Avoid `mutable-class-default` violations for Pydantic subclasses ([#9187](https://github.com/astral-sh/ruff/pull/9187))
- Fix dropped union expressions for piped non-types in `PYI055` autofix ([#9161](https://github.com/astral-sh/ruff/pull/9161))
- Enable annotation quoting for multi-line expressions ([#9142](https://github.com/astral-sh/ruff/pull/9142))
- Deduplicate edits when quoting annotations ([#9140](https://github.com/astral-sh/ruff/pull/9140))
- Prevent invalid utf8 indexing in cell magic detection ([#9146](https://github.com/astral-sh/ruff/pull/9146))
- Avoid nested quotations in auto-quoting fix ([#9168](https://github.com/astral-sh/ruff/pull/9168))
- Add base-class inheritance detection to flake8-django rules ([#9151](https://github.com/astral-sh/ruff/pull/9151))
- Avoid `asyncio-dangling-task` violations on shadowed bindings ([#9215](https://github.com/astral-sh/ruff/pull/9215))

### Documentation

- Fix blog post URL in changelog ([#9119](https://github.com/astral-sh/ruff/pull/9119))
- Add error suppression hint for multi-line strings ([#9205](https://github.com/astral-sh/ruff/pull/9205))
- Fix typo in SemanticModel.parent_expression docstring ([#9167](https://github.com/astral-sh/ruff/pull/9167))
- Document link between import sorting and formatter ([#9117](https://github.com/astral-sh/ruff/pull/9117))

## 0.1.8

This release includes opt-in support for formatting Python snippets within
docstrings via the `docstring-code-format` setting.
[Check out the blog post](https://astral.sh/blog/ruff-v0.1.8) for more details!

### Preview features

- Add `"preserve"` quote-style to mimic Black's skip-string-normalization ([#8822](https://github.com/astral-sh/ruff/pull/8822))
- Implement `prefer_splitting_right_hand_side_of_assignments` preview style ([#8943](https://github.com/astral-sh/ruff/pull/8943))
- \[`pycodestyle`\] Add fix for `unexpected-spaces-around-keyword-parameter-equals` ([#9072](https://github.com/astral-sh/ruff/pull/9072))
- \[`pycodestyle`\] Add fix for comment-related whitespace rules ([#9075](https://github.com/astral-sh/ruff/pull/9075))
- \[`pycodestyle`\] Allow `sys.path` modifications between imports ([#9047](https://github.com/astral-sh/ruff/pull/9047))
- \[`refurb`\] Implement `hashlib-digest-hex` (`FURB181`) ([#9077](https://github.com/astral-sh/ruff/pull/9077))

### Rule changes

- Allow `flake8-type-checking` rules to automatically quote runtime-evaluated references ([#6001](https://github.com/astral-sh/ruff/pull/6001))
- Allow transparent cell magics in Jupyter Notebooks ([#8911](https://github.com/astral-sh/ruff/pull/8911))
- \[`flake8-annotations`\] Avoid `ANN2xx` fixes for abstract methods with empty bodies ([#9034](https://github.com/astral-sh/ruff/pull/9034))
- \[`flake8-self`\] Ignore underscore references in type annotations ([#9036](https://github.com/astral-sh/ruff/pull/9036))
- \[`pep8-naming`\] Allow class names when `apps.get_model` is a non-string ([#9065](https://github.com/astral-sh/ruff/pull/9065))
- \[`pycodestyle`\] Allow `matplotlib.use` calls to intersperse imports ([#9094](https://github.com/astral-sh/ruff/pull/9094))
- \[`pyflakes`\] Support fixing unused assignments in tuples by renaming variables (`F841`) ([#9107](https://github.com/astral-sh/ruff/pull/9107))
- \[`pylint`\] Add fix for `subprocess-run-without-check` (`PLW1510`) ([#6708](https://github.com/astral-sh/ruff/pull/6708))

### Formatter

- Add `docstring-code-format` knob to enable docstring snippet formatting ([#8854](https://github.com/astral-sh/ruff/pull/8854))
- Use double quotes for all docstrings, including single-quoted docstrings ([#9020](https://github.com/astral-sh/ruff/pull/9020))
- Implement "dynamic" line width mode for docstring code formatting ([#9098](https://github.com/astral-sh/ruff/pull/9098))
- Support reformatting Markdown code blocks ([#9030](https://github.com/astral-sh/ruff/pull/9030))
- add support for formatting reStructuredText code snippets ([#9003](https://github.com/astral-sh/ruff/pull/9003))
- Avoid trailing comma for single-argument with positional separator ([#9076](https://github.com/astral-sh/ruff/pull/9076))
- Fix handling of trailing target comment ([#9051](https://github.com/astral-sh/ruff/pull/9051))

### CLI

- Hide unsafe fix suggestions when explicitly disabled ([#9095](https://github.com/astral-sh/ruff/pull/9095))
- Add SARIF support to `--output-format` ([#9078](https://github.com/astral-sh/ruff/pull/9078))

### Bug fixes

- Apply unnecessary index rule prior to enumerate rewrite ([#9012](https://github.com/astral-sh/ruff/pull/9012))
- \[`flake8-err-msg`\] Allow `EM` fixes even if `msg` variable is defined ([#9059](https://github.com/astral-sh/ruff/pull/9059))
- \[`flake8-pie`\] Prevent keyword arguments duplication ([#8450](https://github.com/astral-sh/ruff/pull/8450))
- \[`flake8-pie`\] Respect trailing comma in `unnecessary-dict-kwargs` (`PIE804`) ([#9015](https://github.com/astral-sh/ruff/pull/9015))
- \[`flake8-raise`\] Avoid removing parentheses on ctypes.WinError ([#9027](https://github.com/astral-sh/ruff/pull/9027))
- \[`isort`\] Avoid invalid combination of `force-sort-within-types` and `lines-between-types` ([#9041](https://github.com/astral-sh/ruff/pull/9041))
- \[`isort`\] Ensure that from-style imports are always ordered first in `__future__` ([#9039](https://github.com/astral-sh/ruff/pull/9039))
- \[`pycodestyle`\] Allow tab indentation before keyword ([#9099](https://github.com/astral-sh/ruff/pull/9099))
- \[`pylint`\] Ignore `@overrides` and `@overloads` for `too-many-positional` ([#9000](https://github.com/astral-sh/ruff/pull/9000))
- \[`pyupgrade`\] Enable `printf-string-formatting` fix with comments on right-hand side ([#9037](https://github.com/astral-sh/ruff/pull/9037))
- \[`refurb`\] Make `math-constant` (`FURB152`) rule more targeted ([#9054](https://github.com/astral-sh/ruff/pull/9054))
- \[`refurb`\] Support floating-point base in `redundant-log-base` (`FURB163`) ([#9100](https://github.com/astral-sh/ruff/pull/9100))
- \[`ruff`\] Detect `unused-asyncio-dangling-task` (`RUF006`) on unused assignments ([#9060](https://github.com/astral-sh/ruff/pull/9060))

## 0.1.7

### Preview features

- Implement multiline dictionary and list hugging for preview style ([#8293](https://github.com/astral-sh/ruff/pull/8293))
- Implement the `fix_power_op_line_length` preview style ([#8947](https://github.com/astral-sh/ruff/pull/8947))
- Use Python version to determine typing rewrite safety ([#8919](https://github.com/astral-sh/ruff/pull/8919))
- \[`flake8-annotations`\] Enable auto-return-type involving `Optional` and `Union` annotations ([#8885](https://github.com/astral-sh/ruff/pull/8885))
- \[`flake8-bandit`\] Implement `django-raw-sql` (`S611`) ([#8651](https://github.com/astral-sh/ruff/pull/8651))
- \[`flake8-bandit`\] Implement `tarfile-unsafe-members` (`S202`) ([#8829](https://github.com/astral-sh/ruff/pull/8829))
- \[`flake8-pyi`\] Implement fix for `unnecessary-literal-union` (`PYI030`) ([#7934](https://github.com/astral-sh/ruff/pull/7934))
- \[`flake8-simplify`\] Extend `dict-get-with-none-default` (`SIM910`) to non-literals ([#8762](https://github.com/astral-sh/ruff/pull/8762))
- \[`pylint`\] - add `unnecessary-list-index-lookup` (`PLR1736`) + autofix ([#7999](https://github.com/astral-sh/ruff/pull/7999))
- \[`pylint`\] - implement R0202 and R0203 with autofixes ([#8335](https://github.com/astral-sh/ruff/pull/8335))
- \[`pylint`\] Implement `repeated-keyword` (`PLe1132`) ([#8706](https://github.com/astral-sh/ruff/pull/8706))
- \[`pylint`\] Implement `too-many-positional` (`PLR0917`) ([#8995](https://github.com/astral-sh/ruff/pull/8995))
- \[`pylint`\] Implement `unnecessary-dict-index-lookup` (`PLR1733`) ([#8036](https://github.com/astral-sh/ruff/pull/8036))
- \[`refurb`\] Implement `redundant-log-base` (`FURB163`) ([#8842](https://github.com/astral-sh/ruff/pull/8842))

### Rule changes

- \[`flake8-boolean-trap`\] Allow booleans in `@override` methods ([#8882](https://github.com/astral-sh/ruff/pull/8882))
- \[`flake8-bugbear`\] Avoid `B015`,`B018` for last expression in a cell ([#8815](https://github.com/astral-sh/ruff/pull/8815))
- \[`flake8-pie`\] Allow ellipses for enum values in stub files ([#8825](https://github.com/astral-sh/ruff/pull/8825))
- \[`flake8-pyi`\] Check PEP 695 type aliases for `snake-case-type-alias` and `t-suffixed-type-alias` ([#8966](https://github.com/astral-sh/ruff/pull/8966))
- \[`flake8-pyi`\] Check for kwarg and vararg `NoReturn` type annotations ([#8948](https://github.com/astral-sh/ruff/pull/8948))
- \[`flake8-simplify`\] Omit select context managers from `SIM117` ([#8801](https://github.com/astral-sh/ruff/pull/8801))
- \[`pep8-naming`\] Allow Django model loads in `non-lowercase-variable-in-function` (`N806`) ([#8917](https://github.com/astral-sh/ruff/pull/8917))
- \[`pycodestyle`\] Avoid `E703` for last expression in a cell ([#8821](https://github.com/astral-sh/ruff/pull/8821))
- \[`pycodestyle`\] Update `E402` to work at cell level for notebooks ([#8872](https://github.com/astral-sh/ruff/pull/8872))
- \[`pydocstyle`\] Avoid `D100` for Jupyter Notebooks ([#8816](https://github.com/astral-sh/ruff/pull/8816))
- \[`pylint`\] Implement fix for `unspecified-encoding` (`PLW1514`) ([#8928](https://github.com/astral-sh/ruff/pull/8928))

### Formatter

- Avoid unstable formatting in ellipsis-only body with trailing comment ([#8984](https://github.com/astral-sh/ruff/pull/8984))
- Inline trailing comments for type alias similar to assignments ([#8941](https://github.com/astral-sh/ruff/pull/8941))
- Insert trailing comma when function breaks with single argument ([#8921](https://github.com/astral-sh/ruff/pull/8921))

### CLI

- Update `ruff check` and `ruff format` to default to the current directory ([#8791](https://github.com/astral-sh/ruff/pull/8791))
- Stop at the first resolved parent configuration ([#8864](https://github.com/astral-sh/ruff/pull/8864))

### Configuration

- \[`pylint`\] Default `max-positional-args` to `max-args` ([#8998](https://github.com/astral-sh/ruff/pull/8998))
- \[`pylint`\] Add `allow-dunder-method-names` setting for `bad-dunder-method-name` (`PLW3201`) ([#8812](https://github.com/astral-sh/ruff/pull/8812))
- \[`isort`\] Add support for `from-first` setting ([#8663](https://github.com/astral-sh/ruff/pull/8663))
- \[`isort`\] Add support for `length-sort` settings ([#8841](https://github.com/astral-sh/ruff/pull/8841))

### Bug fixes

- Add support for `@functools.singledispatch` ([#8934](https://github.com/astral-sh/ruff/pull/8934))
- Avoid off-by-one error in stripping noqa following multi-byte char ([#8979](https://github.com/astral-sh/ruff/pull/8979))
- Avoid off-by-one error in with-item named expressions ([#8915](https://github.com/astral-sh/ruff/pull/8915))
- Avoid syntax error via invalid ur string prefix ([#8971](https://github.com/astral-sh/ruff/pull/8971))
- Avoid underflow in `get_model` matching ([#8965](https://github.com/astral-sh/ruff/pull/8965))
- Avoid unnecessary index diagnostics when value is modified ([#8970](https://github.com/astral-sh/ruff/pull/8970))
- Convert over-indentation rule to use number of characters ([#8983](https://github.com/astral-sh/ruff/pull/8983))
- Detect implicit returns in auto-return-types ([#8952](https://github.com/astral-sh/ruff/pull/8952))
- Fix start >= end error in over-indentation ([#8982](https://github.com/astral-sh/ruff/pull/8982))
- Ignore `@overload` and `@override` methods for too-many-arguments checks ([#8954](https://github.com/astral-sh/ruff/pull/8954))
- Lexer start of line is false only for `Mode::Expression` ([#8880](https://github.com/astral-sh/ruff/pull/8880))
- Mark `pydantic_settings.BaseSettings` as having default copy semantics ([#8793](https://github.com/astral-sh/ruff/pull/8793))
- Respect dictionary unpacking in `NamedTuple` assignments ([#8810](https://github.com/astral-sh/ruff/pull/8810))
- Respect local subclasses in `flake8-type-checking` ([#8768](https://github.com/astral-sh/ruff/pull/8768))
- Support type alias statements in simple statement positions ([#8916](https://github.com/astral-sh/ruff/pull/8916))
- \[`flake8-annotations`\] Avoid filtering out un-representable types in return annotation ([#8881](https://github.com/astral-sh/ruff/pull/8881))
- \[`flake8-pie`\] Retain extra ellipses in protocols and abstract methods ([#8769](https://github.com/astral-sh/ruff/pull/8769))
- \[`flake8-pyi`\] Respect local enum subclasses in `simple-defaults` (`PYI052`) ([#8767](https://github.com/astral-sh/ruff/pull/8767))
- \[`flake8-trio`\] Use correct range for `TRIO115` fix ([#8933](https://github.com/astral-sh/ruff/pull/8933))
- \[`flake8-trio`\] Use full arguments range for zero-sleep-call ([#8936](https://github.com/astral-sh/ruff/pull/8936))
- \[`isort`\] fix: mark `__main__` as first-party import ([#8805](https://github.com/astral-sh/ruff/pull/8805))
- \[`pep8-naming`\] Avoid `N806` errors for type alias statements ([#8785](https://github.com/astral-sh/ruff/pull/8785))
- \[`perflint`\] Avoid `PERF101` if there's an append in loop body ([#8809](https://github.com/astral-sh/ruff/pull/8809))
- \[`pycodestyle`\] Allow space-before-colon after end-of-slice ([#8838](https://github.com/astral-sh/ruff/pull/8838))
- \[`pydocstyle`\] Avoid non-character breaks in `over-indentation` (`D208`) ([#8866](https://github.com/astral-sh/ruff/pull/8866))
- \[`pydocstyle`\] Ignore underlines when determining docstring logical lines ([#8929](https://github.com/astral-sh/ruff/pull/8929))
- \[`pylint`\] Extend `self-assigning-variable` to multi-target assignments ([#8839](https://github.com/astral-sh/ruff/pull/8839))
- \[`tryceratops`\] Avoid repeated triggers in nested `tryceratops` diagnostics ([#8772](https://github.com/astral-sh/ruff/pull/8772))

### Documentation

- Add advice for fixing RUF008 when mutability is not desired ([#8853](https://github.com/astral-sh/ruff/pull/8853))
- Added the command to run ruff using pkgx to the installation.md ([#8955](https://github.com/astral-sh/ruff/pull/8955))
- Document fix safety for flake8-comprehensions and some pyupgrade rules ([#8918](https://github.com/astral-sh/ruff/pull/8918))
- Fix doc formatting for zero-sleep-call ([#8937](https://github.com/astral-sh/ruff/pull/8937))
- Remove duplicate imports from os-stat documentation ([#8930](https://github.com/astral-sh/ruff/pull/8930))
- Replace generated reference to MkDocs ([#8806](https://github.com/astral-sh/ruff/pull/8806))
- Update Arch Linux package URL in installation.md ([#8802](https://github.com/astral-sh/ruff/pull/8802))
- \[`flake8-pyi`\] Fix error in `t-suffixed-type-alias` (`PYI043`) example ([#8963](https://github.com/astral-sh/ruff/pull/8963))
- \[`flake8-pyi`\] Improve motivation for `custom-type-var-return-type` (`PYI019`) ([#8766](https://github.com/astral-sh/ruff/pull/8766))

## 0.1.6

### Preview features

- \[`flake8-boolean-trap`\] Extend `boolean-type-hint-positional-argument` (`FBT001`) to include booleans in unions ([#7501](https://github.com/astral-sh/ruff/pull/7501))
- \[`flake8-pie`\] Extend `reimplemented-list-builtin` (`PIE807`) to `dict` reimplementations ([#8608](https://github.com/astral-sh/ruff/pull/8608))
- \[`flake8-pie`\] Extend `unnecessary-pass` (`PIE790`) to include ellipses (`...`) ([#8641](https://github.com/astral-sh/ruff/pull/8641))
- \[`flake8-pie`\] Implement fix for `unnecessary-spread` (`PIE800`) ([#8668](https://github.com/astral-sh/ruff/pull/8668))
- \[`flake8-quotes`\] Implement `unnecessary-escaped-quote` (`Q004`) ([#8630](https://github.com/astral-sh/ruff/pull/8630))
- \[`pycodestyle`\] Implement fix for `multiple-spaces-after-keyword` (`E271`) and `multiple-spaces-before-keyword` (`E272`) ([#8622](https://github.com/astral-sh/ruff/pull/8622))
- \[`pycodestyle`\] Implement fix for `multiple-spaces-after-operator` (`E222`) and `multiple-spaces-before-operator` (`E221`) ([#8623](https://github.com/astral-sh/ruff/pull/8623))
- \[`pyflakes`\] Extend `is-literal` (`F632`) to include comparisons against mutable initializers ([#8607](https://github.com/astral-sh/ruff/pull/8607))
- \[`pylint`\] Implement `redefined-argument-from-local` (`PLR1704`) ([#8159](https://github.com/astral-sh/ruff/pull/8159))
- \[`pylint`\] Implement fix for `unnecessary-lambda` (`PLW0108`) ([#8621](https://github.com/astral-sh/ruff/pull/8621))
- \[`refurb`\] Implement `if-expr-min-max` (`FURB136`) ([#8664](https://github.com/astral-sh/ruff/pull/8664))
- \[`refurb`\] Implement `math-constant` (`FURB152`) ([#8727](https://github.com/astral-sh/ruff/pull/8727))

### Rule changes

- \[`flake8-annotations`\] Add autotyping-like return type inference for annotation rules ([#8643](https://github.com/astral-sh/ruff/pull/8643))
- \[`flake8-future-annotations`\] Implement fix for `future-required-type-annotation` (`FA102`) ([#8711](https://github.com/astral-sh/ruff/pull/8711))
- \[`flake8-implicit-namespace-package`\] Avoid missing namespace violations in scripts with shebangs ([#8710](https://github.com/astral-sh/ruff/pull/8710))
- \[`pydocstyle`\] Update `over-indentation` (`D208`) to preserve indentation offsets when fixing overindented lines ([#8699](https://github.com/astral-sh/ruff/pull/8699))
- \[`pyupgrade`\] Refine `timeout-error-alias` (`UP041`) to remove false positives ([#8587](https://github.com/astral-sh/ruff/pull/8587))

### Formatter

- Fix instability in `await` formatting with fluent style ([#8676](https://github.com/astral-sh/ruff/pull/8676))
- Compare formatted and unformatted ASTs during formatter tests ([#8624](https://github.com/astral-sh/ruff/pull/8624))
- Preserve trailing semicolon for Notebooks ([#8590](https://github.com/astral-sh/ruff/pull/8590))

### CLI

- Improve debug printing for resolving origin of config settings ([#8729](https://github.com/astral-sh/ruff/pull/8729))
- Write unchanged, excluded files to stdout when read via stdin ([#8596](https://github.com/astral-sh/ruff/pull/8596))

### Configuration

- \[`isort`\] Support disabling sections with `no-sections = true` ([#8657](https://github.com/astral-sh/ruff/pull/8657))
- \[`pep8-naming`\] Support local and dynamic class- and static-method decorators ([#8592](https://github.com/astral-sh/ruff/pull/8592))
- \[`pydocstyle`\] Allow overriding pydocstyle convention rules ([#8586](https://github.com/astral-sh/ruff/pull/8586))

### Bug fixes

- Avoid syntax error via importing `trio.lowlevel` ([#8730](https://github.com/astral-sh/ruff/pull/8730))
- Omit unrolled augmented assignments in `PIE794` ([#8634](https://github.com/astral-sh/ruff/pull/8634))
- Slice source code instead of generating it for `EM` fixes ([#7746](https://github.com/astral-sh/ruff/pull/7746))
- Allow whitespace around colon in slices for `whitespace-before-punctuation` (`E203`) ([#8654](https://github.com/astral-sh/ruff/pull/8654))
- Use function range for `no-self-use` ([#8637](https://github.com/astral-sh/ruff/pull/8637))
- F-strings doesn't contain bytes literal for `PLW0129` ([#8675](https://github.com/astral-sh/ruff/pull/8675))
- Improve detection of `TYPE_CHECKING` blocks imported from `typing_extensions` or `_typeshed` ([#8429](https://github.com/astral-sh/ruff/pull/8429))
- Treat display as a builtin in IPython ([#8707](https://github.com/astral-sh/ruff/pull/8707))
- Avoid `FURB113` autofix if comments are present ([#8494](https://github.com/astral-sh/ruff/pull/8494))
- Consider the new f-string tokens for `flake8-commas` ([#8582](https://github.com/astral-sh/ruff/pull/8582))
- Remove erroneous bad-dunder-name reference ([#8742](https://github.com/astral-sh/ruff/pull/8742))
- Avoid recommending Self usages in metaclasses ([#8639](https://github.com/astral-sh/ruff/pull/8639))
- Detect runtime-evaluated base classes defined in the current file ([#8572](https://github.com/astral-sh/ruff/pull/8572))
- Avoid inserting trailing commas within f-strings ([#8574](https://github.com/astral-sh/ruff/pull/8574))
- Remove incorrect deprecation label for stdout and stderr ([#8743](https://github.com/astral-sh/ruff/pull/8743))
- Fix unnecessary parentheses in UP007 fix ([#8610](https://github.com/astral-sh/ruff/pull/8610))
- Remove repeated and erroneous scoped settings headers in docs ([#8670](https://github.com/astral-sh/ruff/pull/8670))
- Trim trailing empty strings when converting to f-strings ([#8712](https://github.com/astral-sh/ruff/pull/8712))
- Fix ordering for `force-sort-within-sections` ([#8665](https://github.com/astral-sh/ruff/pull/8665))
- Run unicode prefix rule over tokens ([#8709](https://github.com/astral-sh/ruff/pull/8709))
- Update UP032 to unescape curly braces in literal parts of converted strings ([#8697](https://github.com/astral-sh/ruff/pull/8697))
- List all ipython builtins ([#8719](https://github.com/astral-sh/ruff/pull/8719))

### Documentation

- Document conventions in the FAQ ([#8638](https://github.com/astral-sh/ruff/pull/8638))
- Redirect from rule codes to rule pages in docs ([#8636](https://github.com/astral-sh/ruff/pull/8636))
- Fix permalink to convention setting ([#8575](https://github.com/astral-sh/ruff/pull/8575))

## 0.1.5

### Preview features

- \[`flake8-bandit`\] Implement `mako-templates` (`S702`) ([#8533](https://github.com/astral-sh/ruff/pull/8533))
- \[`flake8-trio`\] Implement `TRIO105` ([#8490](https://github.com/astral-sh/ruff/pull/8490))
- \[`flake8-trio`\] Implement `TRIO109` ([#8534](https://github.com/astral-sh/ruff/pull/8534))
- \[`flake8-trio`\] Implement `TRIO110` ([#8537](https://github.com/astral-sh/ruff/pull/8537))
- \[`flake8-trio`\] Implement `TRIO115` ([#8486](https://github.com/astral-sh/ruff/pull/8486))
- \[`refurb`\] Implement `type-none-comparison` (`FURB169`) ([#8487](https://github.com/astral-sh/ruff/pull/8487))
- Flag all comparisons against builtin types in `E721` ([#8491](https://github.com/astral-sh/ruff/pull/8491))
- Make `SIM118` fix as safe when the expression is a known dictionary ([#8525](https://github.com/astral-sh/ruff/pull/8525))

### Formatter

- Fix multiline lambda expression statement formatting ([#8466](https://github.com/astral-sh/ruff/pull/8466))

### CLI

- Add hidden `--extension` to override inference of source type from file extension ([#8373](https://github.com/astral-sh/ruff/pull/8373))

### Configuration

- Account for selector specificity when merging `extend_unsafe_fixes` and `override extend_safe_fixes` ([#8444](https://github.com/astral-sh/ruff/pull/8444))
- Add support for disabling cache with `RUFF_NO_CACHE` environment variable ([#8538](https://github.com/astral-sh/ruff/pull/8538))

### Bug fixes

- \[`E721`\] Flag comparisons to `memoryview` ([#8485](https://github.com/astral-sh/ruff/pull/8485))
- Allow collapsed-ellipsis bodies in other statements ([#8499](https://github.com/astral-sh/ruff/pull/8499))
- Avoid `D301` autofix for `u` prefixed strings ([#8495](https://github.com/astral-sh/ruff/pull/8495))
- Only flag `flake8-trio` rules when `trio` import is present ([#8550](https://github.com/astral-sh/ruff/pull/8550))
- Reject more syntactically invalid Python programs ([#8524](https://github.com/astral-sh/ruff/pull/8524))
- Avoid raising `TRIO115` violations for `trio.sleep(...)` calls with non-number values ([#8532](https://github.com/astral-sh/ruff/pull/8532))
- Fix `F841` false negative on assignment to multiple variables ([#8489](https://github.com/astral-sh/ruff/pull/8489))

### Documentation

- Fix link to isort `known-first-party` ([#8562](https://github.com/astral-sh/ruff/pull/8562))
- Add notes on fix safety to a few rules ([#8500](https://github.com/astral-sh/ruff/pull/8500))
- Add missing toml config tabs ([#8512](https://github.com/astral-sh/ruff/pull/8512))
- Add instructions for configuration of Emacs ([#8488](https://github.com/astral-sh/ruff/pull/8488))
- Improve detail link contrast in dark mode ([#8548](https://github.com/astral-sh/ruff/pull/8548))
- Fix typo in example ([#8506](https://github.com/astral-sh/ruff/pull/8506))
- Added tabs for configuration files in the documentation ([#8480](https://github.com/astral-sh/ruff/pull/8480))
- Recommend `project.requires-python` over `target-version` ([#8513](https://github.com/astral-sh/ruff/pull/8513))
- Add singleton escape hatch to `B008` documentation ([#8501](https://github.com/astral-sh/ruff/pull/8501))
- Fix tab configuration docs ([#8502](https://github.com/astral-sh/ruff/pull/8502))

## 0.1.4

### Preview features

- \[`flake8-trio`\] Implement `timeout-without-await` (`TRIO001`) ([#8439](https://github.com/astral-sh/ruff/pull/8439))
- \[`numpy`\] Implement NumPy 2.0 migration rule (`NPY200`) ([#7702](https://github.com/astral-sh/ruff/pull/7702))
- \[`pylint`\] Implement `bad-open-mode` (`W1501`) ([#8294](https://github.com/astral-sh/ruff/pull/8294))
- \[`pylint`\] Implement `import-outside-toplevel` (`C0415`) rule ([#5180](https://github.com/astral-sh/ruff/pull/5180))
- \[`pylint`\] Implement `useless-with-lock` (`W2101`) ([#8321](https://github.com/astral-sh/ruff/pull/8321))
- \[`pyupgrade`\] Implement `timeout-error-alias` (`UP041`) ([#8476](https://github.com/astral-sh/ruff/pull/8476))
- \[`refurb`\] Implement `isinstance-type-none` (`FURB168`) ([#8308](https://github.com/astral-sh/ruff/pull/8308))
- Detect confusable Unicode-to-Unicode units in `RUF001`, `RUF002`, and `RUF003` ([#4430](https://github.com/astral-sh/ruff/pull/4430))
- Add newline after module docstrings in preview style ([#8283](https://github.com/astral-sh/ruff/pull/8283))

### Formatter

- Add a note on line-too-long to the formatter docs ([#8314](https://github.com/astral-sh/ruff/pull/8314))
- Preserve trailing statement semicolons when using `fmt: skip` ([#8273](https://github.com/astral-sh/ruff/pull/8273))
- Preserve trailing semicolons when using `fmt: off` ([#8275](https://github.com/astral-sh/ruff/pull/8275))
- Avoid duplicating linter-formatter compatibility warnings ([#8292](https://github.com/astral-sh/ruff/pull/8292))
- Avoid inserting a newline after function docstrings ([#8375](https://github.com/astral-sh/ruff/pull/8375))
- Insert newline between docstring and following own line comment ([#8216](https://github.com/astral-sh/ruff/pull/8216))
- Split tuples in return positions by comma first ([#8280](https://github.com/astral-sh/ruff/pull/8280))
- Avoid treating byte strings as docstrings ([#8350](https://github.com/astral-sh/ruff/pull/8350))
- Add `--line-length` option to `format` command ([#8363](https://github.com/astral-sh/ruff/pull/8363))
- Avoid parenthesizing unsplittable because of comments ([#8431](https://github.com/astral-sh/ruff/pull/8431))

### CLI

- Add `--output-format` to `ruff rule` and `ruff linter` ([#8203](https://github.com/astral-sh/ruff/pull/8203))

### Bug fixes

- Respect `--force-exclude` in `lint.exclude` and `format.exclude` ([#8393](https://github.com/astral-sh/ruff/pull/8393))
- Respect `--extend-per-file-ignores` on the CLI ([#8329](https://github.com/astral-sh/ruff/pull/8329))
- Extend `bad-dunder-method-name` to permit `__index__` ([#8300](https://github.com/astral-sh/ruff/pull/8300))
- Fix panic with 8 in octal escape ([#8356](https://github.com/astral-sh/ruff/pull/8356))
- Avoid raising `D300` when both triple quote styles are present ([#8462](https://github.com/astral-sh/ruff/pull/8462))
- Consider unterminated f-strings in `FStringRanges` ([#8154](https://github.com/astral-sh/ruff/pull/8154))
- Avoid including literal `shell=True` for truthy, non-`True` diagnostics ([#8359](https://github.com/astral-sh/ruff/pull/8359))
- Avoid triggering single-element test for starred expressions ([#8433](https://github.com/astral-sh/ruff/pull/8433))
- Detect and ignore Jupyter automagics ([#8398](https://github.com/astral-sh/ruff/pull/8398))
- Fix invalid E231 error with f-strings ([#8369](https://github.com/astral-sh/ruff/pull/8369))
- Avoid triggering `NamedTuple` rewrite with starred annotation ([#8434](https://github.com/astral-sh/ruff/pull/8434))
- Avoid un-setting bracket flag in logical lines ([#8380](https://github.com/astral-sh/ruff/pull/8380))
- Place 'r' prefix before 'f' for raw format strings ([#8464](https://github.com/astral-sh/ruff/pull/8464))
- Remove trailing periods from NumPy 2.0 code actions ([#8475](https://github.com/astral-sh/ruff/pull/8475))
- Fix bug where `PLE1307` was raised when formatting `%c` with characters ([#8407](https://github.com/astral-sh/ruff/pull/8407))
- Remove unicode flag from comparable ([#8440](https://github.com/astral-sh/ruff/pull/8440))
- Improve B015 message ([#8295](https://github.com/astral-sh/ruff/pull/8295))
- Use `fixedOverflowWidgets` for playground popover ([#8458](https://github.com/astral-sh/ruff/pull/8458))
- Mark `byte_bounds` as a non-backwards-compatible NumPy 2.0 change ([#8474](https://github.com/astral-sh/ruff/pull/8474))

### Internals

- Add a dedicated cache directory per Ruff version ([#8333](https://github.com/astral-sh/ruff/pull/8333))
- Allow selective caching for `--fix` and `--diff` ([#8316](https://github.com/astral-sh/ruff/pull/8316))
- Improve performance of comment parsing ([#8193](https://github.com/astral-sh/ruff/pull/8193))
- Improve performance of string parsing ([#8227](https://github.com/astral-sh/ruff/pull/8227))
- Use a dedicated sort key for isort import sorting ([#7963](https://github.com/astral-sh/ruff/pull/7963))

## 0.1.3

This release includes a variety of improvements to the Ruff formatter, removing several known and
unintentional deviations from Black.

### Formatter

- Avoid space around pow for `None`, `True` and `False` ([#8189](https://github.com/astral-sh/ruff/pull/8189))
- Avoid sorting all paths in the format command ([#8181](https://github.com/astral-sh/ruff/pull/8181))
- Insert necessary blank line between class and leading comments ([#8224](https://github.com/astral-sh/ruff/pull/8224))
- Avoid introducing new parentheses in annotated assignments ([#8233](https://github.com/astral-sh/ruff/pull/8233))
- Refine the warnings about incompatible linter options ([#8196](https://github.com/astral-sh/ruff/pull/8196))
- Add test and basic implementation for formatter preview mode ([#8044](https://github.com/astral-sh/ruff/pull/8044))
- Refine warning about incompatible `isort` settings ([#8192](https://github.com/astral-sh/ruff/pull/8192))
- Only omit optional parentheses for starting or ending with parentheses ([#8238](https://github.com/astral-sh/ruff/pull/8238))
- Use source type to determine parser mode for formatting ([#8205](https://github.com/astral-sh/ruff/pull/8205))
- Don't warn about magic trailing comma when `isort.force-single-line` is true ([#8244](https://github.com/astral-sh/ruff/pull/8244))
- Use `SourceKind::diff` for formatter ([#8240](https://github.com/astral-sh/ruff/pull/8240))
- Fix `fmt:off` with trailing child comment ([#8234](https://github.com/astral-sh/ruff/pull/8234))
- Formatter parentheses support for `IpyEscapeCommand` ([#8207](https://github.com/astral-sh/ruff/pull/8207))

### Linter

- \[`pylint`\] Add buffer methods to `bad-dunder-method-name` (`PLW3201`) exclusions ([#8190](https://github.com/astral-sh/ruff/pull/8190))
- Match rule prefixes from `external` codes setting in `unused-noqa` ([#8177](https://github.com/astral-sh/ruff/pull/8177))
- Use `line-length` setting for isort in lieu of `pycodestyle.max-line-length` ([#8235](https://github.com/astral-sh/ruff/pull/8235))
- Update fix for `unnecessary-paren-on-raise-exception` to unsafe for unknown types ([#8231](https://github.com/astral-sh/ruff/pull/8231))
- Correct quick fix message for `W605` ([#8255](https://github.com/astral-sh/ruff/pull/8255))

### Documentation

- Fix typo in max-doc-length documentation ([#8201](https://github.com/astral-sh/ruff/pull/8201))
- Improve documentation around linter-formatter conflicts ([#8257](https://github.com/astral-sh/ruff/pull/8257))
- Fix link to error suppression documentation in `unused-noqa` ([#8172](https://github.com/astral-sh/ruff/pull/8172))
- Add `external` option to `unused-noqa` documentation ([#8171](https://github.com/astral-sh/ruff/pull/8171))
- Add title attribute to icons ([#8060](https://github.com/astral-sh/ruff/pull/8060))
- Clarify unsafe case in RSE102 ([#8256](https://github.com/astral-sh/ruff/pull/8256))
- Fix skipping formatting examples ([#8210](https://github.com/astral-sh/ruff/pull/8210))
- docs: fix name of `magic-trailing-comma` option in README ([#8200](https://github.com/astral-sh/ruff/pull/8200))
- Add note about scope of rule changing in versioning policy ([#8169](https://github.com/astral-sh/ruff/pull/8169))
- Document: Fix default lint rules ([#8218](https://github.com/astral-sh/ruff/pull/8218))
- Fix a wrong setting in configuration.md ([#8186](https://github.com/astral-sh/ruff/pull/8186))
- Fix misspelled TOML headers in the tutorial ([#8209](https://github.com/astral-sh/ruff/pull/8209))

## 0.1.2

This release includes the Beta version of the Ruff formatter — an extremely fast, Black-compatible Python formatter.
Try it today with `ruff format`! [Check out the blog post](https://astral.sh/blog/the-ruff-formatter) and [read the docs](https://docs.astral.sh/ruff/formatter/).

### Preview features

- \[`pylint`\] Implement `non-ascii-module-import` (`C2403`) ([#8056](https://github.com/astral-sh/ruff/pull/8056))
- \[`pylint`\] implement `non-ascii-name` (`C2401`) ([#8038](https://github.com/astral-sh/ruff/pull/8038))
- \[`pylint`\] Implement unnecessary-lambda (W0108) ([#7953](https://github.com/astral-sh/ruff/pull/7953))
- \[`refurb`\] Implement `read-whole-file` (`FURB101`) ([#7682](https://github.com/astral-sh/ruff/pull/7682))
- Add fix for `E223`, `E224`, and `E242` ([#8143](https://github.com/astral-sh/ruff/pull/8143))
- Add fix for `E225`, `E226`, `E227`, and `E228` ([#8136](https://github.com/astral-sh/ruff/pull/8136))
- Add fix for `E252` ([#8142](https://github.com/astral-sh/ruff/pull/8142))
- Add fix for `E261` ([#8114](https://github.com/astral-sh/ruff/pull/8114))
- Add fix for `E273` and `E274` ([#8144](https://github.com/astral-sh/ruff/pull/8144))
- Add fix for `E275` ([#8133](https://github.com/astral-sh/ruff/pull/8133))
- Update `SIM401` to catch ternary operations ([#7415](https://github.com/astral-sh/ruff/pull/7415))
- Update `E721` to allow `is` and `is` not for direct type comparisons ([#7905](https://github.com/astral-sh/ruff/pull/7905))

### Rule changes

- Add `backports.strenum` to `deprecated-imports` ([#8113](https://github.com/astral-sh/ruff/pull/8113))
- Update `SIM112` to ignore `https_proxy`, `http_proxy`, and `no_proxy` ([#8140](https://github.com/astral-sh/ruff/pull/8140))
- Update fix for `literal-membership` (`PLR6201`) to be unsafe ([#8097](https://github.com/astral-sh/ruff/pull/8097))
- Update fix for `mutable-argument-defaults` (`B006`) to be unsafe ([#8108](https://github.com/astral-sh/ruff/pull/8108))

### Formatter

- Change `line-ending` default to `auto` ([#8057](https://github.com/astral-sh/ruff/pull/8057))
- Respect parenthesized generators in `has_own_parentheses` ([#8100](https://github.com/astral-sh/ruff/pull/8100))
- Add caching to formatter ([#8089](https://github.com/astral-sh/ruff/pull/8089))
- Remove `--line-length` option from `format` command ([#8131](https://github.com/astral-sh/ruff/pull/8131))
- Add formatter to `line-length` documentation ([#8150](https://github.com/astral-sh/ruff/pull/8150))
- Warn about incompatible formatter options ([#8088](https://github.com/astral-sh/ruff/pull/8088))
- Fix range of unparenthesized tuple subject in match statement ([#8101](https://github.com/astral-sh/ruff/pull/8101))
- Remove experimental formatter warning ([#8148](https://github.com/astral-sh/ruff/pull/8148))
- Don't move type param opening parenthesis comment ([#8163](https://github.com/astral-sh/ruff/pull/8163))
- Update versions in format benchmark script ([#8110](https://github.com/astral-sh/ruff/pull/8110))
- Avoid loading files for cached format results ([#8134](https://github.com/astral-sh/ruff/pull/8134))

### CLI

- Show the `ruff format` command in help menus ([#8167](https://github.com/astral-sh/ruff/pull/8167))
- Add `ruff version` command with long version display ([#8034](https://github.com/astral-sh/ruff/pull/8034))

### Configuration

- New `pycodestyle.max-line-length` option ([#8039](https://github.com/astral-sh/ruff/pull/8039))

### Bug fixes

- Detect `sys.version_info` slices in `outdated-version-block` ([#8112](https://github.com/astral-sh/ruff/pull/8112))
- Avoid if-else simplification for `TYPE_CHECKING` blocks ([#8072](https://github.com/astral-sh/ruff/pull/8072))
- Avoid false-positive print separator diagnostic with starred argument ([#8079](https://github.com/astral-sh/ruff/pull/8079))

### Documentation

- Fix message for `too-many-arguments` lint ([#8092](https://github.com/astral-sh/ruff/pull/8092))
- Fix `extend-unsafe-fixes` and `extend-safe-fixes` example ([#8139](https://github.com/astral-sh/ruff/pull/8139))
- Add links to `flake8-import-conventions` options ([#8115](https://github.com/astral-sh/ruff/pull/8115))
- Rework the documentation to incorporate the Ruff formatter ([#7732](https://github.com/astral-sh/ruff/pull/7732))
- Fix `Options` JSON schema description ([#8081](https://github.com/astral-sh/ruff/pull/8081))
- Fix typo (`pytext` -> `pytest`) ([#8117](https://github.com/astral-sh/ruff/pull/8117))
- Improve `magic-value-comparison` example in docs ([#8111](https://github.com/astral-sh/ruff/pull/8111))

## 0.1.1

### Rule changes

- Add unsafe fix for `escape-sequence-in-docstring` (`D301`) ([#7970](https://github.com/astral-sh/ruff/pull/7970))

### Configuration

- Respect `#(deprecated)` attribute in configuration options ([#8035](https://github.com/astral-sh/ruff/pull/8035))
- Add `[format|lint].exclude` options ([#8000](https://github.com/astral-sh/ruff/pull/8000))
- Respect `tab-size` setting in formatter ([#8006](https://github.com/astral-sh/ruff/pull/8006))
- Add `lint.preview` ([#8002](https://github.com/astral-sh/ruff/pull/8002))

### Preview features

- \[`pylint`\] Implement `literal-membership` (`PLR6201`) ([#7973](https://github.com/astral-sh/ruff/pull/7973))
- \[`pylint`\] Implement `too-many-boolean-expressions` (`PLR0916`) ([#7975](https://github.com/astral-sh/ruff/pull/7975))
- \[`pylint`\] Implement `misplaced-bare-raise` (`E0704`) ([#7961](https://github.com/astral-sh/ruff/pull/7961))
- \[`pylint`\] Implement `global-at-module-level` (`W0604`) ([#8058](https://github.com/astral-sh/ruff/pull/8058))
- \[`pylint`\] Implement `unspecified-encoding` (`PLW1514`) ([#7939](https://github.com/astral-sh/ruff/pull/7939))
- Add fix for `triple-single-quotes` (`D300`) ([#7967](https://github.com/astral-sh/ruff/pull/7967))

### Formatter

- New code style badge for `ruff format` ([#7878](https://github.com/astral-sh/ruff/pull/7878))
- Fix comments outside expression parentheses ([#7873](https://github.com/astral-sh/ruff/pull/7873))
- Add `--target-version` to `ruff format` ([#8055](https://github.com/astral-sh/ruff/pull/8055))
- Skip over parentheses when detecting `in` keyword ([#8054](https://github.com/astral-sh/ruff/pull/8054))
- Add `--diff` option to `ruff format` ([#7937](https://github.com/astral-sh/ruff/pull/7937))
- Insert newline after nested function or class statements ([#7946](https://github.com/astral-sh/ruff/pull/7946))
- Use `pass` over ellipsis in non-function/class contexts ([#8049](https://github.com/astral-sh/ruff/pull/8049))

### Bug fixes

- Lazily evaluate all PEP 695 type alias values ([#8033](https://github.com/astral-sh/ruff/pull/8033))
- Avoid failed assertion when showing fixes from stdin ([#8029](https://github.com/astral-sh/ruff/pull/8029))
- Avoid flagging HTTP and HTTPS literals in urllib-open ([#8046](https://github.com/astral-sh/ruff/pull/8046))
- Avoid flagging `bad-dunder-method-name` for `_` ([#8015](https://github.com/astral-sh/ruff/pull/8015))
- Remove Python 2-only methods from `URLOpen` audit ([#8047](https://github.com/astral-sh/ruff/pull/8047))
- Use set bracket replacement for `iteration-over-set` to preserve whitespace and comments ([#8001](https://github.com/astral-sh/ruff/pull/8001))

### Documentation

- Update tutorial to match revised Ruff defaults ([#8066](https://github.com/astral-sh/ruff/pull/8066))
- Update rule `B005` docs ([#8028](https://github.com/astral-sh/ruff/pull/8028))
- Update GitHub actions example in docs to use `--output-format` ([#8014](https://github.com/astral-sh/ruff/pull/8014))
- Document `lint.preview` and `format.preview` ([#8032](https://github.com/astral-sh/ruff/pull/8032))
- Clarify that new rules should be added to `RuleGroup::Preview`. ([#7989](https://github.com/astral-sh/ruff/pull/7989))

## 0.1.0

This is the first release which uses the `CHANGELOG` file. See [GitHub Releases](https://github.com/astral-sh/ruff/releases) for prior changelog entries.

Read Ruff's new [versioning policy](https://docs.astral.sh/ruff/versioning/).

### Breaking changes

- Unsafe fixes are no longer displayed or applied without opt-in ([#7769](https://github.com/astral-sh/ruff/pull/7769))
- Drop formatting specific rules from the default set ([#7900](https://github.com/astral-sh/ruff/pull/7900))
- The deprecated `format` setting has been removed ([#7984](https://github.com/astral-sh/ruff/pull/7984))
    - The `format` setting cannot be used to configure the output format, use `output-format` instead
    - The `RUFF_FORMAT` environment variable is ignored, use `RUFF_OUTPUT_FORMAT` instead
    - The `--format` option has been removed from `ruff check`, use `--output-format` instead

### Rule changes

- Extend `reimplemented-starmap` (`FURB140`) to catch calls with a single and starred argument ([#7768](https://github.com/astral-sh/ruff/pull/7768))
- Improve cases covered by `RUF015` ([#7848](https://github.com/astral-sh/ruff/pull/7848))
- Update `SIM15` to allow `open` followed by `close` ([#7916](https://github.com/astral-sh/ruff/pull/7916))
- Respect `msgspec.Struct` default-copy semantics in `RUF012` ([#7786](https://github.com/astral-sh/ruff/pull/7786))
- Add `sqlalchemy` methods to \`flake8-boolean-trap\`\` exclusion list ([#7874](https://github.com/astral-sh/ruff/pull/7874))
- Add fix for `PLR1714` ([#7910](https://github.com/astral-sh/ruff/pull/7910))
- Add fix for `PIE804` ([#7884](https://github.com/astral-sh/ruff/pull/7884))
- Add fix for `PLC0208` ([#7887](https://github.com/astral-sh/ruff/pull/7887))
- Add fix for `PYI055` ([#7886](https://github.com/astral-sh/ruff/pull/7886))
- Update `non-pep695-type-alias` to require `--unsafe-fixes` outside of stub files ([#7836](https://github.com/astral-sh/ruff/pull/7836))
- Improve fix message for `UP018` ([#7913](https://github.com/astral-sh/ruff/pull/7913))
- Update `PLW3201` to support `Enum` [sunder names](https://docs.python.org/3/library/enum.html#supported-sunder-names) ([#7987](https://github.com/astral-sh/ruff/pull/7987))

### Preview features

- Only show warnings for empty preview selectors when enabling rules ([#7842](https://github.com/astral-sh/ruff/pull/7842))
- Add `unnecessary-key-check` to simplify `key in dct and dct[key]` to `dct.get(key)` ([#7895](https://github.com/astral-sh/ruff/pull/7895))
- Add `assignment-in-assert` to prevent walrus expressions in assert statements ([#7856](https://github.com/astral-sh/ruff/pull/7856))
- \[`refurb`\] Add `single-item-membership-test` (`FURB171`) ([#7815](https://github.com/astral-sh/ruff/pull/7815))
- \[`pylint`\] Add `and-or-ternary` (`R1706`) ([#7811](https://github.com/astral-sh/ruff/pull/7811))

*New rules are added in [preview](https://docs.astral.sh/ruff/preview/).*

### Configuration

- Add `unsafe-fixes` setting ([#7769](https://github.com/astral-sh/ruff/pull/7769))
- Add `extend-safe-fixes` and `extend-unsafe-fixes` for promoting and demoting fixes ([#7841](https://github.com/astral-sh/ruff/pull/7841))

### CLI

- Added `--unsafe-fixes` option for opt-in to display and apply unsafe fixes ([#7769](https://github.com/astral-sh/ruff/pull/7769))
- Fix use of deprecated `--format` option in warning ([#7837](https://github.com/astral-sh/ruff/pull/7837))
- Show changed files when running under `--check` ([#7788](https://github.com/astral-sh/ruff/pull/7788))
- Write summary messages to stderr when fixing via stdin instead of omitting them ([#7838](https://github.com/astral-sh/ruff/pull/7838))
- Update fix summary message in `check --diff` to include unsafe fix hints ([#7790](https://github.com/astral-sh/ruff/pull/7790))
- Add notebook `cell` field to JSON output format ([#7664](https://github.com/astral-sh/ruff/pull/7664))
- Rename applicability levels to `Safe`, `Unsafe`, and `Display` ([#7843](https://github.com/astral-sh/ruff/pull/7843))

### Bug fixes

- Fix bug where f-strings were allowed in match pattern literal ([#7857](https://github.com/astral-sh/ruff/pull/7857))
- Fix `SIM110` with a yield in the condition ([#7801](https://github.com/astral-sh/ruff/pull/7801))
- Preserve trailing comments in `C414` fixes ([#7775](https://github.com/astral-sh/ruff/pull/7775))
- Check sequence type before triggering `unnecessary-enumerate` `len` suggestion ([#7781](https://github.com/astral-sh/ruff/pull/7781))
- Use correct start location for class/function clause header ([#7802](https://github.com/astral-sh/ruff/pull/7802))
- Fix incorrect fixes for `SIM101` ([#7798](https://github.com/astral-sh/ruff/pull/7798))
- Format comment before parameter default correctly ([#7870](https://github.com/astral-sh/ruff/pull/7870))
- Fix `E251` false positive inside f-strings ([#7894](https://github.com/astral-sh/ruff/pull/7894))
- Allow bindings to be created and referenced within annotations ([#7885](https://github.com/astral-sh/ruff/pull/7885))
- Show per-cell diffs when analyzing notebooks over `stdin` ([#7789](https://github.com/astral-sh/ruff/pull/7789))
- Avoid curly brace escape in f-string format spec ([#7780](https://github.com/astral-sh/ruff/pull/7780))
- Fix lexing single-quoted f-string with multi-line format spec ([#7787](https://github.com/astral-sh/ruff/pull/7787))
- Consider nursery rules to be in-preview for `ruff rule` ([#7812](https://github.com/astral-sh/ruff/pull/7812))
- Report precise location for invalid conversion flag ([#7809](https://github.com/astral-sh/ruff/pull/7809))
- Visit pattern match guard as a boolean test ([#7911](https://github.com/astral-sh/ruff/pull/7911))
- Respect `--unfixable` in `ISC` rules ([#7917](https://github.com/astral-sh/ruff/pull/7917))
- Fix edge case with `PIE804` ([#7922](https://github.com/astral-sh/ruff/pull/7922))
- Show custom message in `PTH118` for `Path.joinpath` with starred arguments ([#7852](https://github.com/astral-sh/ruff/pull/7852))
- Fix false negative in `outdated-version-block` when using greater than comparisons ([#7920](https://github.com/astral-sh/ruff/pull/7920))
- Avoid converting f-strings within Django `gettext` calls ([#7898](https://github.com/astral-sh/ruff/pull/7898))
- Fix false positive in `PLR6301` ([#7933](https://github.com/astral-sh/ruff/pull/7933))
- Treat type aliases as typing-only expressions e.g. resolves false positive in `TCH004` ([#7968](https://github.com/astral-sh/ruff/pull/7968))
- Resolve `cache-dir` relative to project root ([#7962](https://github.com/astral-sh/ruff/pull/7962))
- Respect subscripted base classes in type-checking rules e.g. resolves false positive in `TCH003` ([#7954](https://github.com/astral-sh/ruff/pull/7954))
- Fix JSON schema limit for `line-length` ([#7883](https://github.com/astral-sh/ruff/pull/7883))
- Fix commented-out `coalesce` keyword ([#7876](https://github.com/astral-sh/ruff/pull/7876))

### Documentation

- Document `reimplemented-starmap` performance effects ([#7846](https://github.com/astral-sh/ruff/pull/7846))
- Default to following the system dark/light mode ([#7888](https://github.com/astral-sh/ruff/pull/7888))
- Add documentation for fixes ([#7901](https://github.com/astral-sh/ruff/pull/7901))
- Fix typo in docs of `PLR6301` ([#7831](https://github.com/astral-sh/ruff/pull/7831))
- Update `UP038` docs to note that it results in slower code ([#7872](https://github.com/astral-sh/ruff/pull/7872))
- crlf -> cr-lf ([#7766](https://github.com/astral-sh/ruff/pull/7766))
- Add an example of an unsafe fix ([#7924](https://github.com/astral-sh/ruff/pull/7924))
- Fix documented examples for `unnecessary-subscript-reversal` ([#7774](https://github.com/astral-sh/ruff/pull/7774))
- Correct error in tuple example in ruff formatter docs ([#7822](https://github.com/astral-sh/ruff/pull/7822))
- Add versioning policy to documentation ([#7923](https://github.com/astral-sh/ruff/pull/7923))
- Fix invalid code in `FURB177` example ([#7832](https://github.com/astral-sh/ruff/pull/7832))

### Formatter

- Less scary `ruff format` message ([#7867](https://github.com/astral-sh/ruff/pull/7867))
- Remove spaces from import statements ([#7859](https://github.com/astral-sh/ruff/pull/7859))
- Formatter quoting for f-strings with triple quotes ([#7826](https://github.com/astral-sh/ruff/pull/7826))
- Update `ruff_python_formatter` generate.py comment ([#7850](https://github.com/astral-sh/ruff/pull/7850))
- Document one-call chaining deviation ([#7767](https://github.com/astral-sh/ruff/pull/7767))
- Allow f-string modifications in line-shrinking cases ([#7818](https://github.com/astral-sh/ruff/pull/7818))
- Add trailing comment deviation to README ([#7827](https://github.com/astral-sh/ruff/pull/7827))
- Add trailing zero between dot and exponential ([#7956](https://github.com/astral-sh/ruff/pull/7956))
- Force parentheses for power operations in unary expressions ([#7955](https://github.com/astral-sh/ruff/pull/7955))

### Playground

- Fix playground `Quick Fix` action ([#7824](https://github.com/astral-sh/ruff/pull/7824))
