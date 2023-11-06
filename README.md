This is the ekr-ruff fork of [Ruff Format](https://github.com/rust-lang/rust). This project has the following goals:

## The Goal

Allow more people and projects to use Ruff Format by letting them enforce their *existing* styles.

## Protecting tokens

- `--skip-string-normalization`: Leave the *contents* of string tokens unchanged.
- `--ignore-comment-regex`: Don't format the *interior* of comments whose *location* matches the regex.<br>
   For example, the regex `^\s*#@` would protect Leo's sentinel comments.

## Flexible line lengths

Black sometimes splits lines poorly. Suffering poor line breaks should be *optional*.

- `--no-line-breaks`: Never split lines.
- `--no-line-joins`: Never join lines.
- [Speculative]: Allow *leeway* for splitting/joining lines.<br>
  Don't split or join lines within a specified *range* of line lengths.

##Summary

Options are not the enemy. Tools like git, pylint, etc. have *hundreds* of options.

Options will let more people and projects use Ruff Format, including [Leo](https://leo-editor.github.io/leo-editor/)!

Options that suppress default formatting have no impact on compatibility with Black. Such options should have minimal impact of Ruff's code or tests.