# Migrating from `ruff-lsp`

While `ruff server` supports the same feature set as [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp), migrating to
`ruff server` may require changes to your Ruff or language server configuration.

!!! note

    The [VS Code extension](https://github.com/astral-sh/ruff-vscode) settings include documentation to indicate which
    settings are supported by `ruff server`. As such, this migration guide is primarily targeted at editors that lack
    explicit documentation for `ruff server` settings, such as Helix or Neovim.

Refer to the [setup guide](setup.md) for instructions on how to configure your editor to use `ruff server`.

## Unsupported Settings

Several `ruff-lsp` settings are not supported by `ruff server`. These are, as follows:

- `lint.run`: This setting is no longer relevant for the native language server, which runs on every
    keystroke by default
- `lint.args`, `format.args`: These settings have been replaced by more granular settings in `ruff server` like [`lint.select`](settings.md#select), [`format.preview`](settings.md#format_preview),
    etc. along with the ability to provide a default configuration file using
    [`configuration`](settings.md#configuration)
- [`path`](settings.md#path), [`interpreter`](settings.md#interpreter): These settings are no longer
    accepted by the language server but are still used by the VS Code extension. Refer to their
    respective documentation for more information on how it's being used by the extension.
- `ignoreStandardLibrary`
- `showNotifications`

## New Settings

`ruff server` introduces several new settings that `ruff-lsp` does not have. These are, as follows:

- [`configuration`](settings.md#configuration)
- [`configurationPreference`](settings.md#configurationpreference)
- [`exclude`](settings.md#exclude)
- [`format.preview`](settings.md#format_preview)
- [`lineLength`](settings.md#linelength)
- [`lint.select`](settings.md#select)
- [`lint.extendSelect`](settings.md#extendselect)
- [`lint.ignore`](settings.md#ignore)
- [`lint.preview`](settings.md#lint_preview)

Several of these new settings are replacements for the now-unsupported `format.args` and `lint.args`. For example, if
you've been passing `--select=<RULES>` to `lint.args`, you can migrate to the new server by using `lint.select` with a
value of `["<RULES>"]`.

## Examples

Let's say you have these settings in VS Code:

```json
{
    "ruff.lint.args": "--select=E,F --line-length 80 --config ~/.config/custom_ruff_config.toml"
}
```

After enabling the native server, you can migrate your settings like so:

```json
{
    "ruff.configuration": "~/.config/custom_ruff_config.toml",
    "ruff.lineLength": 80,
    "ruff.lint.select": ["E", "F"]
}
```

Similarly, let's say you have these settings in Helix:

```toml
[language-server.ruff.config.lint]
args = "--select=E,F --line-length 80 --config ~/.config/custom_ruff_config.toml"
```

These can be migrated like so:

```toml
[language-server.ruff.config]
configuration = "~/.config/custom_ruff_config.toml"
lineLength = 80

[language-server.ruff.config.lint]
select = ["E", "F"]
```
