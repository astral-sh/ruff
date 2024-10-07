use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use ruff_index::{newtype_index, IndexVec};
use rustc_hash::{FxHashMap, FxHashSet};

/// Parse the Markdown `source` as a test suite with given `title`.
pub(crate) fn parse<'s>(title: &'s str, source: &'s str) -> anyhow::Result<MarkdownTestSuite<'s>> {
    let parser = Parser::new(title, source);
    parser.parse()
}

#[newtype_index]
struct SectionId;

#[newtype_index]
struct FileId;

/// A parsed markdown file containing tests.
///
/// Borrows from the source string and filepath it was created from.
#[derive(Debug)]
pub(crate) struct MarkdownTestSuite<'s> {
    /// Header sections.
    sections: IndexVec<SectionId, Section<'s>>,

    /// Test files embedded within the Markdown file.
    files: IndexVec<FileId, File<'s>>,
}

impl<'s> MarkdownTestSuite<'s> {
    pub(crate) fn tests<'m>(&'m self) -> MarkdownTestIterator<'m, 's> {
        MarkdownTestIterator {
            suite: self,
            current_file_index: 0,
        }
    }
}

#[derive(Debug)]
pub(crate) struct MarkdownTest<'m, 's> {
    suite: &'m MarkdownTestSuite<'s>,
    section: &'m Section<'s>,
    files: &'m [File<'s>],
}

impl<'m, 's> MarkdownTest<'m, 's> {
    pub(crate) fn name(&self) -> String {
        let mut name = self.section.title.to_string();
        let mut parent_id = self.section.parent_id;
        while let Some(parent) = parent_id.map(|section_id| &self.suite.sections[section_id]) {
            parent_id = parent.parent_id;
            name.insert_str(0, " - ");
            name.insert_str(0, parent.title);
        }
        name
    }

    pub(crate) fn files(&self) -> impl Iterator<Item = &'m File<'s>> {
        self.files.iter()
    }
}

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
        let files = &self.suite.files
            [FileId::from_usize(self.current_file_index)..FileId::from_usize(current_file_index)];
        self.current_file_index = current_file_index;
        Some(MarkdownTest {
            suite: self.suite,
            section: &self.suite.sections[section_id],
            files,
        })
    }
}

#[derive(Debug)]
struct Section<'s> {
    title: &'s str,
    level: usize,
    parent_id: Option<SectionId>,
}

#[derive(Debug)]
pub(crate) struct File<'s> {
    section: SectionId,
    pub(crate) path: &'s str,
    pub(crate) lang: &'s str,
    pub(crate) code: &'s str,
}

static HEADER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(\s*\n)*(?<level>#+)\s+(?<title>.+)\s*\n").unwrap());

static CODE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^```(?<lang>\w+)(?<config>( +\S+)*)\s*\n(?<code>(.|\n)*?)\n```\s*\n").unwrap()
});

static IGNORE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^.*\n?").unwrap());

#[derive(Debug)]
struct Parser<'s> {
    /// Sections of the final [`MarkdownTestSuite`].
    sections: IndexVec<SectionId, Section<'s>>,

    /// Files of the final [`MarkdownTestSuite`].
    files: IndexVec<FileId, File<'s>>,

    /// The unparsed remainder of the Markdown source.
    rest: &'s str,

    /// Stack of ancestor sections.
    stack: Vec<SectionId>,

    /// True if current active section (last in stack) has files.
    current_section_files: Option<FxHashSet<&'s str>>,
}

impl<'s> Parser<'s> {
    fn new(title: &'s str, source: &'s str) -> Self {
        let mut sections = IndexVec::default();
        let root_section_id = sections.push(Section {
            title,
            level: 0,
            parent_id: None,
        });
        Self {
            sections,
            files: IndexVec::default(),
            rest: source,
            stack: vec![root_section_id],
            current_section_files: None,
        }
    }

    fn parse(mut self) -> anyhow::Result<MarkdownTestSuite<'s>> {
        self.start()?;
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

    fn start(&mut self) -> anyhow::Result<()> {
        while !self.rest.is_empty() {
            if let Some(caps) = self.scan(&HEADER_RE) {
                let header_level = caps["level"].len();
                self.pop_sections_to_level(header_level);

                // We never pop the implicit root section.
                let parent = self.stack.last().unwrap();

                let section = Section {
                    // HEADER_RE can't match without a match for group 'title'.
                    title: caps.name("title").unwrap().into(),
                    level: header_level,
                    parent_id: Some(*parent),
                };

                if self.current_section_files.is_some() {
                    return Err(anyhow::anyhow!(
                        "Header '{}' not valid inside a test case; parent '{}' has code files.",
                        section.title,
                        self.sections[*parent].title,
                    ));
                }

                let section_id = self.sections.push(section);
                self.stack.push(section_id);

                self.current_section_files = None;
            } else if let Some(caps) = self.scan(&CODE_RE) {
                // We never pop the implicit root section.
                let parent = self.stack.last().unwrap();

                let mut config: FxHashMap<&'s str, &'s str> = FxHashMap::default();

                if let Some(config_match) = caps.name("config") {
                    for item in config_match.as_str().split_whitespace() {
                        let mut parts = item.split('=');
                        let key = parts.next().unwrap();
                        let Some(val) = parts.next() else {
                            return Err(anyhow::anyhow!("Invalid config item `{}`.", item));
                        };
                        if parts.next().is_some() {
                            return Err(anyhow::anyhow!("Invalid config item `{}`.", item));
                        }
                        config.insert(key, val);
                    }
                }

                let path = config.get("path").unwrap_or(&"test.py");

                self.files.push(File {
                    path,
                    section: *parent,
                    // CODE_RE can't match without matches for 'lang' and 'code'.
                    lang: caps.name("lang").unwrap().into(),
                    code: caps.name("code").unwrap().into(),
                });

                if let Some(current_files) = &mut self.current_section_files {
                    if current_files.contains(*path) {
                        let current = &self.sections[*self.stack.last().unwrap()];
                        if *path == "test.py" {
                            return Err(anyhow::anyhow!(
                                "Test `{}` has duplicate files named `{path}`. \
                                (This is the default filename; \
                                 consider giving some files an explicit name with `path=...`.)",
                                current.title
                            ));
                        }
                        return Err(anyhow::anyhow!(
                            "Test `{}` has duplicate files named `{path}`.",
                            current.title
                        ));
                    }
                    current_files.insert(*path);
                } else {
                    self.current_section_files = Some(FxHashSet::from_iter([*path]));
                }
            } else {
                self.scan(&IGNORE_RE);
            }
        }

        Ok(())
    }

    fn pop_sections_to_level(&mut self, level: usize) {
        while self
            .stack
            .last()
            .is_some_and(|section_id| level <= self.sections[*section_id].level)
        {
            self.stack.pop();
            // We would have errored before pushing a child section if there were files, so we know
            // no parent section can have files.
            self.current_section_files = None;
        }
    }

    fn scan(&mut self, pattern: &Regex) -> Option<Captures<'s>> {
        if let Some(caps) = pattern.captures(self.rest) {
            let (_, rest) = self.rest.split_at(caps.get(0).unwrap().end());
            self.rest = rest;
            Some(caps)
        } else {
            None
        }
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
        let mf = super::parse("file.md", &source);
        assert!(
            mf.as_ref().is_err_and(|err| err.to_string()
                == "Header 'Two' not valid inside a test case; parent 'One' has code files."),
            "Unexpected parse result: {mf:?}",
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
        let mf = super::parse("file.md", &source);
        assert!(
            mf.as_ref()
                .is_err_and(|err| err.to_string() == "Invalid config item `foo`."),
            "Unexpected parse result: {mf:?}",
        );
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
        let mf = super::parse("file.md", &source);
        assert!(
            mf.as_ref()
                .is_err_and(|err| err.to_string() == "Invalid config item `foo=bar=baz`."),
            "Unexpected parse result: {mf:?}",
        );
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
        let mf = super::parse("file.md", &source);
        assert!(
            mf.as_ref().is_err_and(|err| err.to_string()
                == "Test `file.md` has duplicate files named `test.py`. \
                (This is the default filename; consider giving some files an explicit name \
                 with `path=...`.)"),
            "Unexpected parse result: {mf:?}",
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
        let mf = super::parse("file.md", &source);
        assert!(
            mf.as_ref().is_err_and(
                |err| err.to_string() == "Test `file.md` has duplicate files named `foo.py`."
            ),
            "Unexpected parse result: {mf:?}",
        );
    }
}
