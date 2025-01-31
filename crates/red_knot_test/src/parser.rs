use std::sync::LazyLock;

use anyhow::bail;
use memchr::memchr2;
use regex::{Captures, Match, Regex};
use rustc_hash::{FxHashMap, FxHashSet};

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
    pub(crate) path: &'s str,
    pub(crate) lang: &'s str,
    pub(crate) code: &'s str,

    /// The offset of the backticks beginning the code block within the markdown file
    pub(crate) md_offset: TextSize,
}

/// Matches a sequence of `#` characters, followed by a title heading, followed by a newline.
static HEADER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?<level>#+)\s+(?<title>.+)\s*\n").unwrap());

/// Matches a code block fenced by triple backticks, possibly with language and `key=val`
/// configuration items following the opening backticks (in the "tag string" of the code block).
static CODE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        ^```(?<lang>(?-u:\w)+)?(?<config>(?:\x20+\S+)*)\s*\n
        (?<code>(?:.|\n)*?)\n?
        (?<end>```|\z)
        ",
    )
    .unwrap()
});

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

    source: &'s str,
    source_len: TextSize,

    /// Stack of ancestor sections.
    stack: SectionStack,

    /// Names of embedded files in current active section.
    current_section_files: Option<FxHashSet<&'s str>>,

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
            cursor: Cursor::new(source),
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

    fn parse_impl(&mut self) -> anyhow::Result<()> {
        while let Some(position) = memchr2(b'`', b'#', self.cursor.as_bytes()) {
            self.cursor.skip_bytes(position.saturating_sub(1));

            // code blocks and headers must start on a new line.
            if position == 0 || self.cursor.eat_char('\n') {
                match self.cursor.first() {
                    '#' => {
                        if let Some(find) = HEADER_RE.find(self.cursor.as_str()) {
                            self.parse_header(find.as_str())?;
                            self.cursor.skip_bytes(find.len());
                            continue;
                        }
                    }
                    '`' => {
                        if let Some(captures) = CODE_RE.captures(self.cursor.as_str()) {
                            self.parse_code_block(&captures)?;
                            self.cursor.skip_bytes(captures.get(0).unwrap().len());
                            continue;
                        }
                    }
                    _ => unreachable!(),
                }
            }

            // Skip to the end of the line
            if let Some(position) = memchr::memchr(b'\n', self.cursor.as_bytes()) {
                self.cursor.skip_bytes(position);
            } else {
                break;
            }
        }

        Ok(())
    }

    fn parse_header(&mut self, header: &'s str) -> anyhow::Result<()> {
        let mut trimmed = header.trim();

        let mut header_level = 0usize;
        while let Some(rest) = trimmed.strip_prefix('#') {
            header_level += 1;
            trimmed = rest;
        }

        let title = trimmed.trim_start();

        self.pop_sections_to_level(header_level);

        let parent = self.stack.top();

        let section = Section {
            title,
            level: header_level.try_into()?,
            parent_id: Some(parent),
            config: self.sections[parent].config.clone(),
        };

        if self.current_section_files.is_some() {
            return Err(anyhow::anyhow!(
                "Header '{}' not valid inside a test case; parent '{}' has code files.",
                section.title,
                self.sections[parent].title,
            ));
        }

        let section_id = self.sections.push(section);
        self.stack.push(section_id);

        self.current_section_files = None;
        self.current_section_has_config = false;

        Ok(())
    }

    fn parse_code_block(&mut self, captures: &Captures<'s>) -> anyhow::Result<()> {
        // We never pop the implicit root section.
        let section = self.stack.top();

        if captures.name("end").unwrap().is_empty() {
            let code_block_start = self.cursor.token_len();
            let line = self.source.count_lines(TextRange::up_to(code_block_start)) + 1;

            return Err(anyhow::anyhow!("Unterminated code block at line {line}."));
        }

        let mut config: FxHashMap<&'s str, &'s str> = FxHashMap::default();

        if let Some(config_match) = captures.name("config") {
            for item in config_match.as_str().split_whitespace() {
                let mut parts = item.split('=');
                let key = parts.next().unwrap();
                let Some(val) = parts.next() else {
                    return Err(anyhow::anyhow!("Invalid config item `{}`.", item));
                };
                if parts.next().is_some() {
                    return Err(anyhow::anyhow!("Invalid config item `{}`.", item));
                }
                if config.insert(key, val).is_some() {
                    return Err(anyhow::anyhow!("Duplicate config item `{}`.", item));
                }
            }
        }

        let path = config.get("path").copied().unwrap_or("test.py");

        // CODE_RE can't match without matches for 'lang' and 'code'.
        let lang = captures
            .name("lang")
            .as_ref()
            .map(Match::as_str)
            .unwrap_or_default();
        let code = captures.name("code").unwrap().into();

        if lang == "toml" {
            return self.parse_config(code);
        }

        self.files.push(EmbeddedFile {
            path,
            section,
            lang,

            code,

            md_offset: self.offset(),
        });

        if let Some(current_files) = &mut self.current_section_files {
            if !current_files.insert(path) {
                if path == "test.py" {
                    return Err(anyhow::anyhow!(
                        "Test `{}` has duplicate files named `{path}`. \
                                (This is the default filename; \
                                 consider giving some files an explicit name with `path=...`.)",
                        self.sections[section].title
                    ));
                }
                return Err(anyhow::anyhow!(
                    "Test `{}` has duplicate files named `{path}`.",
                    self.sections[section].title
                ));
            };
        } else {
            self.current_section_files = Some(FxHashSet::from_iter([path]));
        }

        Ok(())
    }

    fn parse_config(&mut self, code: &str) -> anyhow::Result<()> {
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

        assert_eq!(file.path, "test.py");
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

        assert_eq!(file.path, "test.py");
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
            ",
        );
        let mf = super::parse("file.md", &source).unwrap();

        let [test1, test2] = &mf.tests().collect::<Vec<_>>()[..] else {
            panic!("expected two tests");
        };

        assert_eq!(test1.name(), "file.md - One");
        assert_eq!(test2.name(), "file.md - Two");

        let [file] = test1.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path, "test.py");
        assert_eq!(file.lang, "py");
        assert_eq!(file.code, "x = 1");

        let [file] = test2.files().collect::<Vec<_>>()[..] else {
            panic!("expected one file");
        };

        assert_eq!(file.path, "test.py");
        assert_eq!(file.lang, "py");
        assert_eq!(file.code, "y = 2");
    }

    #[test]
    fn multiple_file_tests() {
        let source = dedent(
            "
            # One

            ```py path=main.py
            from foo import y
            ```

            ```py path=foo.py
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

        assert_eq!(file.path, "test.py");
        assert_eq!(file.lang, "py");
        assert_eq!(file.code, "y = 2");
    }

    #[test]
    fn custom_file_path() {
        let source = dedent(
            "
            ```py path=foo.py
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
    fn invalid_config_item_no_equals() {
        let source = dedent(
            "
            ```py foo
            x = 1
            ```
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(err.to_string(), "Invalid config item `foo`.");
    }

    #[test]
    fn invalid_config_item_too_many_equals() {
        let source = dedent(
            "
            ```py foo=bar=baz
            x = 1
            ```
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(err.to_string(), "Invalid config item `foo=bar=baz`.");
    }

    #[test]
    fn invalid_config_item_duplicate() {
        let source = dedent(
            "
            ```py foo=bar foo=baz
            x = 1
            ```
            ",
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(err.to_string(), "Duplicate config item `foo=baz`.");
    }

    #[test]
    fn no_duplicate_name_files_in_test() {
        let source = dedent(
            "
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
            "Test `file.md` has duplicate files named `test.py`. \
            (This is the default filename; consider giving some files an explicit name \
            with `path=...`.)"
        );
    }

    #[test]
    fn no_duplicate_name_files_in_test_non_default() {
        let source = dedent(
            "
            ```py path=foo.py
            x = 1
            ```

            ```py path=foo.py
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
}
