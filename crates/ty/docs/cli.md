<!-- WARNING: This file is auto-generated (cargo dev generate-all). Edit the doc comments in 'crates/ty/src/args.rs' if you want to change anything here. -->

# CLI Reference

## ty

An extremely fast Python type checker.

<h3 class="cli-reference">Usage</h3>

```
ty <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="#ty-check"><code>ty check</code></a></dt><dd><p>Check a project for type errors</p></dd>
<dt><a href="#ty-server"><code>ty server</code></a></dt><dd><p>Start the language server</p></dd>
<dt><a href="#ty-version"><code>ty version</code></a></dt><dd><p>Display ty's version</p></dd>
<dt><a href="#ty-help"><code>ty help</code></a></dt><dd><p>Print this message or the help of the given subcommand(s)</p></dd>
</dl>

## ty check

Check a project for type errors

<h3 class="cli-reference">Usage</h3>

```
ty check [OPTIONS] [PATH]...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="ty-check--paths"><a href="#ty-check--paths"><code>PATHS</code></a></dt><dd><p>List of files or directories to check [default: the project root]</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="ty-check--color"><a href="#ty-check--color"><code>--color</code></a> <i>when</i></dt><dd><p>Control when colored output is used</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Display colors if the output goes to an interactive terminal</li>
<li><code>always</code>:  Always display colors</li>
<li><code>never</code>:  Never display colors</li>
</ul></dd><dt id="ty-check--config"><a href="#ty-check--config"><code>--config</code></a>, <code>-c</code> <i>config-option</i></dt><dd><p>A TOML <code>&lt;KEY&gt; = &lt;VALUE&gt;</code> pair (such as you might find in a <code>ty.toml</code> configuration file)
overriding a specific configuration option.</p>
<p>Overrides of individual settings using this option always take precedence
over all configuration files.</p>
</dd><dt id="ty-check--config-file"><a href="#ty-check--config-file"><code>--config-file</code></a> <i>path</i></dt><dd><p>The path to a <code>ty.toml</code> file to use for configuration.</p>
<p>While ty configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>TY_CONFIG_FILE</code> environment variable.</p></dd><dt id="ty-check--error"><a href="#ty-check--error"><code>--error</code></a> <i>rule</i></dt><dd><p>Treat the given rule as having severity 'error'. Can be specified multiple times.</p>
</dd><dt id="ty-check--error-on-warning"><a href="#ty-check--error-on-warning"><code>--error-on-warning</code></a></dt><dd><p>Use exit code 1 if there are any warning-level diagnostics</p>
</dd><dt id="ty-check--exclude"><a href="#ty-check--exclude"><code>--exclude</code></a> <i>exclude</i></dt><dd><p>Glob patterns for files to exclude from type checking.</p>
<p>Uses gitignore-style syntax to exclude files and directories from type checking. Supports patterns like <code>tests/</code>, <code>*.tmp</code>, <code>**/__pycache__/**</code>.</p>
</dd><dt id="ty-check--exit-zero"><a href="#ty-check--exit-zero"><code>--exit-zero</code></a></dt><dd><p>Always use exit code 0, even when there are error-level diagnostics</p>
</dd><dt id="ty-check--extra-search-path"><a href="#ty-check--extra-search-path"><code>--extra-search-path</code></a> <i>path</i></dt><dd><p>Additional path to use as a module-resolution source (can be passed multiple times).</p>
<p>This is an advanced option that should usually only be used for first-party or third-party modules that are not installed into your Python environment in a conventional way. Use <code>--python</code> to point ty to your Python environment if it is in an unusual location.</p>
</dd><dt id="ty-check--force-exclude"><a href="#ty-check--force-exclude"><code>--force-exclude</code></a></dt><dd><p>Enforce exclusions, even for paths passed to ty directly on the command-line. Use <code>--no-force-exclude</code> to disable</p>
</dd><dt id="ty-check--help"><a href="#ty-check--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Print help (see a summary with '-h')</p>
</dd><dt id="ty-check--ignore"><a href="#ty-check--ignore"><code>--ignore</code></a> <i>rule</i></dt><dd><p>Disables the rule. Can be specified multiple times.</p>
</dd><dt id="ty-check--no-progress"><a href="#ty-check--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
</dd><dt id="ty-check--output-format"><a href="#ty-check--output-format"><code>--output-format</code></a> <i>output-format</i></dt><dd><p>The format to use for printing diagnostic messages</p>
<p>Possible values:</p>
<ul>
<li><code>full</code>:  Print diagnostics verbosely, with context and helpful hints (default)</li>
<li><code>concise</code>:  Print diagnostics concisely, one per line</li>
<li><code>gitlab</code>:  Print diagnostics in the JSON format expected by GitLab Code Quality reports</li>
<li><code>github</code>:  Print diagnostics in the format used by GitHub Actions workflow error annotations</li>
</ul></dd><dt id="ty-check--project"><a href="#ty-check--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code> files will be discovered by walking up the directory tree from the given project directory, as will the project's virtual environment (<code>.venv</code>) unless the <code>venv-path</code> option is set.</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
</dd><dt id="ty-check--python"><a href="#ty-check--python"><code>--python</code></a>, <code>--venv</code> <i>path</i></dt><dd><p>Path to your project's Python environment or interpreter.</p>
<p>ty uses your Python environment to resolve third-party imports in your code.</p>
<p>If you're using a project management tool such as uv or you have an activated Conda or virtual environment, you should not generally need to specify this option.</p>
<p>This option can be used to point to virtual or system Python environments.</p>
</dd><dt id="ty-check--python-platform"><a href="#ty-check--python-platform"><code>--python-platform</code></a>, <code>--platform</code> <i>platform</i></dt><dd><p>Target platform to assume when resolving types.</p>
<p>This is used to specialize the type of <code>sys.platform</code> and will affect the visibility of platform-specific functions and attributes. If the value is set to <code>all</code>, no assumptions are made about the target platform. If unspecified, the current system's platform will be used.</p>
</dd><dt id="ty-check--python-version"><a href="#ty-check--python-version"><code>--python-version</code></a>, <code>--target-version</code> <i>version</i></dt><dd><p>Python version to assume when resolving types.</p>
<p>The Python version affects allowed syntax, type definitions of the standard library, and type definitions of first- and third-party modules that are conditional on the Python version.</p>
<p>If a version is not specified on the command line or in a configuration file, ty will try the following techniques in order of preference to determine a value: 1. Check for the <code>project.requires-python</code> setting in a <code>pyproject.toml</code> file and use the minimum version from the specified range 2. Check for an activated or configured Python environment and attempt to infer the Python version of that environment 3. Fall back to the latest stable Python version supported by ty (see <code>ty check --help</code> output)</p>
<p>Possible values:</p>
<ul>
<li><code>3.7</code></li>
<li><code>3.8</code></li>
<li><code>3.9</code></li>
<li><code>3.10</code></li>
<li><code>3.11</code></li>
<li><code>3.12</code></li>
<li><code>3.13</code></li>
<li><code>3.14</code></li>
</ul></dd><dt id="ty-check--quiet"><a href="#ty-check--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output (or <code>-qq</code> for silent output)</p>
</dd><dt id="ty-check--respect-ignore-files"><a href="#ty-check--respect-ignore-files"><code>--respect-ignore-files</code></a></dt><dd><p>Respect file exclusions via <code>.gitignore</code> and other standard ignore files. Use <code>--no-respect-gitignore</code> to disable</p>
</dd><dt id="ty-check--typeshed"><a href="#ty-check--typeshed"><code>--typeshed</code></a>, <code>--custom-typeshed-dir</code> <i>path</i></dt><dd><p>Custom directory to use for stdlib typeshed stubs</p>
</dd><dt id="ty-check--verbose"><a href="#ty-check--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output (or <code>-vv</code> and <code>-vvv</code> for more verbose output)</p>
</dd><dt id="ty-check--warn"><a href="#ty-check--warn"><code>--warn</code></a> <i>rule</i></dt><dd><p>Treat the given rule as having severity 'warn'. Can be specified multiple times.</p>
</dd><dt id="ty-check--watch"><a href="#ty-check--watch"><code>--watch</code></a>, <code>-W</code></dt><dd><p>Watch files for changes and recheck files related to the changed files</p>
</dd></dl>

## ty server

Start the language server

<h3 class="cli-reference">Usage</h3>

```
ty server
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="ty-server--help"><a href="#ty-server--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Print help</p>
</dd></dl>

## ty version

Display ty's version

<h3 class="cli-reference">Usage</h3>

```
ty version
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="ty-version--help"><a href="#ty-version--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Print help</p>
</dd></dl>

## ty generate-shell-completion

Generate shell completion

<h3 class="cli-reference">Usage</h3>

```
ty generate-shell-completion <SHELL>
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="ty-generate-shell-completion--shell"><a href="#ty-generate-shell-completion--shell"><code>SHELL</code></a></dt></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="ty-generate-shell-completion--help"><a href="#ty-generate-shell-completion--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Print help</p>
</dd></dl>

## ty help

Print this message or the help of the given subcommand(s)

<h3 class="cli-reference">Usage</h3>

```
ty help [COMMAND]
```

