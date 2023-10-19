# Changelog

## 0.1.1

### Rule changes
- Add unsafe fix for `escape-sequence-in-docstring` (`D301`) (#7970)
- Add fix for `triple-single-quotes` (`D300`) (#7967)

### Configuration
- Respect `#(deprecated)` attribute in configuration options (#8035)
- Add `[format|lint].exclude` options (#8000)
- Respect `tab-size` setting in formatter (#8006)
- Add `lint.preview` (#8002)

## Preview features
- [`pylint`] Implement `literal-membership` (`PLR6201`) (#7973)
- [`pylint`] Implement `too-many-boolean-expressions` (`PLR0916`) (#7975)
- [`pylint`] Implement `misplaced-bare-raise` (`E0704`) (#7961)
- [`pylint`] Implement `global-at-module-level` (`W0604`) (#8058)
- [`pylint`] Implement `unspecified-encoding` (`PLW1514`) (#7939)


### Formatter
- New code style badge for `ruff format` (#7878)
- Fix comments outside expression parentheses (#7873)
- Add `--target-version` to `ruff format` (#8055)
- Skip over parentheses when detecting `in` keyword (#8054)
- Add `--diff` option to `ruff format` (#7937)
- Insert newline after nested function or class statements (#7946)
- Use `pass` over ellipsis in non-function/class contexts (#8049)

### Bug fixes
- Lazily evaluate all PEP 695 type alias values (#8033)
- Avoid failed assertion when showing fixes from stdin (#8029)
- Avoid flagging HTTP and HTTPS literals in urllib-open (#8046)
- Avoid flagging `bad-dunder-method-name` for `_` (#8015)
- Remove Python 2-only methods from `URLOpen` audit (#8047)
- Use set bracket replacement for `iteration-over-set` to preserve whitespace and comments (#8001)

### Documentation
- Update tutorial to match revised Ruff defaults (#8066)
- Update rule `B005` docs (#8028)
- Update GitHub actions example in docs to use `--output-format` (#8014)
- Document `lint.preview` and `format.preview` (#8032)
- Clarify that new rules should be added to `RuleGroup::Preview`. (#7989)

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