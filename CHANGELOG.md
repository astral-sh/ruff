# Changelog

## 0.1.0

### Breaking changes
- Unsafe fixes are no longer displayed or applied without opt-in (#7769)
- Drop formatting specific rules from the default set (#7900)

### Rule changes
- Extend `reimplemented-starmap` (`FURB140`) to catch calls with a single and starred argument (#7768)
- Improve cases covered by `RUF015` (#7848)
- Update `SIM15` to allow `open` followed by `close` (#7916)
- Respect `msgspec.Struct` default-copy semantics in `RUF012` (#7786)
- Add `sqlalchemy` methods to `flake8-boolean-trap`` exclusion list (#7874)
- Add fix for `PLR1714` (#7910)
- Add fix for `PIE804` (#7884)
- Add fix for `PLC0208` (#7887)
- Add fix for `PYI055` (#7886)
- Update `non-pep695-type-alias` to require `--unsafe-fixes` outside of stub files (#7836)
- Improve fix message for `UP018` (#7913)

### Preview features
- Only show warnings for empty preview selectors when enabling rules (#7842)
- Add `unnecessary-key-check` to simplify `key in dct and dct[key]` to `dct.get(key)` (#7895)
- Add `assignment-in-assert` to prevent walrus expressions in assert statements (#7856)
- [`refurb`] Add `single-item-membership-test` (`FURB171`) (#7815)
- [`pylint`] Add `and-or-ternary` (`R1706`) (#7811)

### Configuration
- Add `unsafe-fixes` setting (#7769)
- Add `extend-safe-fixes` and `extend-unsafe-fixes` for promoting and demoting fixes (#7841)

### CLI
- Added `--unsafe-fixes` option for opt-in to display and apply unsafe fixes (#7769)
- Fix use of deprecated `--format` option in warning (#7837)
- Show changed files when running under `--check` (#7788)
- Write summary messages to stderr when fixing via stdin instead of omitting them (#7838)
- Update fix summary message in `check --diff` to include unsafe fix hints (#7790)
- Add notebook `cell` field to JSON output format (#7664)
- Rename applicability levels to `Safe`, `Unsafe`, and `Display` (#7843)

### Bug fixes
- Fix bug where f-strings were allowed in match pattern literal (#7857)
- Fix `SIM110` with a yield in the condition (#7801)
- Preserve trailing comments in `C414` fixes (#7775)
- Check sequence type before triggering `unnecessary-enumerate` `len` suggestion (#7781)
- Use correct start location for class/function clause header (#7802)
- Fix incorrect fixes for `SIM101` (#7798)
- Format comment before parameter default correctly (#7870)
- Fix `E251` false positive inside f-strings (#7894)
- Allow bindings to be created and referenced within annotations (#7885)
- Show per-cell diffs when analyzing notebooks over `stdin` (#7789)
- Avoid curly brace escape in f-string format spec (#7780)
- Fix lexing single-quoted f-string with multi-line format spec (#7787)
- Consider nursery rules to be in-preview for `ruff rule` (#7812)
- Report precise location for invalid conversion flag (#7809)
- Visit pattern match guard as a boolean test (#7911)
- Respect `--unfixable` in `ISC` rules (#7917)
- Fix edge case with `PIE804` (#7922)
- Show custom message in `PTH118` for `Path.joinpath` with starred arguments (#7852)
- Fix false negative in `outdated-version-block` when using greater than comparisons (#7920)
- Avoid converting f-strings within Django `gettext` calls (#7898)

### Documentation
- Document `reimplemented-starmap` performance effects (#7846)
- Default to following the system dark/light mode (#7888)
- Add documentation for fixes (#7901)
- Fix typo in docs of `PLR6301` (#7831)
- Update `UP038` docs to note that it results in slower code (#7872)
- crlf -> cr-lf (#7766)
- Add an example of an unsafe fix (#7924)
- Fix documented examples for `unnecessary-subscript-reversal` (#7774)
- Correct error in tuple example in ruff formatter docs (#7822)
- Add versioning policy to documentation (#7923)
- Fix invalid code in `FURB177` example (#7832)

### Formatter
- Less scary `ruff format` message (#7867)
- Remove spaces from import statements (#7859)
- Formatter quoting for f-strings with triple quotes (#7826)
- Update `ruff_python_formatter` generate.py comment (#7850)
- Document one-call chaining deviation (#7767)
- Allow f-string modifications in line-shrinking cases (#7818)
- Add trailing comment deviation to README (#7827)

### Playground
- Fix playground `Quick Fix` action (#7824)
