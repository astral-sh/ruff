This is the ekr-ruff fork of [ruff](https://github.com/rust-lang/rust). This project has the following goals:

## First, Do No Harm

- Allow incremental formatting, thereby huge reducing diffs.
- Let projects enforce *existing* styles and styles *of their own choosing*.

## Protecting tokens

- `--skip-string-normalization`: Leave the *contents* of string tokens unchanged.
- `--ignore-comment-regex`:<br>
   Don't format the *interior* of comments whose *location* matches the regex.<br>
   For example, the regex `^\s*#@` would protect Leo's sentinel comments.

## Flexible line lengths

Black sometimes splits lines poorly. Suffering poor line breaks should be *optional*.

- `--no-line-breaks`: Never split lines.
- `--no-line-joins`: Never join lines.
- [Speculative]: Allow *leeway* for splitting/joining lines.
  Don't split or join lines within a specified *range* of line lengths.

## Options are not the enemy

Tools like git, pylint, etc. have *hundreds* of options.