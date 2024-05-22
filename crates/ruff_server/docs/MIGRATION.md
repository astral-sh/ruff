## Migrating From `ruff-lsp`

`ruff server`'s configuration is significantly different from `ruff-lsp`, and as such you may need to update your existing configuration.

> \[!NOTE\]
>
> The VS Code extension settings have documentation that will tell you whether `ruff server` supports a given setting.
> This migration guide may be more useful for the editors that do not have explicitly documented settings for the language server,
> such as Helix or Neovim.

### Unsupported Settings

Several `ruff-lsp` settings are not supported by `ruff server`. These are, as follows:

- `format.args`
- `ignoreStandardLibrary`
- `interpreter`
- `lint.args`
- `lint.run`
- `logLevel`
- `path`

Note that some of these settings, like `interpreter` and `path`, are still accepted by the VS Code extension. `path`, in particular, can be used to set the ruff binary
used to run `ruff server`. But the server itself will no longer accept these as settings.

### New Settings

`ruff server` introduces several new settings that `ruff-lsp` does not have. These are, as follows:

- `configuration`: This is a path to a `ruff.toml` or `pyproject.toml` file to use for configuration. By default, Ruff will discover configuration for each project from the filesystem, mirroring the behavior of the Ruff CLI.
- `configurationPreference`: Used to specify how you want to resolve server settings with local file configuration. The following values are available:
    - `"editorFirst"`: The default strategy - configuration set in the server settings takes priority over configuration set in `.toml` files.
    - `"filesystemFirst"`: An alternative strategy - configuration set in `.toml` files takes priority over configuration set in the server settings.
    - `"editorOnly"`: An alternative strategy - configuration set in `.toml` files is ignored entirely.
- `exclude`: Paths for the linter and formatter to ignore. See [the documentation](https://docs.astral.sh/ruff/settings/#exclude) for more details.
- `format.preview`: Enables [preview mode](https://docs.astral.sh/ruff/settings/#format_preview) for the formatter; enables unstable formatting.
- `lineLength`: The [line length](https://docs.astral.sh/ruff/settings/#line-length) used by the formatter and linter.
- `lint.select`: The rule codes to enable. Use `ALL` to enable all rules. See [the documentation](https://docs.astral.sh/ruff/settings/#lint_select) for more details.
- `lint.extendSelect`: Enables additional rule codes on top of existing configuration, instead of overriding it. Use `ALL` to enable all rules.
- `lint.ignore`: Sets rule codes to disable. See [the documentation](https://docs.astral.sh/ruff/settings/#lint_ignore) for more details.
- `lint.preview`: Enables [preview mode](https://docs.astral.sh/ruff/settings/#lint_preview) for the linter; enables unstable rules and fixes.

Several of these new settings are replacements for the now-unsupported `format.args` and `lint.args`. For example, if you have been passing in `--select=<RULES>`
to `lint.args`, you can migrate to the new server by using `lint.select` with a value of `["<RULES>"]`.

### Examples

Let's say you have these settings in VS Code:

```json
{
    "ruff.lint.args": "--select=E,F --line-length 80 --config ~/.config/custom_ruff_config.toml"
}
```

These can be migrated to the new language server like so:

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
