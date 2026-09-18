use std::sync::LazyLock;

use memchr::memmem::Finder;
use ruff_source_file::UniversalNewlineIterator;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

static FINDER: LazyLock<Finder> = LazyLock::new(|| Finder::new(b"# /// script"));

/// PEP 723 metadata as parsed from a `script` comment block.
///
/// See: <https://peps.python.org/pep-0723/>
///
/// Vendored from: <https://github.com/astral-sh/uv/blob/debe67ffdb0cd7835734100e909b2d8f79613743/crates/uv-scripts/src/lib.rs#L283>
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ScriptTag {
    /// The metadata block.
    metadata: String,
    /// The source range of the metadata block, including its opening and closing delimiters.
    range: TextRange,
    /// Maps offsets in the extracted metadata to offsets in the original Python script.
    source_map: ScriptSourceMap,
}

impl ScriptTag {
    /// Returns the TOML contents of the metadata block.
    pub fn metadata(&self) -> &str {
        &self.metadata
    }

    /// Returns the map from extracted TOML offsets to their original script offsets.
    pub fn source_map(&self) -> &ScriptSourceMap {
        &self.source_map
    }

    /// Given the contents of a Python file, extract the `script` metadata block with leading
    /// comment hashes removed and map its offsets to the original Python script.
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
    /// This function extracts the metadata:
    /// ```toml
    /// requires-python = '>=3.11'
    /// dependencies = [
    ///   'requests<3',
    ///   'rich',
    /// ]
    /// ```
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

        let contents = std::str::from_utf8(contents).ok()?;
        let contents = &contents[index..];

        let start = TextSize::try_from(index).ok()?;
        let mut lines = UniversalNewlineIterator::with_offset(contents, start);

        // Ensure that the first line is exactly `# /// script`.
        if lines.next().is_none_or(|line| line != "# /// script") {
            return None;
        }

        // > Every line between these two lines (# /// TYPE and # ///) MUST be a comment starting
        // > with #. If there are characters after the # then the first character MUST be a space. The
        // > embedded content is formed by taking away the first two characters of each line if the
        // > second character is a space, otherwise just the first character (which means the line
        // > consists of only a single #).
        let mut metadata = String::new();
        let mut source_map = ScriptSourceMap::default();
        let mut closing = None;

        for line in lines {
            // Remove the leading `#`.
            let Some(comment) = line.strip_prefix('#') else {
                break;
            };

            let (content, indent_len) = if comment.is_empty() {
                ("", TextSize::ZERO)
            } else if let Some(content) = comment.strip_prefix(' ') {
                (content, ' '.text_len())
            } else {
                break;
            };

            if content == "///" {
                closing = Some((metadata.len(), source_map.markers.len(), line.range()));
            }

            let prefix_length = '#'.text_len() + indent_len;

            source_map.push_marker(metadata.text_len(), line.start() + prefix_length);
            metadata.push_str(content);
            metadata.push('\n');
        }

        // The last closing `# ///` wins, so discard that delimiter and everything after it.
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
        let (metadata_end, marker_count, closing_range) = closing?;
        metadata.truncate(metadata_end);
        source_map.truncate(marker_count);

        if metadata.is_empty() {
            metadata.push('\n');
        } else {
            source_map.push_marker(metadata.text_len(), closing_range.start());
        }

        Some(Self {
            metadata,
            range: TextRange::new(start, closing_range.end()),
            source_map,
        })
    }
}

impl Ranged for ScriptTag {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Maps offsets in extracted script metadata to offsets in the original Python source.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScriptSourceMap {
    markers: Vec<ScriptSourceMarker>,
}

impl ScriptSourceMap {
    /// Maps a metadata offset to the corresponding offset in the Python source.
    fn map_offset(&self, offset: TextSize) -> TextSize {
        let Some(index) = self
            .markers
            .partition_point(|marker| marker.metadata_offset <= offset)
            .checked_sub(1)
        else {
            return offset;
        };
        let marker = &self.markers[index];

        marker.source_offset + (offset - marker.metadata_offset)
    }

    /// Maps a metadata range to its corresponding range in the Python source.
    pub fn map_range(&self, range: TextRange) -> TextRange {
        TextRange::new(self.map_offset(range.start()), self.map_offset(range.end()))
    }

    fn push_marker(&mut self, metadata_offset: TextSize, source_offset: TextSize) {
        self.markers.push(ScriptSourceMarker {
            metadata_offset,
            source_offset,
        });
    }

    fn truncate(&mut self, len: usize) {
        self.markers.truncate(len);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ScriptSourceMarker {
    metadata_offset: TextSize,
    source_offset: TextSize,
}

#[cfg(test)]
mod tests {
    use ruff_text_size::{Ranged, TextLen, TextRange};

    use super::ScriptTag;

    #[test]
    fn carriage_return_line_endings() -> Result<(), &'static str> {
        let tag = ScriptTag::parse(b"# /// script\r# value = true\r# ///\r")
            .ok_or("Expected script metadata with carriage-return line endings")?;

        assert_eq!(tag.metadata(), "value = true\n");

        Ok(())
    }

    #[test]
    fn metadata_block_range_includes_both_delimiters() -> Result<(), &'static str> {
        let prefix = "#!/usr/bin/env python3\n\n";
        let metadata = "# /// script\n# dependencies = []\n# ///";
        let source = format!("{prefix}{metadata}\n\nprint('hello')\n");
        let tag = ScriptTag::parse(source.as_bytes()).ok_or("Expected valid script metadata")?;

        assert_eq!(
            tag.range(),
            TextRange::at(prefix.text_len(), metadata.text_len())
        );

        Ok(())
    }

    #[test]
    fn metadata_range_accounts_for_unicode_crlf_and_multiline_values() -> Result<(), &'static str> {
        let metadata_value = r#""""
first

last
""""#;
        let source_value = r#""""
# first
#
# last
# """"#
            .replace('\n', "\r\n");
        let source = format!("π\r\n# /// script\r\n# value = {source_value}\r\n# ///\r\n");
        let tag = ScriptTag::parse(source.as_bytes()).ok_or("Expected valid script metadata")?;

        assert_eq!(tag.metadata(), format!("value = {metadata_value}\n"));

        let metadata_range = TextRange::at("value = ".text_len(), metadata_value.text_len());
        let source_range = TextRange::at(
            "π\r\n# /// script\r\n# value = ".text_len(),
            source_value.text_len(),
        );

        assert_eq!(tag.source_map().map_range(metadata_range), source_range);

        Ok(())
    }

    #[test]
    fn last_closing_delimiter_discards_following_comments() -> Result<(), &'static str> {
        let source = r"# /// script
# first = true
# ///
# last = true
# ///
# ignored = true
";
        let tag = ScriptTag::parse(source.as_bytes()).ok_or("Expected valid script metadata")?;

        assert_eq!(
            tag.metadata(),
            r"first = true
///
last = true
"
        );

        let closing_start = source
            .rfind("# ///")
            .map(|offset| source[..offset].text_len())
            .ok_or("Expected the final closing delimiter")?;
        assert_eq!(
            tag.source_map().map_offset(tag.metadata().text_len()),
            closing_start,
        );
        assert_eq!(tag.end(), closing_start + "# ///".text_len());

        Ok(())
    }
}
