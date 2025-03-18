# Settings

The Ruff Language Server provides a set of configuration options to customize its behavior
along with the ability to use an existing `pyproject.toml` or `ruff.toml` file to configure the
linter and formatter. This is done by providing these settings while initializing the server.
VS Code provides a UI to configure these settings, while other editors may require manual
configuration. The [setup](./setup.md) section provides instructions on where to place these settings
as per the editor.

## Top-level

### `configuration`

The `configuration` setting allows you to configure editor-specific Ruff behavior. This can be done
in one of the following ways:

1. **Configuration file path:** Specify the path to a `ruff.toml` or `pyproject.toml` file that
    contains the configuration. User home directory and environment variables will be expanded.
1. **Inline JSON configuration:** Directly provide the configuration as a JSON object.

!!! note "Added in Ruff `0.9.8`"

    The **Inline JSON configuration** option was introduced in Ruff `0.9.8`.

The default behavior, if `configuration` is unset, is to load the settings from the project's
configuration (a `ruff.toml` or `pyproject.toml` in the project's directory), consistent with when
running Ruff on the command-line.

The [`configurationPreference`](#configurationpreference) setting controls the precedence if both an
editor-provided configuration (`configuration`) and a project level configuration file are present.

#### Resolution order {: #configuration_resolution_order }

In an editor, Ruff supports three sources of configuration, prioritized as follows (from highest to
lowest):

1. **Specific settings:** Individual settings like [`lineLength`](#linelength) or
    [`lint.select`](#select) defined in the editor
1. [**`ruff.configuration`**](#configuration): Settings provided via the
    [`configuration`](#configuration) field (either a path to a configuration file or an inline
    configuration object)
1. **Configuration file:** Settings defined in a `ruff.toml` or `pyproject.toml` file in the
    project's directory (if present)

For example, if the line length is specified in all three sources, Ruff will use the value from the
[`lineLength`](#linelength) setting.

**Default value**: `null`

**Type**: `string`

**Example usage**:

_Using configuration file path:_

=== "VS Code"

    ```json
    {
        "ruff.configuration": "~/path/to/ruff.toml"
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        configuration = "~/path/to/ruff.toml"
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "configuration": "~/path/to/ruff.toml"
            }
          }
        }
      }
    }
    ```

_Using inline configuration:_

=== "VS Code"

    ```json
    {
        "ruff.configuration": {
            "lint": {
                "unfixable": ["F401"],
                "extend-select": ["TID251"],
                "flake8-tidy-imports": {
                    "banned-api": {
                        "typing.TypedDict": {
                            "msg": "Use `typing_extensions.TypedDict` instead",
                        }
                    }
                }
            },
            "format": {
                "quote-style": "single"
            }
        }
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        configuration = {
          lint = {
            unfixable = {"F401"},
            ["extend-select"] = {"TID251"},
            ["flake8-tidy-imports"] = {
              ["banned-api"] = {
                ["typing.TypedDict"] = {
                  msg = "Use `typing_extensions.TypedDict` instead"
                }
              }
            }
          },
          format = {
            ["quote-style"] = "single"
          }
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "configuration": {
                "lint": {
                  "unfixable": ["F401"],
                  "extend-select": ["TID251"],
                  "flake8-tidy-imports": {
                    "banned-api": {
                      "typing.TypedDict": {
                        "msg": "Use `typing_extensions.TypedDict` instead"
                      }
                    }
                  }
                },
                "format": {
                  "quote-style": "single"
                }
              }
            }
          }
        }
      }
    }
    ```

### `configurationPreference`

The strategy to use when resolving settings across VS Code and the filesystem. By default, editor
configuration is prioritized over `ruff.toml` and `pyproject.toml` files.

- `"editorFirst"`: Editor settings take priority over configuration files present in the workspace.
- `"filesystemFirst"`: Configuration files present in the workspace takes priority over editor
    settings.
- `"editorOnly"`: Ignore configuration files entirely i.e., only use editor settings.

**Default value**: `"editorFirst"`

**Type**: `"editorFirst" | "filesystemFirst" | "editorOnly"`

**Example usage**:

=== "VS Code"

    ```json
    {
        "ruff.configurationPreference": "filesystemFirst"
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          configurationPreference = "filesystemFirst"
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "configurationPreference": "filesystemFirst"
            }
          }
        }
      }
    }
    ```

### `exclude`

A list of file patterns to exclude from linting and formatting. See [the
documentation](https://docs.astral.sh/ruff/settings/#exclude) for more details.

**Default value**: `null`

**Type**: `string[]`

**Example usage**:

=== "VS Code"

    ```json
    {
        "ruff.exclude": ["**/tests/**"]
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          exclude = ["**/tests/**"]
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "exclude": ["**/tests/**"]
            }
          }
        }
      }
    }
    ```

### `lineLength`

The line length to use for the linter and formatter.

**Default value**: `null`

**Type**: `int`

**Example usage**:

=== "VS Code"

    ```json
    {
        "ruff.lineLength": 100
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          lineLength = 100
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "lineLength": 100
            }
          }
        }
      }
    }
    ```

### `fixAll`

Whether to register the server as capable of handling `source.fixAll` code actions.

**Default value**: `true`

**Type**: `bool`

**Example usage**:

=== "VS Code"

    ```json
    {
        "ruff.fixAll": false
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          fixAll = false
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "fixAll": false
            }
          }
        }
      }
    }
    ```

### `organizeImports`

Whether to register the server as capable of handling `source.organizeImports` code actions.

**Default value**: `true`

**Type**: `bool`

**Example usage**:

=== "VS Code"

    ```json
    {
        "ruff.organizeImports": false
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          organizeImports = false
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "organizeImports": false
            }
          }
        }
      }
    }
    ```

### `showSyntaxErrors`

_New in Ruff [v0.5.0](https://astral.sh/blog/ruff-v0.5.0#changes-to-e999-and-reporting-of-syntax-errors)_

Whether to show syntax error diagnostics.

**Default value**: `true`

**Type**: `bool`

**Example usage**:

=== "VS Code"

    ```json
    {
        "ruff.showSyntaxErrors": false
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          showSyntaxErrors = false
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "showSyntaxErrors": false
            }
          }
        }
      }
    }
    ```

### `logLevel`

The log level to use for the server.

**Default value**: `"info"`

**Type**: `"trace" | "debug" | "info" | "warn" | "error"`

**Example usage**:

=== "VS Code"

    ```json
    {
        "ruff.logLevel": "debug"
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          logLevel = "debug"
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "logLevel": "debug"
            }
          }
        }
      }
    }
    ```

### `logFile`

Path to the log file to use for the server.

If not set, logs will be written to stderr.

**Default value**: `null`

**Type**: `string`

**Example usage**:

=== "VS Code"

    ```json
    {
        "ruff.logFile": "~/path/to/ruff.log"
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          logFile = "~/path/to/ruff.log"
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "logFile": "~/path/to/ruff.log"
            }
          }
        }
      }
    }
    ```

## `codeAction`

Enable or disable code actions provided by the server.

### `disableRuleComment.enable`

Whether to display Quick Fix actions to disable rules via `noqa` suppression comments.

**Default value**: `true`

**Type**: `bool`

**Example usage**:

=== "VS Code"

    ```json
    {
        "ruff.codeAction.disableRuleComment.enable": false
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          codeAction = {
            disableRuleComment = {
              enable = false
            }
          }
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "codeAction": {
                "disableRuleComment": {
                  "enable": false
                }
              }
            }
          }
        }
      }
    }
    ```

### `fixViolation.enable`

Whether to display Quick Fix actions to autofix violations.

**Default value**: `true`

**Type**: `bool`

**Example usage**:

=== "VS Code"

    ```json
    {
        "ruff.codeAction.fixViolation.enable": false
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          codeAction = {
            fixViolation = {
              enable = false
            }
          }
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "codeAction": {
                "fixViolation": = {
                  "enable": false
                }
              }
            }
          }
        }
      }
    }
    ```

## `lint`

Settings specific to the Ruff linter.

### `enable` {: #lint_enable }

Whether to enable linting. Set to `false` to use Ruff exclusively as a formatter.

**Default value**: `true`

**Type**: `bool`

**Example usage**:

=== "VS Code"

    ```json
    {
        "ruff.lint.enable": false
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          lint = {
            enable = false
          }
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "lint": {
                "enable": false
              }
            }
          }
        }
      }
    }
    ```

### `preview` {: #lint_preview }

Whether to enable Ruff's preview mode when linting.

**Default value**: `null`

**Type**: `bool`

**Example usage**:

=== "VS Code"

    ```json
    {
        "ruff.lint.preview": true
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          lint = {
            preview = true
          }
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "lint": {
                "preview": true
              }
            }
          }
        }
      }
    }
    ```

### `select`

Rules to enable by default. See [the documentation](https://docs.astral.sh/ruff/settings/#lint_select).

**Default value**: `null`

**Type**: `string[]`

**Example usage**:

=== "VS Code"

    ```json
    {
        "ruff.lint.select": ["E", "F"]
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          lint = {
            select = {"E", "F"}
          }
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "lint": {
                "select": ["E", "F"]
              }
            }
          }
        }
      }
    }
    ```

### `extendSelect`

Rules to enable in addition to those in [`lint.select`](#select).

**Default value**: `null`

**Type**: `string[]`

**Example usage**:

=== "VS Code"

    ```json
    {
        "ruff.lint.extendSelect": ["W"]
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          lint = {
            extendSelect = {"W"}
          }
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "lint": {
                "extendSelect": ["W"]
              }
            }
          }
        }
      }
    }
    ```

### `ignore`

Rules to disable by default. See [the documentation](https://docs.astral.sh/ruff/settings/#lint_ignore).

**Default value**: `null`

**Type**: `string[]`

**Example usage**:

=== "VS Code"

    ```json
    {
        "ruff.lint.ignore": ["E4", "E7"]
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          lint = {
            ignore = {"E4", "E7"}
          }
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "lint": {
                "ignore": ["E4", "E7"]
              }
            }
          }
        }
      }
    }
    ```

## `format`

Settings specific to the Ruff formatter.

### `preview` {: #format_preview }

Whether to enable Ruff's preview mode when formatting.

**Default value**: `null`

**Type**: `bool`

**Example usage**:

=== "VS Code"

    ```json
    {
        "ruff.format.preview": true
    }
    ```

=== "Neovim"

    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          format = {
            preview = true
          }
        }
      }
    }
    ```

=== "Zed"

    ```json
    {
      "lsp": {
        "ruff": {
          "initialization_options": {
            "settings": {
              "format": {
                "preview": true
              }
            }
          }
        }
      }
    }
    ```

## VS Code specific

Additionally, the Ruff extension provides the following settings specific to VS Code. These settings
are not used by the language server and are only relevant to the extension.

### `enable`

Whether to enable the Ruff extension. Modifying this setting requires restarting VS Code to take effect.

**Default value**: `true`

**Type**: `bool`

**Example usage**:

```json
{
    "ruff.enable": false
}
```

### `format.args`

!!! warning "Deprecated"

    This setting is only used by [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp) which is
    deprecated in favor of the native language server. Refer to the [migration
    guide](migration.md) for more information.

_**This setting is not used by the native language server.**_

Additional arguments to pass to the Ruff formatter.

**Default value**: `[]`

**Type**: `string[]`

**Example usage**:

```json
{
    "ruff.format.args": ["--line-length", "100"]
}
```

### `ignoreStandardLibrary`

!!! warning "Deprecated"

    This setting is only used by [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp) which is
    deprecated in favor of the native language server. Refer to the [migration
    guide](migration.md) for more information.

_**This setting is not used by the native language server.**_

Whether to ignore files that are inferred to be part of the Python standard library.

**Default value**: `true`

**Type**: `bool`

**Example usage**:

```json
{
    "ruff.ignoreStandardLibrary": false
}
```

### `importStrategy`

Strategy for loading the `ruff` executable.

- `fromEnvironment` finds Ruff in the environment, falling back to the bundled version
- `useBundled` uses the version bundled with the extension

**Default value**: `"fromEnvironment"`

**Type**: `"fromEnvironment" | "useBundled"`

**Example usage**:

```json
{
    "ruff.importStrategy": "useBundled"
}
```

### `interpreter`

A list of paths to Python interpreters. Even though this is a list, only the first interpreter is
used.

This setting depends on the [`ruff.nativeServer`](#nativeserver) setting:

- If using the native server, the interpreter is used to find the `ruff` executable when
    [`ruff.importStrategy`](#importstrategy) is set to `fromEnvironment`.
- Otherwise, the interpreter is used to run the `ruff-lsp` server.

**Default value**: `[]`

**Type**: `string[]`

**Example usage**:

```json
{
    "ruff.interpreter": ["/home/user/.local/bin/python"]
}
```

### `lint.args`

!!! warning "Deprecated"

    This setting is only used by [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp) which is
    deprecated in favor of the native language server. Refer to the [migration
    guide](migration.md) for more information.

_**This setting is not used by the native language server.**_

Additional arguments to pass to the Ruff linter.

**Default value**: `[]`

**Type**: `string[]`

**Example usage**:

```json
{
    "ruff.lint.args": ["--config", "/path/to/pyproject.toml"]
}
```

### `lint.run`

!!! warning "Deprecated"

    This setting is only used by [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp) which is
    deprecated in favor of the native language server. Refer to the [migration
    guide](migration.md) for more information.

_**This setting is not used by the native language server.**_

Run Ruff on every keystroke (`onType`) or on save (`onSave`).

**Default value**: `"onType"`

**Type**: `"onType" | "onSave"`

**Example usage**:

```json
{
    "ruff.lint.run": "onSave"
}
```

### `nativeServer`

Whether to use the native language server, [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp) or
automatically decide between the two based on the Ruff version and extension settings.

- `"on"`: Use the native language server. A warning will be displayed if deprecated settings are
    detected.
- `"off"`: Use [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp). A warning will be displayed if
    settings specific to the native server are detected.
- `"auto"`: Automatically select between the native language server and
    [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp) based on the following conditions:
    1. If the Ruff version is >= `0.5.3`, use the native language server unless any deprecated
        settings are detected. In that case, show a warning and use
        [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp) instead.
    1. If the Ruff version is < `0.5.3`, use [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp). A
        warning will be displayed if settings specific to the native server are detected.
- `true`: Same as `on`
- `false`: Same as `off`

**Default value**: `"auto"`

**Type**: `"on" | "off" | "auto" | true | false`

**Example usage**:

```json
{
    "ruff.nativeServer": "on"
}
```

### `path`

A list of path to `ruff` executables.

The first executable in the list which is exists is used. This setting takes precedence over the
[`ruff.importStrategy`](#importstrategy) setting.

**Default value**: `[]`

**Type**: `string[]`

**Example usage**:

```json
{
    "ruff.path": ["/home/user/.local/bin/ruff"]
}
```

### `showNotifications`

!!! warning "Deprecated"

    This setting is only used by [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp) which is
    deprecated in favor of the native language server. Refer to the [migration
    guide](migration.md) for more information.

Setting to control when a notification is shown.

**Default value**: `"off"`

**Type**: `"off" | "onError" | "onWarning" | "always"`

**Example usage**:

```json
{
    "ruff.showNotifications": "onWarning"
}
```

### `trace.server`

The trace level for the language server. Refer to the [LSP
specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#traceValue)
for more information.

**Default value**: `"off"`

**Type**: `"off" | "messages" | "verbose"`

**Example usage**:

```json
{
    "ruff.trace.server": "messages"
}
```
