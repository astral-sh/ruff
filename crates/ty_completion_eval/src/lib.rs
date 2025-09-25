/*!
# Caveats

Source files have the substring `<CURSOR>` removed and its position recorded.
If this substring occurs more than once (or less than once) throughout a project,
then that particular test is considered invalid.
*/

use anyhow::{Context, anyhow};
use memchr::memmem;

use ruff_db::files::system_path_to_file;
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf};
use ty_ide::Completion;
use ty_project::{ProjectDatabase, ProjectMetadata};

pub fn run() -> anyhow::Result<()> {
    // The base path to which all CLI arguments are relative to.
    let cwd = {
        let cwd = std::env::current_dir().context("Failed to get the current working directory")?;
        SystemPathBuf::from_path_buf(cwd).map_err(|path| {
            anyhow!(
                "The current working directory `{}` contains non-Unicode characters. \
                 ty only supports Unicode paths.",
                path.display()
            )
        })?
    };
    // Where we store our truth data.
    let truth = cwd.join("crates").join("ty_completion_eval").join("truth");
    anyhow::ensure!(
        truth.as_std_path().exists(),
        "{truth} does not exist: ty's completion evaluation must be run from the root \
         of the ruff repository",
        truth = truth.as_std_path().display(),
    );
    // The temporary directory at which we copy our truth
    // data to. We do this because we can't use the truth
    // data as-is with its `<CURSOR>` annotations (and perhaps
    // any other future annotations we add). Plus, it seems
    // better to keep the `.venv` directories outside of the
    // repository checkout anyway.
    let tmp_eval_dir = SystemPath::new("/tmp/ty-completion-eval");
    std::fs::create_dir_all(tmp_eval_dir).with_context(|| tmp_eval_dir.to_string())?;

    for source in TestSource::all(&truth)? {
        let test = source.into_test(&tmp_eval_dir)?;
        for c in test.completions()? {
            if let Some(ref edit) = c.import {
                println!("{} import {:?}", c.name, edit.content());
            } else {
                println!("{}", c.name);
            }
        }
        dbg!(&test.answer);
    }

    Ok(())
}

/// A test corresponding to a Python project.
///
/// The test is oriented in such a way that we have a single
/// "cursor" position. This allows us to ask for completions
/// at that position.
struct Test {
    db: ProjectDatabase,
    dir: SystemPathBuf,
    name: String,
    cursor: Cursor,
    answer: CompletionAnswer,
    settings: ty_ide::CompletionSettings,
}

impl Test {
    fn new(
        project_path: &SystemPath,
        truth: CompletionTruth,
        cursor: Cursor,
    ) -> anyhow::Result<Test> {
        let name = project_path.file_name().ok_or_else(|| {
            anyhow::anyhow!("project directory `{project_path}` does not contain a base name")
        })?;

        let system = OsSystem::new(project_path);
        let mut project_metadata = ProjectMetadata::discover(&project_path, &system)?;
        project_metadata.apply_configuration_files(&system)?;
        let db = ProjectDatabase::new(project_metadata, system)?;
        Ok(Test {
            db,
            dir: project_path.to_path_buf(),
            name: name.to_string(),
            cursor,
            answer: truth.answer,
            settings: truth.settings.into(),
        })
    }

    fn completions(&self) -> anyhow::Result<Vec<Completion<'_>>> {
        let file = system_path_to_file(&self.db, &self.cursor.path)
            .with_context(|| format!("failed to get database file for `{}`", self.cursor.path))?;
        let offset = ruff_text_size::TextSize::try_from(self.cursor.offset).with_context(|| {
            format!(
                "failed to convert `<CURSOR>` file offset `{}` to 32-bit integer",
                self.cursor.offset
            )
        })?;
        let completions = ty_ide::completion(&self.db, &self.settings, file, offset);
        Ok(completions)
    }
}

impl std::fmt::Debug for Test {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Test")
            .field("db", &"<ProjectDatabase>")
            .field("dir", &self.dir)
            .field("name", &self.name)
            .field("cursor", &self.cursor)
            .field("answer", &self.answer)
            .field("settings", &self.settings)
            .finish()
    }
}

/// Truth data for a single completion evaluation test.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CompletionTruth {
    answer: CompletionAnswer,
    #[serde(default)]
    settings: CompletionSettings,
}

/// The answer for this completion test.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CompletionAnswer {
    symbol: String,
    module: Option<String>,
}

/// Settings to forward to our completion routine.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CompletionSettings {
    #[serde(default)]
    auto_import: bool,
}

impl From<CompletionSettings> for ty_ide::CompletionSettings {
    fn from(x: CompletionSettings) -> ty_ide::CompletionSettings {
        ty_ide::CompletionSettings {
            auto_import: x.auto_import,
        }
    }
}

/// The "source" of a test, as found in ty's git repository.
#[derive(Debug)]
struct TestSource {
    /// The directory containing this test.
    dir: SystemPathBuf,
    /// The name of this test (the basename of `dir`).
    name: String,
    /// The "truth" data for this test along with any
    /// settings. This is pulled from `{dir}/completion.toml`.
    truth: CompletionTruth,
}

impl TestSource {
    fn all(src_dir: &SystemPath) -> anyhow::Result<Vec<TestSource>> {
        let mut sources = vec![];
        let read_dir = src_dir
            .as_std_path()
            .read_dir()
            .with_context(|| format!("failed to read directory entries in `{src_dir}`"))?;
        for result in read_dir {
            let dent = result
                .with_context(|| format!("failed to get directory entry from `{src_dir}`"))?;
            let path = dent.path();
            if !path.is_dir() {
                continue;
            }

            let dir = SystemPath::from_std_path(&path).ok_or_else(|| {
                anyhow::anyhow!(
                    "truth source directory `{path}` contains invalid UTF-8",
                    path = path.display()
                )
            })?;
            sources.push(TestSource::new(&dir)?);
        }
        Ok(sources)
    }

    fn new(dir: &SystemPath) -> anyhow::Result<TestSource> {
        let name = dir.file_name().ok_or_else(|| {
            anyhow::anyhow!("truth source directory `{dir}` does not contain a base name")
        })?;

        let truth_path = dir.join("completion.toml");
        let truth_data = std::fs::read(truth_path.as_std_path())
            .with_context(|| format!("failed to read truth data at `{truth_path}`"))?;
        let truth = toml::from_slice(&truth_data).with_context(|| {
            format!("failed to parse TOML completion truth data from `{truth_path}`")
        })?;

        Ok(TestSource {
            dir: dir.to_path_buf(),
            name: name.to_string(),
            truth,
        })
    }

    /// Convert this "source" test (from the Ruff repository) into a test we
    /// can mutate and evaluate in a temporary directory.
    ///
    /// This includes running `uv sync` to set up a full virtual environment.
    fn into_test(self, parent_dst_dir: &SystemPath) -> anyhow::Result<Test> {
        let dir = parent_dst_dir.join(&self.name);
        let cursor = copy_project(&self.dir, &dir)?;
        let uv_sync_output = std::process::Command::new("uv")
            .arg("sync")
            .current_dir(dir.as_std_path())
            .output()
            .with_context(|| format!("failed to run `uv sync` in `{dir}`"))?;
        if !uv_sync_output.status.success() {
            let code = uv_sync_output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "UNKNOWN".to_string());
            let stderr = bstr::BStr::new(&uv_sync_output.stderr);
            anyhow::bail!("`uv sync` failed to run with exit code `{code}`, stderr: {stderr}")
        }
        Test::new(&dir, self.truth, cursor)
    }
}

/// The location of `<CURSOR>` within a single Python project.
#[derive(Debug)]
struct Cursor {
    path: SystemPathBuf,
    offset: usize,
}

/// Copy the Python project from `src_dir` to `dst_dir`.
///
/// This also looks for a singular occurrence of `<CURSOR>`
/// among the project files and returns its position. The
/// original `<CURSOR>` string is deleted.
fn copy_project(src_dir: &SystemPath, dst_dir: &SystemPath) -> anyhow::Result<Cursor> {
    std::fs::create_dir_all(dst_dir).with_context(|| dst_dir.to_string())?;

    let mut cursor: Option<Cursor> = None;
    let read_dir = src_dir
        .as_std_path()
        .read_dir()
        .with_context(|| format!("failed to read directory entries in {src_dir}"))?;
    for result in read_dir {
        let dent =
            result.with_context(|| format!("failed to get directory entry from {src_dir}"))?;
        let src = SystemPathBuf::from_path_buf(dent.path())
            .map_err(|_| anyhow::anyhow!("path `{}` is not valid UTF-8", dent.path().display()))?;
        let name = src
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("path `{src}` is missing a basename"))?;
        let dst = dst_dir.join(name);
        if let Some(new_cursor) = copy_file(&src, &dst)? {
            if let Some(cursor) = cursor {
                anyhow::bail!(
                    "found `<CURSOR>` in both `{path1}` and `{path2}`, \
                     but it must occur in exactly one file",
                    path1 = cursor.path,
                    path2 = new_cursor.path,
                );
            }
            cursor = Some(new_cursor);
        }
    }
    cursor.ok_or_else(|| {
        anyhow::anyhow!(
            "could not find any `<CURSOR>` substring in any of the files in `{src_dir}`",
        )
    })
}

/// Copies `src` to `dst` while looking for `<CURSOR>`.
///
/// If a `<CURSOR>` is found, then it is replaced with the empty string
/// and its position is returned.
///
/// # Errors
///
/// When an underlying I/O error occurs or when `<CURSOR>` occurs more than
/// once.
fn copy_file(src: &SystemPath, dst: &SystemPath) -> anyhow::Result<Option<Cursor>> {
    static CURSOR: &[u8] = b"<CURSOR>";

    let src_data =
        std::fs::read(src).with_context(|| format!("failed to read `{src}` for copying"))?;
    let mut cursor = None;
    let mut new = Vec::with_capacity(src_data.len());
    let mut written_to = 0;
    for (i, offset) in memmem::find_iter(&src_data, CURSOR).enumerate() {
        anyhow::ensure!(
            i == 0,
            "found `<CURSOR>` more than once in `{src}` (must occur at most once)",
        );

        new.extend_from_slice(&src_data[written_to..offset]);
        written_to = offset + CURSOR.len();
        cursor = Some(Cursor {
            path: dst.to_path_buf(),
            offset,
        });
    }
    new.extend_from_slice(&src_data[written_to..]);
    std::fs::write(dst, &new)
        .with_context(|| format!("failed to write contents of `{src}` to `{dst}`"))?;
    Ok(cursor)
}
