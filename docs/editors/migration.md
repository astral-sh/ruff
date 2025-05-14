# Migrating from `ruff-lsp`

[`ruff-lsp`][ruff-lsp] is the [Language Server Protocol] implementation for Ruff to power the editor
integrations. It is written in Python and is a separate package from Ruff itself. The **native
server**, however, is the [Language Server Protocol] implementation which is **written in Rust** and
is available under the `ruff server` command. This guide is intended to help users migrate from
[`ruff-lsp`][ruff-lsp] to the native server.

!!! note

    The native server was first introduced in Ruff version `0.3.5`. It was marked as [beta in
    version `0.4.5`](https://astral.sh/blog/ruff-v0.4.5) and officially [stabilized in version
    `0.5.3`](https://github.com/astral-sh/ruff/releases/tag/0.5.3). It is recommended to use the
    latest version of Ruff to ensure the best experience.

The migration process involves any or all of the following:

1. Migrate [deprecated settings](#unsupported-settings) to the [new settings](#new-settings)
1. [Remove settings](#removed-settings) that are no longer supported
1. Update the `ruff` version

Read on to learn more about the unsupported or new settings, or jump to the [examples](#examples)
that enumerate some of the common settings and how to migrate them.

## Unsupported Settings

The following [`ruff-lsp`][ruff-lsp] settings are not supported by the native server:

- [`lint.run`](settings.md#lintrun): This setting is no longer relevant for the native language
    server, which runs on every keystroke by default.
- [`lint.args`](settings.md#lintargs), [`format.args`](settings.md#formatargs): These settings have
    been replaced by more granular settings in the native server like [`lint.select`](settings.md#select),
    [`format.preview`](settings.md#format_preview), etc. along with the ability to override any
    configuration using the [`configuration`](settings.md#configuration) setting.

The following settings are not accepted by the language server but are still used by the [VS Code extension].
Refer to their respective documentation for more information on how each is used by the extension:

- [`path`](settings.md#path)
- [`interpreter`](settings.md#interpreter)

## Removed Settings

Additionally, the following settings are not supported by the native server and should be removed:

- [`ignoreStandardLibrary`](settings.md#ignorestandardlibrary)
- [`showNotifications`](settings.md#shownotifications)

## New Settings

The native server introduces several new settings that [`ruff-lsp`][ruff-lsp] does not have:

- [`configuration`](settings.md#configuration)
- [`configurationPreference`](settings.md#configurationpreference)
- [`exclude`](settings.md#exclude)
- [`format.preview`](settings.md#format_preview)
- [`lineLength`](settings.md#linelength)
- [`lint.select`](settings.md#select)
- [`lint.extendSelect`](settings.md#extendselect)
- [`lint.ignore`](settings.md#ignore)
- [`lint.preview`](settings.md#lint_preview)

## Examples

All of the examples mentioned below are only valid for the [VS Code extension]. For other editors,
please refer to their respective documentation sections in the [settings](settings.md) page.

### Configuration file

If you've been providing a configuration file as shown below:

```json
{
    "ruff.lint.args": "--config ~/.config/custom_ruff_config.toml",
    "ruff.format.args": "--config ~/.config/custom_ruff_config.toml"
}
```

You can migrate to the new server by using the [`configuration`](settings.md#configuration) setting
like below which will apply the configuration to both the linter and the formatter:

```json
{
    "ruff.configuration": "~/.config/custom_ruff_config.toml"
}
```

### `lint.args`

If you're providing the linter flags by using `ruff.lint.args` like so:

```json
{
    "ruff.lint.args": "--select=E,F --unfixable=F401 --unsafe-fixes"
}
```

You can migrate to the new server by using the [`lint.select`](settings.md#select) and
[`configuration`](settings.md#configuration) setting like so:

```json
{
    "ruff.lint.select": ["E", "F"],
    "ruff.configuration": {
        "unsafe-fixes": true,
        "lint": {
            "unfixable": ["F401"]
        }
    }
}
```

The following options can be set directly in the editor settings:

- [`lint.select`](settings.md#select)
- [`lint.extendSelect`](settings.md#extendselect)
- [`lint.ignore`](settings.md#ignore)
- [`lint.preview`](settings.md#lint_preview)

The remaining options can be set using the [`configuration`](settings.md#configuration) setting.

### `format.args`

If you're also providing formatter flags by using `ruff.format.args` like so:

```json
{
    "ruff.format.args": "--line-length 80 --config='format.quote-style=double'"
}
```

You can migrate to the new server by using the [`lineLength`](settings.md#linelength) and
[`configuration`](settings.md#configuration) setting like so:

```json
{
    "ruff.lineLength": 80,
    "ruff.configuration": {
        "format": {
            "quote-style": "double"
        }
    }
}
```

The following options can be set directly in the editor settings:

- [`lineLength`](settings.md#linelength)
- [`format.preview`](settings.md#format_preview)

The remaining options can be set using the [`configuration`](settings.md#configuration) setting.

[language server protocol]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/
[ruff-lsp]: https://github.com/astral-sh/ruff-lsp
[vs code extension]: https://github.com/astral-sh/ruff-vscode
