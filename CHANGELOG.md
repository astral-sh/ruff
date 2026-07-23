# Changelog

## 0.16.0

Released on 2026-07-23.

Check out the [blog post](https://astral.sh/blog/ruff-v0.16.0) for a migration
guide and overview of the changes!

### Breaking changes

- Ruff now enables a much larger set of rules by default (413, up from 59). See the blog post for
    more details and the new [Default Rules](https://docs.astral.sh/ruff/default-rules/) page for a
    full listing of the enabled rules.

- Ruff can now format Python code blocks in Markdown files and will do this by default. See the
    [documentation](https://docs.astral.sh/ruff/formatter/#markdown-code-formatting) for more details.

- Ruff now supports `ruff: ignore` comments the ends of lines, like `noqa` comments`, or on the line preceding a diagnostic. For example, these both suppress an [`unused-import`(`F401\`)\](<https://docs.astral.sh/ruff/rules/unused-import/>) diagnostic:

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
    (`FA102`) now checks for additional [PEP 585]-compatible APIs, such as those from
    `collections.abc`.
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

- Add missing period in `RUF200` "Why is this bad?" documentation portion ([#26930](https://github.com/astral-sh/ruff/pull/26930))
- \[`flake8-simplify`\] Clarify `os.environ` behavior on Windows (`SIM112`) ([#26972](https://github.com/astral-sh/ruff/pull/26972))
- \[`pydocstyle`\] Document fix safety (`D400`) ([#26971](https://github.com/astral-sh/ruff/pull/26971))

### Other changes

- Use Namespace runners for Windows ([#27101](https://github.com/astral-sh/ruff/pull/27101))
- [ty] Fix memory report detail outcome icons ([#26929](https://github.com/astral-sh/ruff/pull/26929))

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
