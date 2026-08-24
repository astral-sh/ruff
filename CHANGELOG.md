# Changelog

## 0.16.4

Released on 2026-08-20.

### Preview features

- \[`flake8-use-pathlib`\] Add autofix for `PTH116` ([#26460](https://github.com/astral-sh/ruff/pull/26460))
- \[`refurb`\] Restrict `delete-full-slice` to lists (`FURB131`) ([#27711](https://github.com/astral-sh/ruff/pull/27711))
- \[`refurb`\] Skip `FURB101` and `FURB103` when the `open` argument is a file descriptor ([#27643](https://github.com/astral-sh/ruff/pull/27643))

### Bug fixes

- Fix `InvalidInstruction` on Windows CPUs that do not support `POPCNT` ([#27803](https://github.com/astral-sh/ruff/pull/27803))
- \[`pyflakes`\] Emit semantic syntax errors in string type definitions as `F722` ([#27835](https://github.com/astral-sh/ruff/pull/27835))
- \[`pylint`\] Allow `os._exit` imports in `import-private-name` (`PLC2701`) ([#27738](https://github.com/astral-sh/ruff/pull/27738))

### Rule changes

- [syntax-errors] Align mixed t-string/bytes error message with CPython 3.14 ([#27766](https://github.com/astral-sh/ruff/pull/27766))
- \[`ruff`\] Add `ctypes.LittleEndianStructure` and related types to existing exception (`RUF012`) ([#27753](https://github.com/astral-sh/ruff/pull/27753))
- [syntax-errors] Detect duplicate keyword arguments ([#17804](https://github.com/astral-sh/ruff/pull/17804))
- [syntax-errors] Detect parameters declared `nonlocal` ([#27628](https://github.com/astral-sh/ruff/pull/27628))

### Server

- Offer display-only fixes and mark safe fixes preferred ([#27807](https://github.com/astral-sh/ruff/pull/27807))
- Support pull diagnostics for notebook cells ([#27779](https://github.com/astral-sh/ruff/pull/27779))

### Documentation

- Add default indicator to rules table ([#27724](https://github.com/astral-sh/ruff/pull/27724))
- Fix broken link to Python docs ([#27757](https://github.com/astral-sh/ruff/pull/27757))

### Other changes

- Fix s390x stacker assembly in release builds ([#27776](https://github.com/astral-sh/ruff/pull/27776))
- Guarantee minimum stack size when parsing a module, standalone expression, and suites ([#25464](https://github.com/astral-sh/ruff/pull/25464))
- Reduce configuration deserialization code size ([#27924](https://github.com/astral-sh/ruff/pull/27924))
- Check packed AST index bounds ([#27849](https://github.com/astral-sh/ruff/pull/27849))

### Contributors

- [@AbhinavMir](https://github.com/AbhinavMir)
- [@eduardorittner](https://github.com/eduardorittner)
- [@royb3](https://github.com/royb3)
- [@MichaReiser](https://github.com/MichaReiser)
- [@carljm](https://github.com/carljm)
- [@rosstitmarsh](https://github.com/rosstitmarsh)
- [@ntBre](https://github.com/ntBre)
- [@zaniebot](https://github.com/zaniebot)
- [@ewdurbin](https://github.com/ewdurbin)
- [@woodruffw](https://github.com/woodruffw)
- [@Sacrimento](https://github.com/Sacrimento)
- [@lakshayxi](https://github.com/lakshayxi)
- [@WhiteFox0-0](https://github.com/WhiteFox0-0)
- [@baltasarblanco](https://github.com/baltasarblanco)

## 0.16.3

Released on 2026-08-13.

### Preview features

- \[`pylint`\] Fix false negatives on negative numbers (`PLR6104`) ([#27251](https://github.com/astral-sh/ruff/pull/27251))
- \[`pyupgrade`\] Add rule to replace `while 1` with `while True` (`UP048`) ([#27190](https://github.com/astral-sh/ruff/pull/27190))

### Bug fixes

- \[`flake8-bandit`\] Also check keyword arguments (`S602`, `S603`, `S607`, `S609`) ([#27687](https://github.com/astral-sh/ruff/pull/27687))
- \[`pylint`\] Allow `continue` in `finally` on Python 3.8 ([#27626](https://github.com/astral-sh/ruff/pull/27626))
- \[`pylint`\] Fix `PLE1307` false positive with bools ([#27651](https://github.com/astral-sh/ruff/pull/27651))
- \[`pylint`\] Fix false positives and negatives with `%b` format character (`PLE1300`, `PLE1307`) ([#27560](https://github.com/astral-sh/ruff/pull/27560))
- \[`pylint`\] Improve handling of concatenated strings (`PLE1300`) ([#27659](https://github.com/astral-sh/ruff/pull/27659))

### Rule changes

- \[`numpy`\] Make `np.chararray` autofix backwards-compatible (`NPY201`) ([#27527](https://github.com/astral-sh/ruff/pull/27527))

### Performance

- Enable PGO for Linux x86-64 Ruff releases ([#27570](https://github.com/astral-sh/ruff/pull/27570))
- Enable PGO for Linux ARM64 Ruff releases ([#27574](https://github.com/astral-sh/ruff/pull/27574))
- Enable PGO for Windows x86-64 Ruff releases ([#27573](https://github.com/astral-sh/ruff/pull/27573))
- Enable PGO for macOS ARM64 Ruff releases ([#27572](https://github.com/astral-sh/ruff/pull/27572))
- Reduce `Expr` size to 64 bytes ([#27591](https://github.com/astral-sh/ruff/pull/27591))

### CLI

- Hyperlink rule codes in `ruff check --statistics` output ([#27646](https://github.com/astral-sh/ruff/pull/27646))

### Documentation

- \[`ruff`\] Also suggest `asyncio.TaskGroup` (`RUF006`) ([#27461](https://github.com/astral-sh/ruff/pull/27461))

### Other changes

- Use mimalloc v3 ([#27586](https://github.com/astral-sh/ruff/pull/27586))

### Contributors

- [@Andrej730](https://github.com/Andrej730)
- [@alonfaraj](https://github.com/alonfaraj)
- [@romero-deshaw](https://github.com/romero-deshaw)
- [@Avasam](https://github.com/Avasam)
- [@tjkuson](https://github.com/tjkuson)
- [@charliermarsh](https://github.com/charliermarsh)
- [@chirizxc](https://github.com/chirizxc)
- [@saberoueslati](https://github.com/saberoueslati)
- [@MichaReiser](https://github.com/MichaReiser)

## 0.16.2

Released on 2026-08-06.

### Bug fixes

- \[`flake8-pyi`\] Avoid false positives on `singledispatch` functions (`PYI041`) ([#27335](https://github.com/astral-sh/ruff/pull/27335))

### Server

- Register formatting capabilities dynamically to exclude TOML files ([#27332](https://github.com/astral-sh/ruff/pull/27332))

### Contributors

- [@MeGaGiGaGon](https://github.com/MeGaGiGaGon)
- [@charliermarsh](https://github.com/charliermarsh)
- [@epage](https://github.com/epage)
- [@sharkdp](https://github.com/sharkdp)
- [@ntBre](https://github.com/ntBre)

## 0.16.1

Released on 2026-07-30.

### Preview features

- Add an option to opt out of human-readable names ([#27160](https://github.com/astral-sh/ruff/pull/27160))
- \[`flake8-pytest-style`\] Make fixes safe by default and unsafe only when comments are present (`PT018`) ([#27201](https://github.com/astral-sh/ruff/pull/27201))
- \[`pyupgrade`\] Skip fix when a defaulted `TypeVar` precedes a non-defaulted one (`UP040`, `UP046`, `UP047`) ([#27133](https://github.com/astral-sh/ruff/pull/27133))
- \[`ruff`\] Fix false positive with unpacked arguments (`RUF065`) ([#26959](https://github.com/astral-sh/ruff/pull/26959))

### Bug fixes

- Bump `gen-lsp-types` to gracefully handle unknown enumeration values in LSP messages ([#27230](https://github.com/astral-sh/ruff/pull/27230))
- \[`flake8-bugbear`\] Mark `range` as immutable (`B008`) ([#27247](https://github.com/astral-sh/ruff/pull/27247))
- \[`flake8-comprehensions`\] NFKC-normalize keyword names in `C408` fix ([#26813](https://github.com/astral-sh/ruff/pull/26813))
- \[`flake8-return`\] Fix false positive when variable is read in `finally` clause (`RET504`) ([#25441](https://github.com/astral-sh/ruff/pull/25441))
- \[`pydocstyle`\] Skip section detection inside RST directive bodies (`D214`, `D405`, `D413`) ([#23635](https://github.com/astral-sh/ruff/pull/23635))
- \[`refurb`\] Parenthesize `yield` arguments in the `FURB192` fix ([#27192](https://github.com/astral-sh/ruff/pull/27192))

### Rule changes

- \[`flake8-pytest-style`\] Mark `PT022` fixes as unsafe ([#26440](https://github.com/astral-sh/ruff/pull/26440))
- \[`refurb`\] Mark fixes that remove unknown separators as unsafe (`FURB105`) ([#27200](https://github.com/astral-sh/ruff/pull/27200))

### Server

- Fix indexing of excluded nested Ruff workspaces ([#27303](https://github.com/astral-sh/ruff/pull/27303))
- Lint TOML files in the LSP ([#26862](https://github.com/astral-sh/ruff/pull/26862))

### Documentation

- Cover `pycon` Markdown formatting ([#27153](https://github.com/astral-sh/ruff/pull/27153))
- \[`flake8-bandit`\] Document `TYPE_CHECKING` exception (`S101`) ([#27004](https://github.com/astral-sh/ruff/pull/27004))
- \[`flake8-import-conventions`\] Document that `extend-aliases` can override default aliases ([#27191](https://github.com/astral-sh/ruff/pull/27191))
- \[`pylint`\] Add missing fix safety gotchas for `non-augmented-assignment` (`PLR6104`) ([#27250](https://github.com/astral-sh/ruff/pull/27250))

### Other changes

- Reduce syntax error noise by swallowing dedents like indents ([#27170](https://github.com/astral-sh/ruff/pull/27170))
- Vendor latest annotate-snippets ([#27033](https://github.com/astral-sh/ruff/pull/27033))

### Contributors

- [@bxff](https://github.com/bxff)
- [@anishgirianish](https://github.com/anishgirianish)
- [@Avasam](https://github.com/Avasam)
- [@epage](https://github.com/epage)
- [@LHMQ878](https://github.com/LHMQ878)
- [@MichaReiser](https://github.com/MichaReiser)
- [@ntBre](https://github.com/ntBre)
- [@HarshalPatel1972](https://github.com/HarshalPatel1972)
- [@mjpieters](https://github.com/mjpieters)
- [@joshuavetos](https://github.com/joshuavetos)
- [@jesco-absolute](https://github.com/jesco-absolut)
- [@vidigoat](https://github.com/vidigoat)
- [@baltasarblanco](https://github.com/baltasarblanco)
- [@ribru17](https://github.com/ribru17)
- [@oh-summy](https://github.com/oh-summy)
- [@Jayashanker-Padishala](https://github.com/Jayashanker-Padishala)

## 0.16.0

Released on 2026-07-23.

Check out the [blog post](https://astral.sh/blog/ruff-v0.16.0) for a migration
guide and overview of the changes!

### Breaking changes

- Ruff now enables a much larger set of rules by default (413, up from 59). See the blog post for
    more details and the new [Default Rules](https://docs.astral.sh/ruff/default-rules/) page for a
    full listing of the enabled rules. Note that this is primarily an expansion, but 18 of the more
    opinionated pycodestyle (`E`) and pyflakes (`F`) rules have been removed from the default set:
    `E401`, `E402`, `E701`, `E702`, `E703`, `E711`, `E712`, `E713`, `E714`, `E721`, `E731`, `E741`,
    `E742`, `E743`, `F403`, `F405`, `F406`, and `F722`.

- Ruff can now format Python code blocks in Markdown files and will do this by default. See the
    [documentation](https://docs.astral.sh/ruff/formatter/#markdown-code-formatting) for more details.

- Ruff now supports `ruff: ignore` comments at the ends of lines, like `noqa` comments, or on the line preceding a diagnostic. For example, these both suppress an [`unused-import`](https://docs.astral.sh/ruff/rules/unused-import/) (`F401`) diagnostic:

    ```py
    import math  # ruff: ignore[F401]

    # ruff: ignore[F401]
    import os
    ```

- Fixes are now shown in `check` and `format --check` output:

    ````console
    ❯ ruff format --check .
    unformatted: File would be reformatted
     --> try.md:1:1
      |
    1 | ```python
      - import   math
    2 + import math
    3 | ```
      |

    1 file would be reformatted
    ````

    This example also shows off the Markdown formatting.

- `format --check` now supports the same output formats as the linter, including the `github` and
    `gitlab` outputs for rendering annotations in CI:

    ```console
    ❯ ruff format --check --output-format github .
    ::error title=ruff (unformatted),file=try.md,line=2,col=8,endLine=2,endColumn=10::try.md:2:8: unformatted: File would be reformatted
    ```

    See the CLI help or [documentation](https://docs.astral.sh/ruff/settings/#output-format) for the
    full list of supported formats.

- The `filename`, `location`, `end_location`, `fix.edits[].location`, and `fix.edits[].end_location`
    fields in the JSON output format may now be `null` rather than defaulting to the empty string and
    row 1, column 1, respectively.

### Stabilization

The following rules have been stabilized and are no longer in preview:

- [`airflow3-incompatible-function-signature`](https://docs.astral.sh/ruff/rules/airflow3-incompatible-function-signature)
    (`AIR303`)
- [`missing-copyright-notice`](https://docs.astral.sh/ruff/rules/missing-copyright-notice)
    (`CPY001`)
- [`unnecessary-from-float`](https://docs.astral.sh/ruff/rules/unnecessary-from-float) (`FURB164`)
- [`sorted-min-max`](https://docs.astral.sh/ruff/rules/sorted-min-max) (`FURB192`)
- [`implicit-string-concatenation-in-collection-literal`](https://docs.astral.sh/ruff/rules/implicit-string-concatenation-in-collection-literal)
    (`ISC004`)
- [`log-exception-outside-except-handler`](https://docs.astral.sh/ruff/rules/log-exception-outside-except-handler)
    (`LOG004`)
- [`invalid-bool-return-type`](https://docs.astral.sh/ruff/rules/invalid-bool-return-type)
    (`PLE0304`)
- [`too-many-positional-arguments`](https://docs.astral.sh/ruff/rules/too-many-positional-arguments)
    (`PLR0917`)
- [`stop-iteration-return`](https://docs.astral.sh/ruff/rules/stop-iteration-return) (`PLR1708`)
- [`none-not-at-end-of-union`](https://docs.astral.sh/ruff/rules/none-not-at-end-of-union)
    (`RUF036`)
- [`access-annotations-from-class-dict`](https://docs.astral.sh/ruff/rules/access-annotations-from-class-dict)
    (`RUF063`)
- [`duplicate-entry-in-dunder-all`](https://docs.astral.sh/ruff/rules/duplicate-entry-in-dunder-all)
    (`RUF068`)

The following behaviors have been stabilized:

- [`blind-except`](https://docs.astral.sh/ruff/rules/blind-except) (`BLE001`) is now suppressed when
    the exception is logged via `logging` methods other than `critical`, `error` and `exception`.
- [`future-required-type-annotation`](https://docs.astral.sh/ruff/rules/future-required-type-annotation)
    (`FA102`) now checks for additional [PEP 585](https://peps.python.org/pep-0585/)-compatible
    APIs, such as those from `collections.abc`.
- [`f-string-in-get-text-func-call`](https://docs.astral.sh/ruff/rules/f-string-in-get-text-func-call)
    (`INT001`),
    [`format-in-get-text-func-call`](https://docs.astral.sh/ruff/rules/format-in-get-text-func-call)
    (`INT002`), and
    [`printf-in-get-text-func-call`](https://docs.astral.sh/ruff/rules/printf-in-get-text-func-call)
    (`INT003`) now check for additional common ways of using the `gettext` module, such as assigning
    it to `builtins._`.
- [`suspicious-url-open-usage`](https://docs.astral.sh/ruff/rules/suspicious-url-open-usage)
    (`S310`) now resolves local string literal bindings to avoid more false positives.
- [`snmp-insecure-version`](https://docs.astral.sh/ruff/rules/snmp-insecure-version) (`S508`) and
    [`snmp-weak-cryptography`](https://docs.astral.sh/ruff/rules/snmp-weak-cryptography) (`S509`) now
    support the recommended API from newer versions of PySNMP.
- [`typing-text-str-alias`](https://docs.astral.sh/ruff/rules/typing-text-str-alias) (`UP019`) now
    recognizes `typing_extensions.Text` in addition to `typing.Text`.

### Preview features

- \[`pyupgrade`\] Fix false positive with `TypeVar` default before Python 3.13 (`UP040`) ([#26888](https://github.com/astral-sh/ruff/pull/26888))

### Bug fixes

- \[`ruff`\] Fix missing check on unrecognized early bound (`RUF016`) ([#26986](https://github.com/astral-sh/ruff/pull/26986))

### Rule changes

- Insert a space after the colon in Ruff suppression comments ([#27123](https://github.com/astral-sh/ruff/pull/27123))

### Performance

- \[`pyupgrade`\] Speed up `unnecessary-future-import` (`UP010`) ([#27047](https://github.com/astral-sh/ruff/pull/27047))

### Documentation

- \[`ruff`\] Add missing period in "Why is this bad?" section (`RUF200`) ([#26930](https://github.com/astral-sh/ruff/pull/26930))
- \[`flake8-simplify`\] Clarify `os.environ` behavior on Windows (`SIM112`) ([#26972](https://github.com/astral-sh/ruff/pull/26972))
- \[`pydocstyle`\] Document fix safety (`D400`) ([#26971](https://github.com/astral-sh/ruff/pull/26971))

### Contributors

- [@jonathandung](https://github.com/jonathandung)
- [@Joosboy](https://github.com/Joosboy)
- [@MichaReiser](https://github.com/MichaReiser)
- [@Andrej730](https://github.com/Andrej730)
- [@ntBre](https://github.com/ntBre)
- [@zaniebot](https://github.com/zaniebot)

## 0.15.x

See [changelogs/0.15.x](./changelogs/0.15.x.md)

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
