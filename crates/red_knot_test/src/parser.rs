use anyhow::bail;
use rustc_hash::FxHashSet;

use ruff_index::{newtype_index, IndexVec};
use ruff_python_trivia::Cursor;
use ruff_source_file::LineRanges;
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
}

#[newtype_index]
struct EmbeddedFileId;

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
    pub(crate) path: String,
    pub(crate) lang: &'s str,
    pub(crate) code: &'s str,

    /// The offset of the backticks beginning the code block within the markdown file
    pub(crate) md_offset: TextSize,
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
    unnamed_file_count: usize,

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
    current_section_files: Option<FxHashSet<String>>,

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
        });
        Self {
            sections,
            source,
            files: IndexVec::default(),
            unnamed_file_count: 0,
            cursor: Cursor::new(source),
            preceding_blank_lines: 0,
            explicit_path: None,
            source_len: source.text_len(),
            stack: SectionStack::new(root_section_id),
            current_section_files: None,
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
            let c = self.cursor.first();
            if end_predicate(c) {
                return Some(&self.source[start..self.offset().to_usize()]);
            }
            self.cursor.bump();
        }

        None
    }

    fn parse_impl(&mut self) -> anyhow::Result<()> {
        const CODE_BLOCK_END: &[u8] = b"```";

        while let Some(first) = self.cursor.bump() {
            match first {
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

                            self.process_code_block(lang, code)?;
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
        };

        if self.current_section_files.is_some() {
            bail!(
                "Header '{}' not valid inside a test case; parent '{}' has code files.",
                section.title,
                self.sections[parent].title,
            );
        }

        let section_id = self.sections.push(section);
        self.stack.push(section_id);

        self.current_section_files = None;
        self.current_section_has_config = false;

        Ok(())
    }

    fn process_code_block(&mut self, lang: &'s str, code: &'s str) -> anyhow::Result<()> {
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
            Some(path) => path.to_string(),
            None => {
                self.unnamed_file_count += 1;

                match lang {
                    "py" | "pyi" => format!("mdtest_snippet__{}.{lang}", self.unnamed_file_count),
                    "" => format!("mdtest_snippet__{}.py", self.unnamed_file_count),
                    _ => {
                        bail!(
                            "Cannot generate name for `lang={}`: Unsupported extension",
                            lang
                        );
                    }
                }
            }
        };

        self.files.push(EmbeddedFile {
            path: path.clone(),
            section,
            lang,
            code,
            md_offset: self.offset(),
        });

        if let Some(current_files) = &mut self.current_section_files {
            if !current_files.insert(path.clone()) {
                bail!(
                    "Test `{}` has duplicate files named `{path}`.",
                    self.sections[section].title
                );
            };
        } else {
            self.current_section_files = Some(FxHashSet::from_iter([path]));
        }

        Ok(())
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

    fn pop_sections_to_level(&mut self, level: usize) {
        while level <= self.sections[self.stack.top()].level.into() {
            self.stack.pop();
            // We would have errored before pushing a child section if there were files, so we know
            // no parent section can have files.
            self.current_section_files = None;
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

        assert_eq!(file.path, "mdtest_snippet__1.py");
        assert_eq!(file.lang, "py");
        assert_eq!(file.code, "x = 1");
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

        assert_eq!(file.path, "mdtest_snippet__1.py");
        assert_eq!(file.lang, "py");
        assert_eq!(file.code, "x = 1");
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

            ```pyi
            a: int
            ```

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

        assert_eq!(file.path, "mdtest_snippet__1.py");
        assert_eq!(file.lang, "py");
        assert_eq!(file.code, "x = 1");

        let [file] = test2.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path, "mdtest_snippet__2.py");
        assert_eq!(file.lang, "py");
        assert_eq!(file.code, "y = 2");

        let [file_1, file_2] = test3.files().collect::<Vec<_>>()[..] else {
            panic!("expected two files");
        };

        assert_eq!(file_1.path, "mdtest_snippet__3.pyi");
        assert_eq!(file_1.lang, "pyi");
        assert_eq!(file_1.code, "a: int");

        assert_eq!(file_2.path, "mdtest_snippet__4.pyi");
        assert_eq!(file_2.lang, "pyi");
        assert_eq!(file_2.code, "b: str");
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

        assert_eq!(main.path, "main.py");
        assert_eq!(main.lang, "py");
        assert_eq!(main.code, "from foo import y");

        assert_eq!(foo.path, "foo.py");
        assert_eq!(foo.lang, "py");
        assert_eq!(foo.code, "y = 2");

        let [file] = test2.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path, "mdtest_snippet__1.py");
        assert_eq!(file.lang, "py");
        assert_eq!(file.code, "y = 2");
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

        assert_eq!(file.path, "foo.py");
        assert_eq!(file.lang, "py");
        assert_eq!(file.code, "x = 1");
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

        assert_eq!(file.code, "x = 1\ny = 2");
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

        assert_eq!(file.code, "");
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

        let mf = super::parse("file.md", &source).unwrap();

        let [test] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected one test");
        };
        let [file] = test.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.code, "x = 10");
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
            "Cannot generate name for `lang=json`: Unsupported extension"
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

        assert_eq!(file.path, "lorem");
        assert_eq!(file.code, "x = 1");
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

        assert_eq!(file.path, "lorem.yaml");
        assert_eq!(file.code, "x = 1");
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

            ```
            y = 2
            ```

            ## A not-so-well-fenced block

            ```
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
        super::parse("file.md", &source).expect_err("Indented code blocks are not supported.");
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
        assert_eq!(file.path, "mdtest_snippet__1.py");
        assert_eq!(file.code, "x = 1");
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
        assert_eq!(file.path, "mdtest_snippet__1.py");
        assert_eq!(file.code, "x = 1");
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
    fn no_duplicate_name_files_in_test_2() {
        let source = dedent(
            "
            `mdtest_snippet__1.py`:

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
            "Test `file.md` has duplicate files named `mdtest_snippet__1.py`."
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

        assert_eq!(file.path, "foo.py");
        assert_eq!(file.code, "x = 1");
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

        assert_eq!(file.path, "foo.py");
        assert_eq!(file.code, "x = 1");
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

        assert_eq!(file.path, "foo.py");
        assert_eq!(file.code, "x = 1");
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

        assert_eq!(file.path, "foo bar.py");
        assert_eq!(file.code, "x = 1");
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

        assert_eq!(file.path, "mdtest_snippet__1.py");
        assert_eq!(file.code, "x = 1");
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

        assert_eq!(file.path, "mdtest_snippet__1.py");
        assert_eq!(file.code, "x = 1");
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

        assert_eq!(file.path, "mdtest_snippet__1.py");
        assert_eq!(file.code, "x = 1");
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

        assert_eq!(file.path, "mdtest_snippet__1.py");
        assert_eq!(file.code, "x = 1");
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
}
