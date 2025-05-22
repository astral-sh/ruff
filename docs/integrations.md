# Integrations

## GitHub Actions

GitHub Actions has everything you need to run Ruff out-of-the-box:

```yaml
name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Python
        uses: actions/setup-python@v5
        with:
          python-version: "3.11"
      - name: Install dependencies
        run: |
          python -m pip install --upgrade pip
          pip install ruff
      # Update output format to enable automatic inline annotations.
      - name: Run Ruff
        run: ruff check --output-format=github .
```

Ruff can also be used as a GitHub Action via [`ruff-action`](https://github.com/astral-sh/ruff-action).

By default, `ruff-action` runs as a pass-fail test to ensure that a given repository doesn't contain
any lint rule violations as per its [configuration](configuration.md).
However, under-the-hood, `ruff-action` installs and runs `ruff` directly, so it can be used to
execute any supported `ruff` command (e.g., `ruff check --fix`).

`ruff-action` supports all GitHub-hosted runners, and can be used with any published Ruff version
(i.e., any version available on [PyPI](https://pypi.org/project/ruff/)).

To use `ruff-action`, create a file (e.g., `.github/workflows/ruff.yml`) inside your repository
with:

```yaml
name: Ruff
on: [ push, pull_request ]
jobs:
  ruff:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: astral-sh/ruff-action@v3
```

Alternatively, you can include `ruff-action` as a step in any other workflow file:

```yaml
      - uses: astral-sh/ruff-action@v3
```

`ruff-action` accepts optional configuration parameters via `with:`, including:

- `version`: The Ruff version to install (default: latest).
- `args`: The command-line arguments to pass to Ruff (default: `"check"`).
- `src`: The source paths to pass to Ruff (default: `[".", "src"]`).

For example, to run `ruff check --select B ./src` using Ruff version `0.8.0`:

```yaml
- uses: astral-sh/ruff-action@v3
  with:
    version: 0.8.0
    args: check --select B
    src: "./src"
```

## GitLab CI/CD

You can add the following configuration to `.gitlab-ci.yml` to run a `ruff format` in parallel with a `ruff check` compatible with GitLab's codequality report.

```yaml
.base_ruff:
  stage: build
  interruptible: true
  image:
    name: ghcr.io/astral-sh/ruff:0.11.11-alpine
  before_script:
    - cd $CI_PROJECT_DIR
    - ruff --version

Ruff Check:
  extends: .base_ruff
  script:
    - ruff check --output-format=gitlab > code-quality-report.json
  artifacts:
    reports:
      codequality: $CI_PROJECT_DIR/code-quality-report.json

Ruff Format:
  extends: .base_ruff
  script:
    - ruff format --diff
```

## pre-commit

Ruff can be used as a [pre-commit](https://pre-commit.com) hook via [`ruff-pre-commit`](https://github.com/astral-sh/ruff-pre-commit):

```yaml
- repo: https://github.com/astral-sh/ruff-pre-commit
  # Ruff version.
  rev: v0.11.11
  hooks:
    # Run the linter.
    - id: ruff
    # Run the formatter.
    - id: ruff-format
```

To enable lint fixes, add the `--fix` argument to the lint hook:

```yaml
- repo: https://github.com/astral-sh/ruff-pre-commit
  # Ruff version.
  rev: v0.11.11
  hooks:
    # Run the linter.
    - id: ruff
      args: [ --fix ]
    # Run the formatter.
    - id: ruff-format
```

To avoid running on Jupyter Notebooks, remove `jupyter` from the list of allowed filetypes:

```yaml
- repo: https://github.com/astral-sh/ruff-pre-commit
  # Ruff version.
  rev: v0.11.11
  hooks:
    # Run the linter.
    - id: ruff
      types_or: [ python, pyi ]
      args: [ --fix ]
    # Run the formatter.
    - id: ruff-format
      types_or: [ python, pyi ]
```

When running with `--fix`, Ruff's lint hook should be placed _before_ Ruff's formatter hook, and
_before_ Black, isort, and other formatting tools, as Ruff's fix behavior can output code changes
that require reformatting.

When running without `--fix`, Ruff's formatter hook can be placed before or after Ruff's lint hook.

(As long as your Ruff configuration avoids any [linter-formatter incompatibilities](formatter.md#conflicting-lint-rules),
`ruff format` should never introduce new lint errors, so it's safe to run Ruff's format hook _after_
`ruff check --fix`.)

## `mdformat`

[mdformat](https://mdformat.readthedocs.io/en/stable/users/plugins.html#code-formatter-plugins) is
capable of formatting code blocks within Markdown. The [`mdformat-ruff`](https://github.com/Freed-Wu/mdformat-ruff)
plugin enables mdformat to format Python code blocks with Ruff.


## Docker

Ruff provides a distroless Docker image including the `ruff` binary. The following tags are published:

- `ruff:latest`
- `ruff:{major}.{minor}.{patch}`, e.g., `ruff:0.6.6`
- `ruff:{major}.{minor}`, e.g., `ruff:0.6` (the latest patch version)

In addition, ruff publishes the following images:

<!-- prettier-ignore -->
- Based on `alpine:3.20`:
  - `ruff:alpine`
  - `ruff:alpine3.20`
- Based on `debian:bookworm-slim`:
  - `ruff:debian-slim`
  - `ruff:bookworm-slim`
- Based on `buildpack-deps:bookworm`:
  - `ruff:debian`
  - `ruff:bookworm`

As with the distroless image, each image is published with ruff version tags as
`ruff:{major}.{minor}.{patch}-{base}` and `ruff:{major}.{minor}-{base}`, e.g., `ruff:0.6.6-alpine`.

