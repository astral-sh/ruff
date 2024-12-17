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

    Unlike [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp) and similar to the Ruff's CLI, the
    native language server requires user to explicitly include the Jupyter Notebook files in the set
    of files to lint and format. Refer to the [Jupyter Notebook discovery](https://docs.astral.sh/ruff/configuration/#jupyter-notebook-discovery)
    section on how to do this.

<img
src="https://astral.sh/static/GIF/v0.4.5/ipynb_editing_vscode.gif"
alt="Editing multiple Jupyter Notebook cells in VS Code"
/>

<img
src="https://astral.sh/static/GIF/v0.4.5/ipynb_range_format_vscode.gif"
alt="Formatting a selection within a Jupyter Notebook cell in VS Code"
/>
