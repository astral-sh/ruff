# Changelog

## 0.1.5

### Preview features

- \[TRIO\] Add TRIO109 rule ([#8534](https://github.com/astral-sh/ruff/pull/8534))
- Add TRIO110 rule ([#8537](https://github.com/astral-sh/ruff/pull/8537))
- \[`refurb`\] Implement `type-none-comparison` (`FURB169`) ([#8487](https://github.com/astral-sh/ruff/pull/8487))
- Make `SIM118` fix as safe when the expression is a known dictionary  ([#8525](https://github.com/astral-sh/ruff/pull/8525))
- \[`TRIO`\] Add `TRIO105`: `SyncTrioCall` ([#8490](https://github.com/astral-sh/ruff/pull/8490))

### Rule changes

- Flag all comparisons against builtin types in E721 ([#8491](https://github.com/astral-sh/ruff/pull/8491))
- \[`flake8-bandit`\] Implement `mako-templates` (`S702`) ([#8533](https://github.com/astral-sh/ruff/pull/8533))
- \[`TRIO`\] Add `TRIO115`: TrioZeroSleepCall ([#8486](https://github.com/astral-sh/ruff/pull/8486))

### Formatter

- Fix multiline lambda expression statement formatting ([#8466](https://github.com/astral-sh/ruff/pull/8466))

### CLI

- Add hidden `--extension` to override inference of source type from file extension ([#8373](https://github.com/astral-sh/ruff/pull/8373))
- Allow RUFF_NO_CACHE environment variable (like RUFF_CACHE_DIR) ([#8538](https://github.com/astral-sh/ruff/pull/8538))

### Configuration

- Account for selector specificity when merging `extend_unsafe_fixes` and `override extend_safe_fixes` ([#8444](https://github.com/astral-sh/ruff/pull/8444))

### Bug fixes

- \[`E721`\] Flag comparisons to `memoryview` ([#8485](https://github.com/astral-sh/ruff/pull/8485))
- Allow collapsed-ellipsis bodies in other statements ([#8499](https://github.com/astral-sh/ruff/pull/8499))
- Avoid `D301` autofix for `u` prefixed strings ([#8495](https://github.com/astral-sh/ruff/pull/8495))
- Only flag flake8-trio rule when trio is present ([#8550](https://github.com/astral-sh/ruff/pull/8550))
- Reject more syntactically invalid Python programs ([#8524](https://github.com/astral-sh/ruff/pull/8524))
- Avoid raising `TRIO115` violations for `trio.sleep(...)` calls with non-number values ([#8532](https://github.com/astral-sh/ruff/pull/8532))

### Other changes

- Add Pillow to Ruff users ([#8523](https://github.com/astral-sh/ruff/pull/8523))
- Add a `Fix` constructor that takes `Applicability` as an argument ([#8514](https://github.com/astral-sh/ruff/pull/8514))
- Bump bitflags from 2.4.0 to 2.4.1 ([#8511](https://github.com/astral-sh/ruff/pull/8511))
- Doc: Fix link to isort `known-first-party` ([#8562](https://github.com/astral-sh/ruff/pull/8562))
- Add notes on fix safety to a few rules ([#8500](https://github.com/astral-sh/ruff/pull/8500))
- Add instructions for configuration of Emacs ([#8488](https://github.com/astral-sh/ruff/pull/8488))
- Bump syn from 2.0.38 to 2.0.39 ([#8509](https://github.com/astral-sh/ruff/pull/8509))
- Add dedicated method to find typed binding ([#8517](https://github.com/astral-sh/ruff/pull/8517))
- Improve detail link contrast in dark mode ([#8548](https://github.com/astral-sh/ruff/pull/8548))
- Fix typo in example ([#8506](https://github.com/astral-sh/ruff/pull/8506))
- Fix `F841` false negative on assignment to multiple variables ([#8489](https://github.com/astral-sh/ruff/pull/8489))
- Upgrade pre-commit dependencies ([#8518](https://github.com/astral-sh/ruff/pull/8518))
- docs: Add missing toml config tabs ([#8512](https://github.com/astral-sh/ruff/pull/8512))
- Update applicability messages for clarity in tests ([#8541](https://github.com/astral-sh/ruff/pull/8541))
- Add PyMC Marketing to Users ([#8529](https://github.com/astral-sh/ruff/pull/8529))
- Add `rooster` release management configuration and instructions ([#8567](https://github.com/astral-sh/ruff/pull/8567))
- Ensure ecosystem project errors are properly fenced ([#8516](https://github.com/astral-sh/ruff/pull/8516))
- Added tabs for configuration files in the documentation ([#8480](https://github.com/astral-sh/ruff/pull/8480))
- Bump serde_json from 1.0.107 to 1.0.108 ([#8510](https://github.com/astral-sh/ruff/pull/8510))
- Bump serde-wasm-bindgen from 0.6.0 to 0.6.1 ([#8507](https://github.com/astral-sh/ruff/pull/8507))
- Bump codspeed-criterion-compat from 2.3.0 to 2.3.1 ([#8508](https://github.com/astral-sh/ruff/pull/8508))
- Add singleton escape hatch to B008 documentation ([#8501](https://github.com/astral-sh/ruff/pull/8501))
- Recommend `project.requires-python` over `target-version` ([#8513](https://github.com/astral-sh/ruff/pull/8513))
- Skip ecosystem check when no changes detected ([#8520](https://github.com/astral-sh/ruff/pull/8520))
- Fix tab configuration docs ([#8502](https://github.com/astral-sh/ruff/pull/8502))
- Update pre-commit documentation ([#8545](https://github.com/astral-sh/ruff/pull/8545))

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
- Update fix for  `mutable-argument-defaults` (`B006`) to be unsafe ([#8108](https://github.com/astral-sh/ruff/pull/8108))

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

_New rules are added in [preview](https://docs.astral.sh/ruff/preview/)._

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
