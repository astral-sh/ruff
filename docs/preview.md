# Preview

Ruff includes an opt-in preview mode to provide an opportunity for community feedback and increase confidence that
changes are a net-benefit before enabling them for everyone.

Preview mode enables a collection of newer rules and fixes that are considered experimental or unstable.

## Enabling preview mode

Preview mode can be enabled with the `--preview` flag on the CLI or by setting `preview = true` in your Ruff
configuration file (e.g. `pyproject.toml`).

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

## Legacy behavior

Before the preview mode was introduced, new rules were added in a "nursery" category that required selection of
rules with their exact code.

The nursery category has been deprecated and all rules in the nursery are now considered to be in preview. For backwards
compatibility, nursery rules are selectable with their exact codes without enabling preview mode but a warning will
be displayed.
