# Preview

Ruff includes an opt-in preview mode to provide an opportunity for community feedback and increase confidence that
changes are a net-benefit before enabling them for everyone.

Preview mode enables a collection of newer lint rules, fixes, and formatter style changes that are
considered experimental or unstable,.

## Enabling preview mode

Preview mode can be enabled with the `--preview` flag on the CLI or by setting `preview = true` in your Ruff
configuration file.

Preview mode can be configured separately for linting and formatting (requires Ruff v0.1.1+). To enable preview lint rules without preview style formatting:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    preview = true
    ```

=== "ruff.toml"

    ```toml
    [lint]
    preview = true
    ```

To enable preview style formatting without enabling any preview lint rules:

=== "pyproject.toml"

    ```toml
    [tool.ruff.format]
    preview = true
    ```

=== "ruff.toml"

    ```toml
    [format]
    preview = true
    ```

## Using rules that are in preview

If a rule is marked as preview, it can only be selected if preview mode is enabled. For example, consider a
hypothetical rule, `HYP001`. If `HYP001` were in preview, it would _not_ be enabled by adding following to your
config file:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    extend-select = ["HYP001"]
    ```

=== "ruff.toml"

    ```toml
    [lint]
    extend-select = ["HYP001"]
    ```

It also would _not_ be enabled by selecting the `HYP` category, like so:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    extend-select = ["HYP"]
    ```

=== "ruff.toml"

    ```toml
    [lint]
    extend-select = ["HYP"]
    ```

Similarly, it would _not_ be enabled via the `ALL` selector:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    select = ["ALL"]
    ```

=== "ruff.toml"

    ```toml
    [lint]
    select = ["ALL"]
    ```

However, it would be enabled in any of the above cases if you enabled preview in your configuration file:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    extend-select = ["HYP"]
    preview = true
    ```

=== "ruff.toml"

    ```toml
    [lint]
    extend-select = ["HYP"]
    preview = true
    ```

Or, if you provided the `--preview` CLI flag.

To see which rules are currently in preview, visit the [rules reference](rules.md).

## Selecting single preview rules

When preview mode is enabled, selecting rule categories or prefixes will include all preview rules that match.
If you'd prefer to opt-in to each preview rule individually, you can toggle the `explicit-preview-rules`
setting in your configuration file:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    preview = true
    explicit-preview-rules = true
    ```

=== "ruff.toml"

    ```toml
    [lint]
    preview = true
    explicit-preview-rules = true
    ```

In our previous example, `--select` with `ALL` `HYP`, `HYP0`, or `HYP00` would not enable `HYP001`. Each preview
rule will need to be selected with its exact code, e.g. `--select ALL,HYP001`.

If preview mode is not enabled, this setting has no effect.
