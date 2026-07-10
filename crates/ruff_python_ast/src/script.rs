use std::sync::LazyLock;

use memchr::memmem::Finder;
use ruff_diagnostics::SourceMap;
use ruff_text_size::TextSize;

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
    /// Maps offsets in `metadata` back to offsets in the original script.
    metadata_source_map: SourceMap,
    /// The content of the script after the metadata block.
    postlude: String,
}

impl ScriptTag {
    /// Returns the TOML contents of the metadata block.
    pub fn metadata(&self) -> &str {
        &self.metadata
    }

    /// Returns the map from TOML metadata ranges to ranges in the original script.
    pub fn metadata_source_map(&self) -> &SourceMap {
        &self.metadata_source_map
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
        // Identify the opening pragma.
        let index = FINDER.find(contents)?;

        Self::parse_at(contents, index)
    }

    /// Extracts a `script` metadata block known to start at `index`.
    ///
    /// Returns `None` if `index` does not point to an exact opening pragma at the start of a line.
    pub fn parse_at(contents: &[u8], index: usize) -> Option<Self> {
        let (prelude, contents) = contents.split_at_checked(index)?;

        // The opening pragma must be the first line, or immediately preceded by a newline.
        if prelude
            .last()
            .is_some_and(|byte| !matches!(*byte, b'\r' | b'\n'))
        {
            return None;
        }

        // Extract the preceding content.
        let prelude = std::str::from_utf8(prelude).ok()?;

        // Decode as UTF-8.
        let contents = std::str::from_utf8(contents).ok()?;

        let mut lines = lines_with_offsets(contents, index);

        // Ensure that the first line is exactly `# /// script`.
        if lines.next().is_none_or(|line| line.text != "# /// script") {
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
            let Some(comment) = line.text.strip_prefix('#') else {
                python_script.push(line.text);
                python_script.extend(lines.map(|line| line.text));
                break;
            };

            // If the line is empty, continue.
            if comment.is_empty() {
                toml.push(ScriptMetadataLine {
                    text: "",
                    source_start: line.start + 1,
                    source_end: line.end,
                });
                continue;
            }

            // Otherwise, the line _must_ start with ` `.
            let Some(metadata) = comment.strip_prefix(' ') else {
                python_script.push(comment);
                python_script.extend(lines.map(|line| line.text));
                break;
            };

            toml.push(ScriptMetadataLine {
                text: metadata,
                source_start: line.start + 2,
                source_end: line.end,
            });
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
        let index = toml.iter().rev().position(|line| line.text == "///")?;
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

        // Join the lines into a single string while recording how each line maps to the script.
        let mut metadata = String::new();
        let mut source_map = SourceMap::default();
        for line in &toml {
            source_map.push_marker(
                TextSize::try_from(metadata.len()).ok()?,
                TextSize::try_from(line.source_start).ok()?,
            );
            metadata.push_str(line.text);
            metadata.push('\n');
        }
        if let Some(last) = toml.last() {
            source_map.push_marker(
                TextSize::try_from(metadata.len()).ok()?,
                TextSize::try_from(last.source_end).ok()?,
            );
        }

        let prelude = prelude.to_string();
        if metadata.is_empty() {
            metadata.push('\n');
        }
        let postlude = python_script.join("\n") + "\n";

        Some(Self {
            prelude,
            metadata,
            metadata_source_map: source_map,
            postlude,
        })
    }
}

struct ScriptLine<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

fn lines_with_offsets(source: &str, start: usize) -> impl Iterator<Item = ScriptLine<'_>> {
    let mut offset = start;

    source.split_inclusive('\n').map(move |raw_line| {
        let line_start = offset;
        offset += raw_line.len();

        let text = raw_line
            .strip_suffix('\n')
            .map_or(raw_line, |line| line.strip_suffix('\r').unwrap_or(line));

        ScriptLine {
            text,
            start: line_start,
            end: offset,
        }
    })
}

struct ScriptMetadataLine<'a> {
    text: &'a str,
    source_start: usize,
    source_end: usize,
}
