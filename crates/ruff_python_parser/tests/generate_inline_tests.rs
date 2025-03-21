//! This module takes specially formatted comments from `ruff_python_parser` code
//! and turns them into test fixtures. The code is derived from `rust-analyzer`
//! and `biome`.
//!
//! References:
//! - <https://github.com/rust-lang/rust-analyzer/blob/e4a405f877efd820bef9c0e77a02494e47c17512/crates/parser/src/tests/sourcegen_inline_tests.rs>
//! - <https://github.com/biomejs/biome/blob/b9f8ffea9967b098ec4c8bf74fa96826a879f043/xtask/codegen/src/parser_tests.rs>
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::ops::{AddAssign, Deref, DerefMut};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .canonicalize()
        .unwrap()
}

#[test]
fn generate_inline_tests() -> Result<()> {
    let parser_dir = project_root().join("crates/ruff_python_parser/src/");
    let tests = TestCollection::try_from(parser_dir.as_path())?;

    let mut test_files = TestFiles::default();
    test_files += install_tests(&tests.ok, "crates/ruff_python_parser/resources/inline/ok")?;
    test_files += install_tests(&tests.err, "crates/ruff_python_parser/resources/inline/err")?;

    if !test_files.is_empty() {
        anyhow::bail!("{}", test_files);
    }

    Ok(())
}

#[derive(Debug, Default)]
struct TestFiles {
    unreferenced: Vec<PathBuf>,
    updated: Vec<PathBuf>,
}

impl TestFiles {
    fn is_empty(&self) -> bool {
        self.unreferenced.is_empty() && self.updated.is_empty()
    }
}

impl AddAssign<TestFiles> for TestFiles {
    fn add_assign(&mut self, other: TestFiles) {
        self.unreferenced.extend(other.unreferenced);
        self.updated.extend(other.updated);
    }
}

impl fmt::Display for TestFiles {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            writeln!(f, "No unreferenced or updated test files found")
        } else {
            let root_dir = project_root();
            if !self.unreferenced.is_empty() {
                writeln!(
                    f,
                    "Unreferenced test files found for which no comment exists:",
                )?;
                for path in &self.unreferenced {
                    writeln!(f, "  {}", path.strip_prefix(&root_dir).unwrap().display())?;
                }
                writeln!(f, "Please delete these files manually")?;
            }
            if !self.updated.is_empty() {
                if !self.unreferenced.is_empty() {
                    writeln!(f)?;
                }
                writeln!(
                    f,
                    "Following files were not up-to date and has been updated:",
                )?;
                for path in &self.updated {
                    writeln!(f, "  {}", path.strip_prefix(&root_dir).unwrap().display())?;
                }
                writeln!(
                    f,
                    "Re-run the tests with `cargo test` to update the test snapshots"
                )?;
                if std::env::var("CI").is_ok() {
                    writeln!(
                        f,
                        "NOTE: Run the tests locally and commit the updated files"
                    )?;
                }
            }
            Ok(())
        }
    }
}

fn install_tests(tests: &HashMap<String, Test>, target_dir: &str) -> Result<TestFiles> {
    let root_dir = project_root();
    let tests_dir = root_dir.join(target_dir);
    if !tests_dir.is_dir() {
        fs::create_dir_all(&tests_dir)?;
    }

    // Test kind is irrelevant for existing test cases.
    let existing = existing_tests(&tests_dir)?;

    let mut updated_files = vec![];

    for (name, test) in tests {
        let path = match existing.get(name) {
            Some(path) => path.clone(),
            None => tests_dir.join(name).with_extension("py"),
        };
        match fs::read_to_string(&path) {
            Ok(old_contents) if old_contents == test.contents => continue,
            _ => {}
        }
        fs::write(&path, &test.contents)
            .with_context(|| format!("Failed to write to {:?}", path.display()))?;
        updated_files.push(path);
    }

    Ok(TestFiles {
        unreferenced: existing
            .into_iter()
            .filter(|(name, _)| !tests.contains_key(name))
            .map(|(_, path)| path)
            .collect::<Vec<_>>(),
        updated: updated_files,
    })
}

#[derive(Default, Debug)]
struct TestCollection {
    ok: HashMap<String, Test>,
    err: HashMap<String, Test>,
}

impl TryFrom<&Path> for TestCollection {
    type Error = anyhow::Error;

    fn try_from(path: &Path) -> Result<Self> {
        let mut tests = TestCollection::default();

        for entry in walkdir::WalkDir::new(path) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension().unwrap_or_default() != "rs" {
                continue;
            }
            let text = fs::read_to_string(entry.path())?;
            for test in collect_tests(&text) {
                if test.is_ok() {
                    if let Some(old_test) = tests.ok.insert(test.name.clone(), test) {
                        anyhow::bail!(
                            "Duplicate test found: {name:?} (search '// test_ok {name}' for the location)\n",
                            name = old_test.name
                        );
                    }
                } else if let Some(old_test) = tests.err.insert(test.name.clone(), test) {
                    anyhow::bail!(
                        "Duplicate test found: {name:?} (search '// test_err {name}' for the location)\n",
                        name = old_test.name
                    );
                }
            }
        }

        Ok(tests)
    }
}

#[derive(Debug, Clone, Copy)]
enum TestKind {
    Ok,
    Err,
}

/// A test of the following form:
///
/// ```text
/// // (test_ok|test_err) name
/// // <code>
/// ```
#[derive(Debug)]
struct Test {
    name: String,
    contents: String,
    kind: TestKind,
}

impl Test {
    const fn is_ok(&self) -> bool {
        matches!(self.kind, TestKind::Ok)
    }
}

/// Collect the tests from the given source text.
fn collect_tests(text: &str) -> Vec<Test> {
    let mut tests = Vec::new();

    for comment_block in extract_comment_blocks(text) {
        let first_line = &comment_block[0];

        let (kind, name) = match first_line.split_once(' ') {
            Some(("test_ok", suffix)) => (TestKind::Ok, suffix),
            Some(("test_err", suffix)) => (TestKind::Err, suffix),
            _ => continue,
        };

        let text: String = comment_block[1..]
            .iter()
            .cloned()
            .chain([String::new()])
            .collect::<Vec<_>>()
            .join("\n");

        assert!(!text.trim().is_empty() && text.ends_with('\n'));

        tests.push(Test {
            name: name.to_string(),
            contents: text,
            kind,
        });
    }

    tests
}

#[derive(Debug, Default)]
struct CommentBlock(Vec<String>);

impl Deref for CommentBlock {
    type Target = Vec<String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CommentBlock {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Extract the comment blocks from the given source text.
///
/// A comment block is a sequence of lines that start with `// ` and are separated
/// by an empty line. An empty comment line (`//`) is also part of the block.
fn extract_comment_blocks(text: &str) -> Vec<CommentBlock> {
    const COMMENT_PREFIX: &str = "// ";
    const COMMENT_PREFIX_LEN: usize = COMMENT_PREFIX.len();

    let mut comment_blocks = Vec::new();
    let mut block = CommentBlock::default();

    for line in text.lines().map(str::trim_start) {
        if line == "//" {
            block.push(String::new());
            continue;
        }

        if line.starts_with(COMMENT_PREFIX) {
            block.push(line[COMMENT_PREFIX_LEN..].to_string());
        } else {
            if !block.is_empty() {
                comment_blocks.push(std::mem::take(&mut block));
            }
        }
    }
    if !block.is_empty() {
        comment_blocks.push(block);
    }
    comment_blocks
}

/// Returns the existing tests in the given directory.
fn existing_tests(dir: &Path) -> Result<HashMap<String, PathBuf>> {
    let mut tests = HashMap::new();

    for file in fs::read_dir(dir)? {
        let path = file?.path();
        if path.extension().unwrap_or_default() != "py" {
            continue;
        }
        let name = path
            .file_stem()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap();
        if let Some(old) = tests.insert(name, path) {
            anyhow::bail!("Multiple test file exists for {old:?}");
        }
    }

    Ok(tests)
}
