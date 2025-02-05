use std::{borrow::Cow, collections::hash_map::Entry};

use anyhow::bail;
use rustc_hash::FxHashMap;

use ruff_index::{newtype_index, IndexVec};
use ruff_python_trivia::Cursor;
use ruff_source_file::{LineIndex, LineRanges, OneIndexed};
use ruff_text_size::{TextLen, TextRange, TextSize};

use crate::config::MarkdownTestConfig;

/// Parse the Markdown `source` as a test suite with given `title`.
pub(crate) fn parse<'s>(title: &'s str, source: &'s str) -> anyhow::Result<MarkdownTestSuite<'s>> {
    let parser = Parser::new(title, source);
    parser.parse()
}

/// A parsed markdown file containing tests.
///
/// Borrows from the source string and filepath it was created from.
#[derive(Debug)]
pub(crate) struct MarkdownTestSuite<'s> {
    /// Header sections.
    sections: IndexVec<SectionId, Section<'s>>,

    /// Test files embedded within the Markdown file.
    files: IndexVec<EmbeddedFileId, EmbeddedFile<'s>>,
}

impl<'s> MarkdownTestSuite<'s> {
    pub(crate) fn tests(&self) -> MarkdownTestIterator<'_, 's> {
        MarkdownTestIterator {
            suite: self,
            current_file_index: 0,
        }
    }
}

/// A single test inside a [`MarkdownTestSuite`].
///
/// A test is a single header section (or the implicit root section, if there are no Markdown
/// headers in the file), containing one or more embedded Python files as fenced code blocks, and
/// containing no nested header subsections.
#[derive(Debug)]
pub(crate) struct MarkdownTest<'m, 's> {
    suite: &'m MarkdownTestSuite<'s>,
    section: &'m Section<'s>,
    files: &'m [EmbeddedFile<'s>],
}

impl<'m, 's> MarkdownTest<'m, 's> {
    pub(crate) fn name(&self) -> String {
        let mut name = String::new();
        let mut parent_id = self.section.parent_id;
        while let Some(next_id) = parent_id {
            let parent = &self.suite.sections[next_id];
            parent_id = parent.parent_id;
            if !name.is_empty() {
                name.insert_str(0, " - ");
            }
            name.insert_str(0, parent.title);
        }
        if !name.is_empty() {
            name.push_str(" - ");
        }
        name.push_str(self.section.title);
        name
    }

    pub(crate) fn files(&self) -> impl Iterator<Item = &'m EmbeddedFile<'s>> {
        self.files.iter()
    }

    pub(crate) fn configuration(&self) -> &MarkdownTestConfig {
        &self.section.config
    }

    pub(super) fn should_snapshot_diagnostics(&self) -> bool {
        self.section.snapshot_diagnostics
    }
}

/// Iterator yielding all [`MarkdownTest`]s in a [`MarkdownTestSuite`].
#[derive(Debug)]
pub(crate) struct MarkdownTestIterator<'m, 's> {
    suite: &'m MarkdownTestSuite<'s>,
    current_file_index: usize,
}

impl<'m, 's> Iterator for MarkdownTestIterator<'m, 's> {
    type Item = MarkdownTest<'m, 's>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut current_file_index = self.current_file_index;
        let mut file = self.suite.files.get(current_file_index.into());
        let section_id = file?.section;
        while file.is_some_and(|file| file.section == section_id) {
            current_file_index += 1;
            file = self.suite.files.get(current_file_index.into());
        }
        let files = &self.suite.files[EmbeddedFileId::from_usize(self.current_file_index)
            ..EmbeddedFileId::from_usize(current_file_index)];
        self.current_file_index = current_file_index;
        Some(MarkdownTest {
            suite: self.suite,
            section: &self.suite.sections[section_id],
            files,
        })
    }
}

#[newtype_index]
struct SectionId;

/// A single header section of a [`MarkdownTestSuite`], or the implicit root "section".
///
/// A header section is the part of a Markdown file beginning with a `#`-prefixed header line, and
/// extending until the next header line at the same or higher outline level (that is, with the
/// same number or fewer `#` characters).
///
/// A header section may either contain one or more embedded Python files (making it a
/// [`MarkdownTest`]), or it may contain nested sections (headers with more `#` characters), but
/// not both.
#[derive(Debug)]
struct Section<'s> {
    title: &'s str,
    level: u8,
    parent_id: Option<SectionId>,
    config: MarkdownTestConfig,
    snapshot_diagnostics: bool,
}

#[newtype_index]
struct EmbeddedFileId;

#[derive(Debug)]
pub(crate) struct CodeBlock<'s> {
    pub(crate) code: &'s str,

    /// The offset of the backticks beginning the code block within the markdown file
    pub(crate) backtick_offset: TextSize,
}

/// Holds information about the position and the length of a single code block in a
/// Markdown file.
#[derive(Debug)]
pub(crate) struct CodeBlockDimensions {
    pub(crate) backtick_offset: TextSize,
    pub(crate) line_count: usize,
}

/// Holds information about the position and length of all code blocks that are part of
/// a single embedded file in a Markdown file. This is used to reconstruct absolute line
/// numbers (in the Markdown file) from relative line numbers (in the embedded file).
///
/// If we have a Markdown section with multiple code blocks like this:
///
///    01   # Test
///    02
///    03   Part 1:
///    04
///    05   ```py
///    06   a = 1    # Relative line number: 1
///    07   b = 2    # Relative line number: 2
///    08   ```
///    09
///    10   Part 2:
///    11
///    12   ```py
///    13   c = 3    # Relative line number: 3
///    14   ```
///
/// We want to reconstruct the absolute line number (left) from relative
/// line numbers. The information we have is the start line and the line
/// count of each code block:
///
///    - Block 1: (start =  5, count = 2)
///    - Block 2: (start = 12, count = 1)
///
/// For example, if we see a relative line number of 3, we see that it is
/// larger than the line count of the first block, so we subtract the line
/// count of the first block, and then add the new relative line number (1)
/// to the absolute start line of the second block (12), resulting in an
/// absolute line number of 13.
pub(crate) struct CodeBlockStructure {
    start_line_and_line_count: Vec<(usize, usize)>,
}

impl CodeBlockStructure {
    pub(crate) fn new(
        md_index: &LineIndex,
        dimensions: impl Iterator<Item = CodeBlockDimensions>,
    ) -> CodeBlockStructure {
        CodeBlockStructure {
            start_line_and_line_count: dimensions
                .map(|d| (md_index.line_index(d.backtick_offset).get(), d.line_count))
                .collect(),
        }
    }

    pub(crate) fn to_absolute_line_number(&self, relative_line_number: OneIndexed) -> OneIndexed {
        let mut absolute_line_number = 0;
        let mut relative_line_number = relative_line_number.get();

        for (start_line, line_count) in &self.start_line_and_line_count {
            if let Some(updated) = relative_line_number.checked_sub(*line_count) {
                relative_line_number = updated;
            } else {
                absolute_line_number = start_line + relative_line_number;
                break;
            }
        }

        OneIndexed::new(absolute_line_number).expect("Relative line number out of bounds")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum EmbeddedFilePath<'s> {
    AutogeneratedPy,
    AutogeneratedPyi,
    Explicit(&'s str),
}

impl EmbeddedFilePath<'_> {
    pub(crate) fn as_str(&self) -> &str {
        match self {
            EmbeddedFilePath::AutogeneratedPy => "mdtest_snippet.py",
            EmbeddedFilePath::AutogeneratedPyi => "mdtest_snippet.pyi",
            EmbeddedFilePath::Explicit(path) => path,
        }
    }

    fn is_explicit(&self) -> bool {
        matches!(self, EmbeddedFilePath::Explicit(_))
    }

    fn is_allowed_explicit_path(path: &str) -> bool {
        [
            EmbeddedFilePath::AutogeneratedPy,
            EmbeddedFilePath::AutogeneratedPyi,
        ]
        .iter()
        .all(|autogenerated| path != autogenerated.as_str())
    }
}

/// A single file embedded in a [`Section`] as a fenced code block.
///
/// Currently must be a Python file (`py` language), a type stub (`pyi`) or a [typeshed `VERSIONS`]
/// file.
///
/// TOML configuration blocks are also supported, but are not stored as `EmbeddedFile`s. In the
/// future we plan to support `pth` files as well.
///
/// A Python embedded file makes its containing [`Section`] into a [`MarkdownTest`], and will be
/// type-checked and searched for inline-comment assertions to match against the diagnostics from
/// type checking.
///
/// [typeshed `VERSIONS`]: https://github.com/python/typeshed/blob/c546278aae47de0b2b664973da4edb613400f6ce/stdlib/VERSIONS#L1-L18
#[derive(Debug)]
pub(crate) struct EmbeddedFile<'s> {
    section: SectionId,
    path: EmbeddedFilePath<'s>,
    pub(crate) lang: &'s str,
    code_blocks: Vec<CodeBlock<'s>>,
}

impl EmbeddedFile<'_> {
    /// Returns the full code for the embedded file, which can consist
    /// of multiple code blocks.
    pub(crate) fn code(&self) -> Cow<'_, str> {
        if self.code_blocks.len() == 1 {
            Cow::Borrowed(self.code_blocks[0].code)
        } else {
            let mut merged_code = String::new();
            for block in &self.code_blocks {
                // Treat empty code blocks as non-existent, instead of creating
                // an additional empty line:
                if block.code.is_empty() {
                    continue;
                }

                if !merged_code.is_empty() {
                    merged_code.push('\n');
                }
                merged_code.push_str(block.code);
            }
            Cow::Owned(merged_code)
        }
    }

    pub(crate) fn path_str(&self) -> &str {
        self.path.as_str()
    }

    pub(crate) fn code_block_dimensions(&self) -> impl Iterator<Item = CodeBlockDimensions> + '_ {
        self.code_blocks.iter().map(|b| CodeBlockDimensions {
            backtick_offset: b.backtick_offset,
            line_count: b.code.lines().count(),
        })
    }
}

#[derive(Debug)]
struct SectionStack(Vec<SectionId>);

impl SectionStack {
    fn new(root_section_id: SectionId) -> Self {
        Self(vec![root_section_id])
    }

    fn push(&mut self, section_id: SectionId) {
        self.0.push(section_id);
    }

    fn pop(&mut self) -> Option<SectionId> {
        let popped = self.0.pop();
        debug_assert_ne!(popped, None, "Should never pop the implicit root section");
        debug_assert!(
            !self.0.is_empty(),
            "Should never pop the implicit root section"
        );
        popped
    }

    fn top(&mut self) -> SectionId {
        *self
            .0
            .last()
            .expect("Should never pop the implicit root section")
    }
}

/// Parse the source of a Markdown file into a [`MarkdownTestSuite`].
#[derive(Debug)]
struct Parser<'s> {
    /// [`Section`]s of the final [`MarkdownTestSuite`].
    sections: IndexVec<SectionId, Section<'s>>,

    /// [`EmbeddedFile`]s of the final [`MarkdownTestSuite`].
    files: IndexVec<EmbeddedFileId, EmbeddedFile<'s>>,

    /// The unparsed remainder of the Markdown source.
    cursor: Cursor<'s>,

    // Number of consecutive empty lines.
    preceding_blank_lines: usize,

    // Explicitly specified path for the upcoming code block.
    explicit_path: Option<&'s str>,

    source: &'s str,
    source_len: TextSize,

    /// Stack of ancestor sections.
    stack: SectionStack,

    /// Names of embedded files in current active section.
    current_section_files: FxHashMap<EmbeddedFilePath<'s>, EmbeddedFileId>,

    /// Whether or not the current section has a config block.
    current_section_has_config: bool,
}

impl<'s> Parser<'s> {
    fn new(title: &'s str, source: &'s str) -> Self {
        let mut sections = IndexVec::default();
        let root_section_id = sections.push(Section {
            title,
            level: 0,
            parent_id: None,
            config: MarkdownTestConfig::default(),
            snapshot_diagnostics: false,
        });
        Self {
            sections,
            source,
            files: IndexVec::default(),
            cursor: Cursor::new(source),
            preceding_blank_lines: 0,
            explicit_path: None,
            source_len: source.text_len(),
            stack: SectionStack::new(root_section_id),
            current_section_files: FxHashMap::default(),
            current_section_has_config: false,
        }
    }

    fn parse(mut self) -> anyhow::Result<MarkdownTestSuite<'s>> {
        self.parse_impl()?;
        Ok(self.finish())
    }

    fn finish(mut self) -> MarkdownTestSuite<'s> {
        self.sections.shrink_to_fit();
        self.files.shrink_to_fit();

        MarkdownTestSuite {
            sections: self.sections,
            files: self.files,
        }
    }

    fn skip_whitespace(&mut self) {
        self.cursor.eat_while(|c| c.is_whitespace() && c != '\n');
    }

    fn skip_to_beginning_of_next_line(&mut self) -> bool {
        if let Some(position) = memchr::memchr(b'\n', self.cursor.as_bytes()) {
            self.cursor.skip_bytes(position + 1);
            true
        } else {
            false
        }
    }

    fn consume_until(&mut self, mut end_predicate: impl FnMut(char) -> bool) -> Option<&'s str> {
        let start = self.offset().to_usize();

        while !self.cursor.is_eof() {
            if end_predicate(self.cursor.first()) {
                return Some(&self.source[start..self.offset().to_usize()]);
            }
            self.cursor.bump();
        }

        None
    }

    fn parse_impl(&mut self) -> anyhow::Result<()> {
        const SECTION_CONFIG_SNAPSHOT: &str = "<!-- snapshot-diagnostics -->";
        const CODE_BLOCK_END: &[u8] = b"```";

        while let Some(first) = self.cursor.bump() {
            match first {
                '<' => {
                    self.explicit_path = None;
                    self.preceding_blank_lines = 0;
                    // If we want to support more comment directives, then we should
                    // probably just parse the directive generically first. But it's
                    // not clear if we'll want to add more, since comments are hidden
                    // from GitHub Markdown rendering.
                    if self
                        .cursor
                        .as_str()
                        .starts_with(&SECTION_CONFIG_SNAPSHOT[1..])
                    {
                        self.cursor.skip_bytes(SECTION_CONFIG_SNAPSHOT.len() - 1);
                        self.process_snapshot_diagnostics()?;
                    }
                }
                '#' => {
                    self.explicit_path = None;
                    self.preceding_blank_lines = 0;

                    // Determine header level (number of '#' characters)
                    let mut header_level = 1;
                    while self.cursor.eat_char('#') {
                        header_level += 1;
                    }

                    // Parse header title
                    if let Some(title) = self.consume_until(|c| c == '\n') {
                        let title = title.trim();

                        if !title.is_empty() {
                            self.process_header(header_level, title)?;
                        }
                    }
                }
                '`' => {
                    if self.cursor.eat_char2('`', '`') {
                        // We saw the triple-backtick beginning of a code block.

                        let backtick_offset = self.offset() - "```".text_len();

                        if self.preceding_blank_lines < 1 && self.explicit_path.is_none() {
                            bail!("Code blocks must start on a new line and be preceded by at least one blank line.");
                        }

                        self.skip_whitespace();

                        // Parse the code block language specifier
                        let lang = self
                            .consume_until(|c| matches!(c, ' ' | '\n'))
                            .unwrap_or_default();

                        self.skip_whitespace();

                        if !self.cursor.eat_char('\n') {
                            bail!("Trailing code-block metadata is not supported. Only the code block language can be specified.");
                        }

                        if let Some(position) =
                            memchr::memmem::find(self.cursor.as_bytes(), CODE_BLOCK_END)
                        {
                            let mut code = &self.cursor.as_str()[..position];
                            self.cursor.skip_bytes(position + CODE_BLOCK_END.len());

                            if code.ends_with('\n') {
                                code = &code[..code.len() - '\n'.len_utf8()];
                            }

                            self.process_code_block(lang, code, backtick_offset)?;
                        } else {
                            let code_block_start = self.cursor.token_len();
                            let line = self.source.count_lines(TextRange::up_to(code_block_start));
                            bail!("Unterminated code block at line {line}.");
                        }

                        self.explicit_path = None;
                    } else if self.preceding_blank_lines > 0 {
                        // This could be a line that specifies an explicit path for a Markdown code block (`module.py`:)
                        self.explicit_path = None;

                        if let Some(path) = self.consume_until(|c| matches!(c, '`' | '\n')) {
                            if self.cursor.eat_char('`') {
                                self.skip_whitespace();
                                if self.cursor.eat_char(':') {
                                    self.explicit_path = Some(path);
                                }
                            }
                        }
                    }

                    self.preceding_blank_lines = 0;
                }
                '\n' => {
                    self.preceding_blank_lines += 1;
                    continue;
                }
                c => {
                    self.preceding_blank_lines = 0;
                    self.explicit_path = None;

                    if c.is_whitespace() {
                        self.skip_whitespace();
                        if self.cursor.eat_char('`')
                            && self.cursor.eat_char('`')
                            && self.cursor.eat_char('`')
                        {
                            bail!("Indented code blocks are not supported.");
                        }
                    }
                }
            }

            if !self.skip_to_beginning_of_next_line() {
                break;
            }
        }

        Ok(())
    }

    fn process_header(&mut self, header_level: usize, title: &'s str) -> anyhow::Result<()> {
        self.pop_sections_to_level(header_level);

        let parent = self.stack.top();

        let section = Section {
            title,
            level: header_level.try_into()?,
            parent_id: Some(parent),
            config: self.sections[parent].config.clone(),
            snapshot_diagnostics: self.sections[parent].snapshot_diagnostics,
        };

        if !self.current_section_files.is_empty() {
            bail!(
                "Header '{}' not valid inside a test case; parent '{}' has code files.",
                section.title,
                self.sections[parent].title,
            );
        }

        let section_id = self.sections.push(section);
        self.stack.push(section_id);

        self.current_section_files.clear();
        self.current_section_has_config = false;

        Ok(())
    }

    fn process_code_block(
        &mut self,
        lang: &'s str,
        code: &'s str,
        backtick_offset: TextSize,
    ) -> anyhow::Result<()> {
        // We never pop the implicit root section.
        let section = self.stack.top();

        if lang == "toml" {
            return self.process_config_block(code);
        }

        if let Some(explicit_path) = self.explicit_path {
            if !lang.is_empty()
                && lang != "text"
                && explicit_path.contains('.')
                && !explicit_path.ends_with(&format!(".{lang}"))
            {
                bail!(
                    "File ending of test file path `{explicit_path}` does not match `lang={lang}` of code block"
                );
            }
        }

        let path = match self.explicit_path {
            Some(path) => {
                if !EmbeddedFilePath::is_allowed_explicit_path(path) {
                    bail!("The file name `{path}` must not be used explicitly.",);
                }

                EmbeddedFilePath::Explicit(path)
            }
            None => match lang {
                "py" => EmbeddedFilePath::AutogeneratedPy,
                "pyi" => EmbeddedFilePath::AutogeneratedPyi,
                "" => {
                    bail!("Cannot auto-generate file name for code block with empty language specifier");
                }
                _ => {
                    bail!(
                        "Cannot auto-generate file name for code block with language `{}`",
                        lang
                    );
                }
            },
        };

        let code_block = CodeBlock {
            code,
            backtick_offset,
        };

        let has_merged_snippets = self.current_section_has_merged_snippets();
        let has_explicit_file_paths = self.current_section_has_explicit_file_paths();

        match self.current_section_files.entry(path.clone()) {
            Entry::Vacant(entry) => {
                if has_merged_snippets {
                    bail!("Merged snippets are not allowed in the presence of other files.");
                }

                let index = self.files.push(EmbeddedFile {
                    path: path.clone(),
                    section,
                    lang,
                    code_blocks: vec![code_block],
                });
                entry.insert(index);
            }
            Entry::Occupied(entry) => {
                if path.is_explicit() {
                    bail!(
                        "Test `{}` has duplicate files named `{}`.",
                        self.sections[section].title,
                        path.as_str(),
                    );
                };

                if has_explicit_file_paths {
                    bail!("Merged snippets are not allowed in the presence of other files.");
                }

                let index = *entry.get();
                self.files[index].code_blocks.push(CodeBlock {
                    code,
                    backtick_offset,
                });
            }
        }

        Ok(())
    }

    fn current_section_has_explicit_file_paths(&self) -> bool {
        self.current_section_files
            .iter()
            .any(|(path, _)| path.is_explicit())
    }

    fn current_section_has_merged_snippets(&self) -> bool {
        self.current_section_files
            .values()
            .any(|id| self.files[*id].code_blocks.len() > 1)
    }

    fn process_config_block(&mut self, code: &str) -> anyhow::Result<()> {
        if self.current_section_has_config {
            bail!("Multiple TOML configuration blocks in the same section are not allowed.");
        }

        let current_section = &mut self.sections[self.stack.top()];
        current_section.config = MarkdownTestConfig::from_str(code)?;

        self.current_section_has_config = true;

        Ok(())
    }

    fn process_snapshot_diagnostics(&mut self) -> anyhow::Result<()> {
        if self.current_section_has_config {
            bail!(
                "Section config to enable snapshotting diagnostics must come before \
                 everything else (including TOML configuration blocks).",
            );
        }
        if !self.current_section_files.is_empty() {
            bail!(
                "Section config to enable snapshotting diagnostics must come before \
                 everything else (including embedded files).",
            );
        }

        let current_section = &mut self.sections[self.stack.top()];
        if current_section.snapshot_diagnostics {
            bail!(
                "Section config to enable snapshotting diagnostics should appear \
                 at most once.",
            );
        }
        current_section.snapshot_diagnostics = true;

        Ok(())
    }

    fn pop_sections_to_level(&mut self, level: usize) {
        while level <= self.sections[self.stack.top()].level.into() {
            self.stack.pop();
            // We would have errored before pushing a child section if there were files, so we know
            // no parent section can have files.
            self.current_section_files.clear();
        }
    }

    /// Retrieves the current offset of the cursor within the source code.
    fn offset(&self) -> TextSize {
        self.source_len - self.cursor.text_len()
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_trivia::textwrap::dedent;

    use crate::parser::EmbeddedFilePath;

    #[test]
    fn empty() {
        let mf = super::parse("file.md", "").unwrap();

        assert!(mf.tests().next().is_none());
    }

    #[test]
    fn single_file_test() {
        let source = dedent(
            "
            ```py
            x = 1
            ```
            ",
        );
        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };

        assert_eq!(test.name(), "file.md");

        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path, EmbeddedFilePath::AutogeneratedPy);
        assert_eq!(file.lang, "py");
        assert_eq!(file.code(), "x = 1");
    }

    #[test]
    fn no_new_line_at_eof() {
        let source = dedent(
            "
            ```py
            x = 1
            ```",
        );
        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };

        assert_eq!(test.name(), "file.md");

        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path, EmbeddedFilePath::AutogeneratedPy);
        assert_eq!(file.lang, "py");
        assert_eq!(file.code(), "x = 1");
    }

    #[test]
    fn multiple_tests() {
        let source = dedent(
            "
            # One

            ```py
            x = 1
            ```

            # Two

            ```py
            y = 2
            ```

            # Three

            `mod_a.pyi`:

            ```pyi
            a: int
            ```

            `mod_b.pyi`:

            ```pyi
            b: str
            ```
            ",
        );
        let mf = super::parse("file.md", &source).unwrap();

        let [test1, test2, test3] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected three tests");
        };

        assert_eq!(test1.name(), "file.md - One");
        assert_eq!(test2.name(), "file.md - Two");
        assert_eq!(test3.name(), "file.md - Three");

        let [file] = test1.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path, EmbeddedFilePath::AutogeneratedPy);
        assert_eq!(file.lang, "py");
        assert_eq!(file.code(), "x = 1");

        let [file] = test2.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path, EmbeddedFilePath::AutogeneratedPy);
        assert_eq!(file.lang, "py");
        assert_eq!(file.code(), "y = 2");

        let [file_1, file_2] = test3.files().collect::<Vec<_>>()[..] else {
            panic!("expected two files");
        };

        assert_eq!(file_1.path_str(), "mod_a.pyi");
        assert_eq!(file_1.lang, "pyi");
        assert_eq!(file_1.code(), "a: int");

        assert_eq!(file_2.path_str(), "mod_b.pyi");
        assert_eq!(file_2.lang, "pyi");
        assert_eq!(file_2.code(), "b: str");
    }

    #[test]
    fn multiple_file_tests() {
        let source = dedent(
            "
            # One

            `main.py`:

            ```py
            from foo import y
            ```

            `foo.py`:

            ```py
            y = 2
            ```

            # Two

            ```py
            y = 2
            ```
            ",
        );
        let mf = super::parse("file.md", &source).unwrap();

        let [test1, test2] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected two tests");
        };

        assert_eq!(test1.name(), "file.md - One");
        assert_eq!(test2.name(), "file.md - Two");

        let [main, foo] = test1.files().collect::<Vec<_>>()[..] else {
            panic!("expected two files");
        };

        assert_eq!(main.path_str(), "main.py");
        assert_eq!(main.lang, "py");
        assert_eq!(main.code(), "from foo import y");

        assert_eq!(foo.path_str(), "foo.py");
        assert_eq!(foo.lang, "py");
        assert_eq!(foo.code(), "y = 2");

        let [file] = test2.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path, EmbeddedFilePath::AutogeneratedPy);
        assert_eq!(file.lang, "py");
        assert_eq!(file.code(), "y = 2");
    }

    #[test]
    fn merged_snippets() {
        let source = dedent(
            "
            # One

            This is the first part of the embedded file:

            ```py
            x = 1
            ```

            And this is the second part:

            ```py
            y = 2
            ```

            And this is the third part:

            ```py
            z = 3
            ```
            ",
        );
        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };

        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path, EmbeddedFilePath::AutogeneratedPy);
        assert_eq!(file.lang, "py");
        assert_eq!(file.code(), "x = 1\ny = 2\nz = 3");
    }

    #[test]
    fn no_merged_snippets_for_explicit_paths() {
        let source = dedent(
            "
            # One

            `foo.py`:

            ```py
            x = 1
            ```

            `foo.py`:

            ```py
            y = 2
            ```
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(
            err.to_string(),
            "Test `One` has duplicate files named `foo.py`."
        );
    }

    #[test]
    fn disallow_merged_snippets_in_presence_of_explicit_paths() {
        for source in [
            // Merged snippets first
            "
            # One

            ```py
            x = 1
            ```

            ```py
            y = 2
            ```

            `foo.py`:

            ```py
            print('hello')
            ```
            ",
            // Explicit path first
            "
            # One

            `foo.py`:

            ```py
            print('hello')
            ```

            ```py
            x = 1
            ```

            ```py
            y = 2
            ```
            ",
        ] {
            let err = super::parse("file.md", &dedent(source)).expect_err("Should fail to parse");
            assert_eq!(
                err.to_string(),
                "Merged snippets are not allowed in the presence of other files."
            );
        }
    }

    #[test]
    fn disallow_pyi_snippets_in_presence_of_merged_py_snippets() {
        let source = dedent(
            "
            # One

            ```py
            x = 1
            ```

            ```py
            y = 2
            ```

            ```pyi
            x: int
            ```
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(
            err.to_string(),
            "Merged snippets are not allowed in the presence of other files."
        );
    }

    #[test]
    fn custom_file_path() {
        let source = dedent(
            "
            `foo.py`:

            ```py
            x = 1
            ```
            ",
        );
        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };
        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path_str(), "foo.py");
        assert_eq!(file.lang, "py");
        assert_eq!(file.code(), "x = 1");
    }

    #[test]
    fn multi_line_file() {
        let source = dedent(
            "
            ```py
            x = 1
            y = 2
            ```
            ",
        );
        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };
        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.code(), "x = 1\ny = 2");
    }

    #[test]
    fn empty_file() {
        let source = dedent(
            "
            ```py
            ```
            ",
        );

        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };
        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.code(), "");
    }

    #[test]
    fn no_lang() {
        let source = dedent(
            "
            ```
            x = 10
            ```
            ",
        );

        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(
            err.to_string(),
            "Cannot auto-generate file name for code block with empty language specifier",
        );
    }

    #[test]
    fn cannot_generate_name_for_lang() {
        let source = dedent(
            "
            ```json
            {}
            ```
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(
            err.to_string(),
            "Cannot auto-generate file name for code block with language `json`"
        );
    }

    #[test]
    fn mismatching_lang() {
        let source = dedent(
            "
            `a.py`:

            ```pyi
            x = 1
            ```
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(
            err.to_string(),
            "File ending of test file path `a.py` does not match `lang=pyi` of code block"
        );
    }

    #[test]
    fn files_with_no_extension_can_have_any_lang() {
        let source = dedent(
            "
            `lorem`:

            ```foo
            x = 1
            ```
            ",
        );

        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };
        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path_str(), "lorem");
        assert_eq!(file.code(), "x = 1");
    }

    #[test]
    fn files_with_lang_text_can_have_any_paths() {
        let source = dedent(
            "
            `lorem.yaml`:

            ```text
            x = 1
            ```
            ",
        );

        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };
        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path_str(), "lorem.yaml");
        assert_eq!(file.code(), "x = 1");
    }

    #[test]
    fn unterminated_code_block_1() {
        let source = dedent(
            "
            ```
            x = 1
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(err.to_string(), "Unterminated code block at line 2.");
    }

    #[test]
    fn unterminated_code_block_2() {
        let source = dedent(
            "
            ## A well-fenced block

            ```py
            y = 2
            ```

            ## A not-so-well-fenced block

            ```py
            x = 1
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(err.to_string(), "Unterminated code block at line 10.");
    }

    #[test]
    fn header_start_at_beginning_of_line() {
        let source = dedent(
            "
            # A test

                # not a header

            ```py
            x = 1
            ```
            ",
        );
        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };

        assert_eq!(test.name(), "file.md - A test");
    }

    #[test]
    fn code_blocks_must_not_be_indented() {
        let source = dedent(
            "
            # A test?

                ```py
                x = 1
                ```
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(err.to_string(), "Indented code blocks are not supported.");
    }

    #[test]
    fn no_header_inside_test() {
        let source = dedent(
            "
            # One

            ```py
            x = 1
            ```

            ## Two
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(
            err.to_string(),
            "Header 'Two' not valid inside a test case; parent 'One' has code files."
        );
    }

    #[test]
    fn line_break_in_header_1() {
        let source = dedent(
            "
            #
            Foo

            ```py
            x = 1
            ```
            ",
        );

        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };
        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(test.section.title, "file.md");
        assert_eq!(file.path, EmbeddedFilePath::AutogeneratedPy);
        assert_eq!(file.code(), "x = 1");
    }

    #[test]
    fn line_break_in_header_2() {
        let source = dedent(
            "
            # Foo

            ##
            Lorem

            ```py
            x = 1
            ```
            ",
        );

        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };
        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(test.section.title, "Foo");
        assert_eq!(file.path, EmbeddedFilePath::AutogeneratedPy);
        assert_eq!(file.code(), "x = 1");
    }

    #[test]
    fn no_duplicate_name_files_in_test() {
        let source = dedent(
            "
            `foo.py`:

            ```py
            x = 1
            ```

            `foo.py`:

            ```py
            y = 2
            ```
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(
            err.to_string(),
            "Test `file.md` has duplicate files named `foo.py`."
        );
    }

    #[test]
    fn no_usage_of_autogenerated_name() {
        let source = dedent(
            "
            `mdtest_snippet.py`:

            ```py
            x = 1
            ```

            ```py
            y = 2
            ```
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(
            err.to_string(),
            "The file name `mdtest_snippet.py` must not be used explicitly."
        );
    }

    #[test]
    fn separate_path() {
        let source = dedent(
            "
            `foo.py`:

            ```py
            x = 1
            ```
            ",
        );

        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };
        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path_str(), "foo.py");
        assert_eq!(file.code(), "x = 1");
    }

    #[test]
    fn separate_path_whitespace_1() {
        let source = dedent(
            "
            `foo.py` :

            ```py
            x = 1
            ```
            ",
        );

        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };
        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path_str(), "foo.py");
        assert_eq!(file.code(), "x = 1");
    }

    #[test]
    fn separate_path_whitespace_2() {
        let source = dedent(
            "
            `foo.py`:
            ```py
            x = 1
            ```
            ",
        );

        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };
        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path_str(), "foo.py");
        assert_eq!(file.code(), "x = 1");
    }

    #[test]
    fn path_with_space() {
        let source = dedent(
            "
            `foo bar.py`:

            ```py
            x = 1
            ```
            ",
        );

        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };
        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path_str(), "foo bar.py");
        assert_eq!(file.code(), "x = 1");
    }

    #[test]
    fn path_with_line_break() {
        let source = dedent(
            "
            `foo
            .py`:

            ```py
            x = 1
            ```
            ",
        );

        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };
        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path, EmbeddedFilePath::AutogeneratedPy);
        assert_eq!(file.code(), "x = 1");
    }

    #[test]
    fn path_with_backtick() {
        let source = dedent(
            "
            `foo`bar.py`:

            ```py
            x = 1
            ```
            ",
        );

        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };
        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path, EmbeddedFilePath::AutogeneratedPy);
        assert_eq!(file.code(), "x = 1");
    }

    #[test]
    fn path_colon_on_next_line() {
        let source = dedent(
            "
            `foo.py`
            :

            ```py
            x = 1
            ```
            ",
        );

        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };
        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path, EmbeddedFilePath::AutogeneratedPy);
        assert_eq!(file.code(), "x = 1");
    }

    #[test]
    fn random_trailing_backtick_quoted() {
        let source = dedent(
            "
            A long sentence that forces a line break
            `int`:

            ```py
            x = 1
            ```
            ",
        );

        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };
        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path, EmbeddedFilePath::AutogeneratedPy);
        assert_eq!(file.code(), "x = 1");
    }

    #[test]
    fn no_newline_between_prose_and_code() {
        // Regression test for https://github.com/astral-sh/ruff/issues/15923
        let source = dedent(
            "
            Some code:
            No newline between prose and code:
            ```py
            # A syntax error:
            §
            ```
            ",
        );

        super::parse("file.md", &source).expect_err(
            "Code blocks must start on a new line and be preceded by at least one blank line.",
        );
    }

    #[test]
    fn config_no_longer_allowed() {
        let source = dedent(
            "
            ```py foo=bar
            x = 1
            ```
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(err.to_string(), "Trailing code-block metadata is not supported. Only the code block language can be specified.");
    }

    #[test]
    fn duplicate_section_directive_not_allowed() {
        let source = dedent(
            "
            # Some header

            <!-- snapshot-diagnostics -->
            <!-- snapshot-diagnostics -->

            ```py
            x = 1
            ```
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(
            err.to_string(),
            "Section config to enable snapshotting diagnostics should appear at most once.",
        );
    }

    #[test]
    fn section_directive_must_appear_before_config() {
        let source = dedent(
            "
            # Some header

            ```toml
            [environment]
            typeshed = \"/typeshed\"
            ```

            <!-- snapshot-diagnostics -->

            ```py
            x = 1
            ```
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(
            err.to_string(),
            "Section config to enable snapshotting diagnostics must \
             come before everything else \
             (including TOML configuration blocks).",
        );
    }

    #[test]
    fn section_directive_must_appear_before_embedded_files() {
        let source = dedent(
            "
            # Some header

            ```py
            x = 1
            ```

            <!-- snapshot-diagnostics -->
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(
            err.to_string(),
            "Section config to enable snapshotting diagnostics must \
             come before everything else \
             (including embedded files).",
        );
    }
}
