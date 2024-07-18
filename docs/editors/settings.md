# Settings

The Ruff Language Server provides a set of configuration options to customize its behavior
along with the ability to use an existing `pyproject.toml` or `ruff.toml` file to configure the
linter and formatter. This is done by providing these settings while initializing the server.
VS Code provides a UI to configure these settings, while other editors may require manual
configuration. The [setup](./setup.md) section provides instructions on where to place these settings
as per the editor.

## Top-level

### `configuration`

Path to a `ruff.toml` or `pyproject.toml` file to use for configuration.

By default, Ruff will discover configuration for each project from the filesystem, mirroring the
behavior of the Ruff CLI.

**Default value**: `null`

**Type**: `string`

**Example usage**:

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
        settings = {
          configuration = "~/path/to/ruff.toml"
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

### `extendIgnore`

Rules to disable in addition to those in [`lint.ignore`](#ignore).

**Default value**: `null`

**Type**: `string[]`

**Example usage**:

=== "VS Code"
    ```json
    {
        "ruff.lint.extendIgnore": ["W1"]
    }
    ```

=== "Neovim"
    ```lua
    require('lspconfig').ruff.setup {
      init_options = {
        settings = {
          lint = {
            extendIgnore = {"W1"}
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

## VS Code specific

The extension provides additional settings to control the behavior of the Ruff extension in VS Code.
The detailed documentation for these settings can be found in the UI of the settings editor in VS
Code.

Refer to the [VS Code extension documentation](https://github.com/astral-sh/ruff-vscode#settings)
for more information.
