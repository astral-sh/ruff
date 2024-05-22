## Migrating From `ruff-lsp`

While `ruff server` supports the same feature set as [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp), migrating to
`ruff server` may require changes to your Ruff or language server configuration.

> \[!NOTE\]
>
> The [VS Code extension](https://github.com/astral-sh/ruff-vscode) settings include documentation to indicate which
> settings are supported by `ruff server`. As such, this migration guide is primarily targeted at editors that lack
> explicit documentation for `ruff server` settings, such as Helix or Neovim.

### Unsupported Settings

Several `ruff-lsp` settings are not supported by `ruff server`. These are, as follows:

- `format.args`
- `ignoreStandardLibrary`
- `interpreter`
- `lint.args`
- `lint.run`
- `logLevel`
- `path`

Note that some of these settings, like `interpreter` and `path`, are still accepted by the VS Code extension. `path`,
in particular, can be used to specify a dedicated binary to use when initializing `ruff server`. But the language server
itself will no longer accept such settings.

### New Settings

`ruff server` introduces several new settings that `ruff-lsp` does not have. These are, as follows:

- `configuration`: A path to a `ruff.toml` or `pyproject.toml` file to use for configuration. By default, Ruff will discover configuration for each project from the filesystem, mirroring the behavior of the Ruff CLI.
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

Several of these new settings are replacements for the now-unsupported `format.args` and `lint.args`. For example, if
you've been passing `--select=<RULES>` to `lint.args`, you can migrate to the new server by using `lint.select` with a
value of `["<RULES>"]`.

### Examples

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
