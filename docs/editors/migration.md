# Migrating from `ruff-lsp`

To provide some context, [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp) is the LSP implementation for Ruff to power the editor
integrations which is written in Python and is a separate package from Ruff itself. The **native
server** is the LSP implementation which is written in Rust and is available under the `ruff server`
command. This guide is intended to help users migrate from
[`ruff-lsp`](https://github.com/astral-sh/ruff-lsp) to the native server.

!!! note

    The native server was first introduced in Ruff version `0.3.5`. It was marked as beta in version
    `0.4.5` and officially stabilized in version `0.5.3`. It is recommended to use the latest
    version of Ruff to ensure the best experience.

The migration process involves any or all of the following:

1. Migrate [deprecated settings](#unsupported-settings) to the [new settings](#new-settings)
1. [Remove settings](#removed-settings) that are no longer supported
1. Update the `ruff` version

## Unsupported Settings

The following [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp) settings are not supported by `ruff server`:

- `lint.run`: This setting is no longer relevant for the native language server, which runs on every
    keystroke by default.
- `lint.args`, `format.args`: These settings have been replaced by more granular settings in `ruff server` like [`lint.select`](settings.md#select), [`format.preview`](settings.md#format_preview),
    etc. along with the ability to provide a default configuration file using [`configuration`](settings.md#configuration).

The following settings are not accepted by the language server but are still used by the VS Code
extension. Refer to their respective documentation for more information on how it's being used by
the extension:

- [`path`](settings.md#path)
- [`interpreter`](settings.md#interpreter)

## Removed Settings

Additionally, the following settings are not supported by the native server, they should be removed:

- `ignoreStandardLibrary`
- `showNotifications`

## New Settings

`ruff server` introduces several new settings that [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp) does not have. These are, as follows:

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
