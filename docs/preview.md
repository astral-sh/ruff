# Preview

Ruff includes an opt-in preview mode to provide an opportunity for community feedback and increase confidence that
changes are a net-benefit before enabling them for everyone.

Preview mode enables a collection of newer lint rules, fixes, and formatter style changes that are
considered experimental or unstable,.

## Enabling preview mode

Preview mode can be enabled with the `--preview` flag on the CLI or by setting `preview = true` in your Ruff
configuration file (e.g. `pyproject.toml`).

Preview mode can be configured separately for linting and formatting (requires Ruff v0.1.1+). To enable preview lint rules without preview style formatting:

```toml
[lint]
preview = true
```

To enable preview style formatting without enabling any preview lint rules:

```toml
[format]
preview = true
```

## Using rules that are in preview

If a rule is marked as preview, it can only be selected if preview mode is enabled. For example, consider a
hypothetical rule, `HYP001`. If `HYP001` were in preview, it would _not_ be enabled by adding following to your
`pyproject.toml`:

```toml
[tool.ruff]
extend-select = ["HYP001"]
```

It also would _not_ be enabled by selecting the `HYP` category, like so:

```toml
[tool.ruff]
extend-select = ["HYP"]
```

Similarly, it would _not_ be enabled via the `ALL` selector:

```toml
[tool.ruff]
select = ["ALL"]
```

However, it would be enabled in any of the above cases if you you enabled preview in your configuration file:

```toml
[tool.ruff]
extend-select = ["HYP"]
preview = true
```

Or, if you provided the `--preview` CLI flag.

To see which rules are currently in preview, visit the [rules reference](rules.md).

## Selecting single preview rules

When preview mode is enabled, selecting rule categories or prefixes will include all preview rules that match.
If you'd prefer to opt-in to each preview rule individually, you can toggle the `explicit-preview-rules`
setting in your `pyproject.toml`:

```toml
[tool.ruff]
preview = true
explicit-preview-rules = true
```

In our previous example, `--select` with `ALL` `HYP`, `HYP0`, or `HYP00` would not enable `HYP001`. Each preview
rule will need to be selected with its exact code, e.g. `--select ALL,HYP001`.

If preview mode is not enabled, this setting has no effect.

## Legacy behavior

Before the preview mode was introduced, new rules were added in a "nursery" category that required selection of
rules with their exact codes — similar to if `explicit-preview-rules` is enabled.

The nursery category has been deprecated and all rules in the nursery are now considered to be in preview.
For backwards compatibility, nursery rules are selectable with their exact codes without enabling preview mode.
However, this behavior will display a warning and support will be removed in a future release.
