# Changelog

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
