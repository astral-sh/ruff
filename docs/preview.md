# Preview

Ruff includes an opt-in preview mode to provide an opportunity for community feedback and increase confidence that
changes are a net-benefit before enabling them for everyone.

Preview mode enables a collection of unstable features such as new lint rules and fixes, formatter style changes, interface updates, and more. Warnings about deprecated features may turn into errors when using preview mode.

Enabling preview mode does not on its own enable all preview rules. See the [rules section](#using-rules-that-are-in-preview) for details on selecting preview rules.

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

=== "CLI"

    ```console
    ruff check --preview
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

=== "CLI"

    ```console
    ruff format --preview
    ```

## Using rules that are in preview

If a rule is marked as preview, it can only be selected if preview mode is enabled. For example, consider a
hypothetical rule, `HYP001`. If `HYP001` were in preview, it would _not_ be enabled by adding it to the selected rule set.

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

=== "CLI"

    ```console
    ruff check --extend-select HYP001
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

=== "CLI"

    ```console
    ruff check --extend-select HYP
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

=== "CLI"

    ```console
    ruff check --select ALL
    ```

However, it _would_ be enabled in any of the above cases if you enabled preview mode:

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

=== "CLI"

    ```console
    ruff check --extend-select HYP --preview
    ```

To see which rules are currently in preview, visit the [rules reference](rules.md).

## Selecting single preview rules

When preview mode is enabled, selecting rule categories or prefixes will include all preview rules that match.
If you'd prefer to opt in to each preview rule individually, you can toggle the `explicit-preview-rules`
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

## Deprecated rules

When preview mode is enabled, deprecated rules will be disabled. If a deprecated rule is selected explicitly, an
error will be raised. Deprecated rules will not be included if selected via a rule category or prefix.
