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
    /// Returns the TOML contents of the metadata block.
    pub fn metadata(&self) -> &str {
        &self.metadata
    }

    /// Returns metadata padded so TOML spans refer to offsets in the original Python source.
    ///
    /// PEP 723 removes the `# ` prefix from every metadata line. Replacing those prefixes and the
    /// preceding source with whitespace instead preserves each metadata value's original offset.
    pub fn metadata_with_source_offsets(&self, source: &str) -> Option<String> {
        let prelude = source.get(..self.prelude.len())?;
        if prelude != self.prelude {
            return None;
        }

        let mut lines = source.get(self.prelude.len()..)?.split_inclusive('\n');
        let opening = lines.next()?;
        let mut metadata = String::with_capacity(self.prelude.len() + self.metadata.len());
        metadata.extend(prelude.bytes().chain(opening.bytes()).map(|byte| {
            if matches!(byte, b'\n' | b'\r') {
                char::from(byte)
            } else {
                ' '
            }
        }));

        for expected in self.metadata.lines() {
            let line = lines.next()?;
            let without_newline = line.strip_suffix('\n').unwrap_or(line);
            let body = without_newline
                .strip_suffix('\r')
                .unwrap_or(without_newline);

            if expected.is_empty() && body == "# ///" {
                break;
            }

            let content = if let Some(content) = body.strip_prefix("# ") {
                content
            } else if body == "#" {
                ""
            } else {
                return None;
            };

            if content != expected {
                return None;
            }

            metadata.push(' ');
            if body.starts_with("# ") {
                metadata.push(' ');
            }
            metadata.push_str(content);
            metadata.push_str(&line[body.len()..]);
        }

        Some(metadata)
    }

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
        FINDER
            .find_iter(contents)
            .find_map(|index| Self::parse_at(contents, index))
    }

    fn parse_at(contents: &[u8], index: usize) -> Option<Self> {
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

#[cfg(test)]
mod tests {
    use super::ScriptTag;

    #[test]
    fn metadata_offsets_match_python_source() {
        for source in [
            "# /// script\n# requires-python = \">=3.12\"\n# ///\n",
            "#!/usr/bin/env python\n# /// script\n#\n# requires-python = \">=3.12\"\n# ///\n",
            "# café\r\n# /// script\r\n#\r\n# requires-python = \">=3.12\"\r\n# ///\r\n",
        ] {
            let metadata_offset = ScriptTag::parse(source.as_bytes())
                .and_then(|tag| tag.metadata_with_source_offsets(source))
                .and_then(|metadata| metadata.find("\">=3.12\""));

            assert_eq!(metadata_offset, source.find("\">=3.12\""), "{source:?}");
        }
    }

    #[test]
    fn empty_metadata_preserves_source_offsets() {
        let source = "# /// script\n# ///\n";
        assert!(
            ScriptTag::parse(source.as_bytes())
                .and_then(|tag| tag.metadata_with_source_offsets(source))
                .is_some()
        );
    }
}
