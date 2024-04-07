# Ruff Formatter

The Ruff formatter is an extremely fast Python code formatter that ships as part of the `ruff`
CLI.

## Goals

The formatter is designed to be a drop-in replacement for [Black](https://github.com/psf/black),
but with an excessive focus on performance and direct integration with Ruff.

Specifically, the formatter is intended to emit near-identical output when run over Black-formatted
code. When run over extensive Black-formatted projects like Django and Zulip, > 99.9% of lines
are formatted identically. When migrating an existing project from Black to Ruff, you should expect
to see a few differences on the margins, but the vast majority of your code should be unchanged.

If you identify deviations in your project, spot-check them against the [intentional deviations](https://docs.astral.sh/ruff/formatter/black/)
enumerated below, as well as the [unintentional deviations](https://github.com/astral-sh/ruff/issues?q=is%3Aopen+is%3Aissue+label%3Aformatter)
filed in the issue tracker. If you've identified a new deviation, please [file an issue](https://github.com/astral-sh/ruff/issues/new).

When run over _non_-Black-formatted code, the formatter makes some different decisions than Black,
and so more deviations should be expected, especially around the treatment of end-of-line comments.
For details, see [Black compatibility](https://docs.astral.sh/ruff/formatter/#black-compatibility).

## Getting started

The Ruff formatter is available as of Ruff v0.1.2. Head to [The Ruff Formatter](https://docs.astral.sh/ruff/formatter/) for usage instructions and a comparison to Black.
