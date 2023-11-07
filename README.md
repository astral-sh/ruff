This repo is the ekr-ruff fork of [ruff format](https://github.com/rust-lang/rust).

**The Goal**

Allow more people and projects to use Ruff Format by letting them enforce their *existing* styles.

**Motivation**

- Leo can't use ruff format: it rewrites Leo's sentinel comments.
- Working on this project will teach me rust and improve Leo's rust importer.

**Token-related options**

- `--skip-string-normalization`: Leave the *contents* of string tokens unchanged.
- `--ignore-comment-regex`: Don't format the *interior* of comments whose *location* matches the regex.<br>
   For example, the regex `^\s*#@` would protect Leo's sentinel comments.

**Line-length options**

Adding or deleting newlines should be *options*, not mandates.

- `--no-line-breaks`: Never split lines.
- `--no-line-joins`: Never join lines.
- [Speculative]: Allow *leeway* for splitting/joining lines.<br>
  Don't split or join lines within a specified *range* of line lengths.

**Compatibility**

- Options that suppress default formatting have no impact on compatibility with Black.
- Such options should have minimal impact on Ruff's code or tests.

**Summary**

- Options are not the enemy: git, pylint, etc. have *hundreds* of options.
- Options will let more projects use ruff format, including [Leo](https://leo-editor.github.io/leo-editor/)!
