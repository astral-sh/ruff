# Features

This section provides a detailed overview of the features provided by the Ruff Language Server.

## Diagnostic Highlighting

Provide diagnostics for your Python code in real-time.

<img
src="https://astral.sh/static/GIF/v0.4.5/violation_hx.gif"
alt="Editing a file in Helix"
/>

## Dynamic Configuration

The server dynamically refreshes the diagnostics when a configuration file is changed in the
workspace, whether it's a `pyproject.toml`, `ruff.toml`, or `.ruff.toml` file.

The server relies on the file watching capabilities of the editor to detect changes to these files.
If an editor does not support file watching, the server will not be able to detect
changes to the configuration file and thus will not refresh the diagnostics.

<img
src="https://astral.sh/static/GIF/v0.4.5/config_reload_vscode.gif"
alt="Editing a `pyproject.toml` configuration file in VS Code"
/>

## Formatting

Provide code formatting for your Python code. The server can format an entire document or a specific
range of lines.

The VS Code extension provides the `Ruff: Format Document` command to format an entire document.
In VS Code, the range formatting can be triggered by selecting a range of lines, right-clicking, and
selecting `Format Selection` from the context menu.

<img
src="https://astral.sh/static/GIF/v0.4.5/format_vscode.gif"
alt="Formatting a document in VS Code"
/>

## Markdown formatting

!!! note "Preview mode required"

    Markdown formatting is currently only available in [preview mode](../preview.md#preview). You must
    enable [format.preview](./settings.md#format_preview) for Markdown formatting to work in editors.

Ruff can format Python code blocks within Markdown files. This formats fenced code blocks
(```` ```py ````, ```` ```python ````, etc.) while leaving the rest of the document unchanged.
See the [formatter documentation](../formatter.md#markdown-code-formatting) for details on which
code blocks are supported.

### Enabling in VS Code

1. Enable preview mode for the formatter in your settings:

   ```json
   {
     "ruff.format.preview": true
   }
   ```

2. Set Ruff as the default formatter for Markdown files:

   ```json
   {
     "[markdown]": {
       "editor.defaultFormatter": "charliermarsh.ruff"
     }
   }
   ```

3. Optionally, enable format on save for Markdown:

   ```json
   {
     "[markdown]": {
       "editor.defaultFormatter": "charliermarsh.ruff",
       "editor.formatOnSave": true
     }
   }
   ```

### Conflicts with other formatters

If you use other Markdown formatters (e.g., Prettier, Markdown All in One), they may conflict with
Ruff. VS Code only uses one default formatter per language. Recommended approaches:

**Option 1: Use Ruff exclusively for Markdown** (recommended for Python-focused projects)

Set Ruff as the default formatter for Markdown. Ruff only formats Python code blocks and leaves
other Markdown content unchanged:

```json
{
  "[markdown]": {
    "editor.defaultFormatter": "charliermarsh.ruff"
  }
}
```

**Option 2: Use Ruff for Python, another formatter for Markdown**

If you prefer a general-purpose Markdown formatter for prose, disable Ruff for Markdown and use
 Ruff only for Python files:

```json
{
  "[python]": {
    "editor.defaultFormatter": "charliermarsh.ruff"
  },
  "[markdown]": {
    "editor.defaultFormatter": "esbenp.prettier-vscode"
  }
}
```

**Option 3: Format with both (manual)**

Use Ruff for Markdown when you want to format Python code blocks: run `Ruff: Format Document` or
`Format Document` with Ruff selected. Use your other formatter for general Markdown formatting.

### Configuration

Markdown formatting respects your Ruff configuration. Use `pyproject.toml` or `ruff.toml` to customize
formatting behavior:

=== "pyproject.toml"

    ```toml
    [tool.ruff]
    line-length = 100

    [tool.ruff.format]
    quote-style = "single"
    ```

=== "ruff.toml"

    ```toml
    line-length = 100

    [format]
    quote-style = "single"
    ```

To disable Markdown formatting for specific files, add them to `extend-exclude`:

=== "pyproject.toml"

    ```toml
    [tool.ruff]
    extend-exclude = ["docs/**/*.md"]
    ```

=== "ruff.toml"

    ```toml
    extend-exclude = ["docs/**/*.md"]
    ```

To format Markdown files with extensions other than `.md` (e.g., `.mdx`, `.qmd`), configure
[extension mappings](../settings.md#extension):

=== "pyproject.toml"

    ```toml
    [tool.ruff]
    extension = { mdx = "markdown", qmd = "markdown" }
    ```

=== "ruff.toml"

    ```toml
    extension = { mdx = "markdown", qmd = "markdown" }
    ```

## Code Actions

Code actions are context-sensitive suggestions that can help you fix issues in your code. They are
usually triggered by a shortcut or by clicking a light bulb icon in the editor. The Ruff Language
Server provides the following code actions:

- Apply a quick fix for a diagnostic that has a fix available (e.g., removing an unused import).
- Ignore a diagnostic with a `# noqa` comment.
- Apply all quick fixes available in the document.
- Organize imports in the document.

<img
src="https://astral.sh/static/GIF/v0.4.5/code_action_hx.gif"
alt="Applying a quick fix in Helix"
/>

You can even run these actions on-save. For example, to fix all issues and organize imports on save
in VS Code, add the following to your `settings.json`:

```json
{
  "[python]": {
    "editor.codeActionsOnSave": {
      "source.fixAll.ruff": "explicit",
      "source.organizeImports.ruff": "explicit"
    }
  }
}
```

### Fix Safety

Ruff's automatic fixes are labeled as "safe" and "unsafe". By default, the "Fix all" action will not
apply unsafe fixes. However, unsafe fixes can be applied manually with the "Quick fix" action.
Application of unsafe fixes when using "Fix all" can be enabled by setting `unsafe-fixes = true` in
your Ruff configuration file.

See the [Ruff fix documentation](https://docs.astral.sh/ruff/linter/#fix-safety) for more details on
how fix safety works.

## Hover

The server can provide the rule documentation when focusing over a NoQA code in the comment.
Focusing is usually hovering with a mouse, but can also be triggered with a shortcut.

<img
src="https://astral.sh/static/GIF/v0.4.5/hover_vscode.gif"
alt="Hovering over a noqa code in VS Code"
/>

## Jupyter Notebook

Similar to Ruff's CLI, the Ruff Language Server fully supports Jupyter Notebook files with all the
capabilities available to Python files.

!!! note

    Ruff has built-in support for Jupyter Notebooks. The native language server will discover and
    lint or format `.ipynb` files by default as of version 0.6.0. Refer to the
    [Jupyter Notebook discovery](../configuration.md#jupyter-notebook-discovery) section for
    more details.

<img
src="https://astral.sh/static/GIF/v0.4.5/ipynb_editing_vscode.gif"
alt="Editing multiple Jupyter Notebook cells in VS Code"
/>

<img
src="https://astral.sh/static/GIF/v0.4.5/ipynb_range_format_vscode.gif"
alt="Formatting a selection within a Jupyter Notebook cell in VS Code"
/>
