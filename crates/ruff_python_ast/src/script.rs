use std::sync::LazyLock;

use memchr::memmem::Finder;

static FINDER: LazyLock<Finder> = LazyLock::new(|| Finder::new(b"# /// script"));

/// PEP 723 metadata as parsed from a `script` comment block.
///
/// See: <https://peps.python.org/pep-0723/>
///
/// Vendored from: <https://github.com/astral-sh/uv/blob/debe67ffdb0cd7835734100e909b2d8f79613743/crates/uv-scripts/src/lib.rs#L283>
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ScriptTag {
    /// The content of the script before the metadata block.
    prelude: String,
    /// The metadata block.
    metadata: String,
    /// The content of the script after the metadata block.
    postlude: String,
}

impl ScriptTag {
    /// Given the contents of a Python file, extract the `script` metadata block with leading
    /// comment hashes removed, any preceding shebang or content (prelude), and the remaining Python
    /// script.
    ///
    /// Given the following input string representing the contents of a Python script:
    ///
    /// ```python
    /// #!/usr/bin/env python3
    /// # /// script
    /// # requires-python = '>=3.11'
    /// # dependencies = [
    /// #   'requests<3',
    /// #   'rich',
    /// # ]
    /// # ///
    ///
    /// import requests
    ///
    /// print("Hello, World!")
    /// ```
    ///
    /// This function would return:
    ///
    /// - Preamble: `#!/usr/bin/env python3\n`
    /// - Metadata: `requires-python = '>=3.11'\ndependencies = [\n  'requests<3',\n  'rich',\n]`
    /// - Postlude: `import requests\n\nprint("Hello, World!")\n`
    ///
    /// See: <https://peps.python.org/pep-0723/>
    pub fn parse(contents: &[u8]) -> Option<Self> {
        // Identify the opening pragma.
        let index = FINDER.find(contents)?;

        // The opening pragma must be the first line, or immediately preceded by a newline.
        if !(index == 0 || matches!(contents[index - 1], b'\r' | b'\n')) {
            return None;
        }

        // Extract the preceding content.
        let prelude = std::str::from_utf8(&contents[..index]).ok()?;

        // Decode as UTF-8.
        let contents = &contents[index..];
        let contents = std::str::from_utf8(contents).ok()?;

        let mut lines = contents.lines();

        // Ensure that the first line is exactly `# /// script`.
        if lines.next().is_none_or(|line| line != "# /// script") {
            return None;
        }

        // > Every line between these two lines (# /// TYPE and # ///) MUST be a comment starting
        // > with #. If there are characters after the # then the first character MUST be a space. The
        // > embedded content is formed by taking away the first two characters of each line if the
        // > second character is a space, otherwise just the first character (which means the line
        // > consists of only a single #).
        let mut toml = vec![];

        // Extract the content that follows the metadata block.
        let mut python_script = vec![];

        while let Some(line) = lines.next() {
            // Remove the leading `#`.
            let Some(line) = line.strip_prefix('#') else {
                python_script.push(line);
                python_script.extend(lines);
                break;
            };

            // If the line is empty, continue.
            if line.is_empty() {
                toml.push("");
                continue;
            }

            // Otherwise, the line _must_ start with ` `.
            let Some(line) = line.strip_prefix(' ') else {
                python_script.push(line);
                python_script.extend(lines);
                break;
            };

            toml.push(line);
        }

        // Find the closing `# ///`. The precedence is such that we need to identify the _last_ such
        // line.
        //
        // For example, given:
        // ```python
        // # /// script
        // #
        // # ///
        // #
        // # ///
        // ```
        //
        // The latter `///` is the closing pragma
        let index = toml.iter().rev().position(|line| *line == "///")?;
        let index = toml.len() - index;

        // Discard any lines after the closing `# ///`.
        //
        // For example, given:
        // ```python
        // # /// script
        // #
        // # ///
        // #
        // #
        // ```
        //
        // We need to discard the last two lines.
        toml.truncate(index - 1);

        // Join the lines into a single string.
        let prelude = prelude.to_string();
        let metadata = toml.join("\n") + "\n";
        let postlude = python_script.join("\n") + "\n";

        Some(Self {
            prelude,
            metadata,
            postlude,
        })
    }
}
