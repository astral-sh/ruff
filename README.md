<!-- Begin section: Overview -->

This is the ekr-ruff fork of ruff: https://github.com/rust-lang/rust

# First, Do No Harm

This project has the following goals:

- Allow incremental formatting, reducing diffs.
- Option: Leave the *contents* of comment tokens unchanged.
- Option: Leave the *contents* of string tokens unchanged: `--skip-string-normalization`.
- Option: Suppress line breaks.
- Option: Suppress line joins.
