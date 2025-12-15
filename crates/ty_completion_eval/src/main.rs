/*!
A simple command line tool for running a completion evaluation.

See `crates/ty_completion_eval/README.md` for examples and more docs.
*/

use std::io::Write;
use std::process::ExitCode;
use std::sync::LazyLock;

use anyhow::{Context, anyhow};
use clap::Parser;
use regex::bytes::Regex;

use ruff_db::files::system_path_to_file;
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf};
use ty_ide::Completion;
use ty_project::metadata::Options;
use ty_project::metadata::options::EnvironmentOptions;
use ty_project::metadata::value::RelativePathBuf;
use ty_project::{ProjectDatabase, ProjectMetadata};
use ty_python_semantic::ModuleName;

#[derive(Debug, clap::Parser)]
#[command(
    author,
    name = "ty_completion_eval",
    about = "Run a information retrieval evaluation on ty-powered completions."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// Run an evaluation on all tasks.
    All(AllCommand),
    /// Show the completions for a single task.
    ///
    /// This is useful for debugging one single completion task. For
    /// example, let's say you make a change to a ranking heuristic and
    /// everything looks good except for a few tasks where the rank for
    /// the expected answer regressed. Just use this command to run a
    /// specific task and you'll get the actual completions for that
    /// task printed to stdout.
    ///
    /// If the expected answer is found in the completion list, then
    /// it is marked with an `*` along with its rank.
    ShowOne(ShowOneCommand),
}

#[derive(Debug, clap::Parser)]
struct AllCommand {
    /// The mean reciprocal rank threshold that the evaluation must
    /// meet or exceed in order for the evaluation to pass.
    #[arg(
        long,
        help = "The mean reciprocal rank threshold.",
        value_name = "FLOAT",
        default_value_t = 0.001
    )]
    threshold: f64,
    /// If given, a CSV file of the results for each individual task
    /// is written to the path given.
    #[arg(
        long,
        help = "When provided, write individual task results in CSV format.",
        value_name = "FILE"
    )]
    tasks: Option<String>,
    /// Whether to keep the temporary evaluation directory around
    /// after finishing or not. Keeping it around is useful for
    /// debugging when something has gone wrong.
    #[arg(
        long,
        help = "Whether to keep the temporary evaluation directory around or not."
    )]
    keep_tmp_dir: bool,
}

#[derive(Debug, clap::Parser)]
struct ShowOneCommand {
    /// The name of one or more completion tasks to run in isolation.
    ///
    /// The name corresponds to the name of a directory in
    /// `./crates/ty_completion_eval/truth/`.
    #[arg(help = "The task name to run.", value_name = "TASK_NAME")]
    task_name: String,
    /// The name of the file, relative to the root of the
    /// Python project, that contains one or more completion
    /// tasks to run in isolation.
    #[arg(long, help = "The file name to run.", value_name = "FILE_NAME")]
    file_name: Option<String>,
    /// The index of the cursor directive within `file_name`
    /// to select.
    #[arg(
        long,
        help = "The index of the cursor directive to run.",
        value_name = "INDEX"
    )]
    index: Option<usize>,
    /// Whether to keep the temporary evaluation directory around
    /// after finishing or not. Keeping it around is useful for
    /// debugging when something has gone wrong.
    #[arg(
        long,
        help = "Whether to keep the temporary evaluation directory around or not."
    )]
    keep_tmp_dir: bool,
}

impl ShowOneCommand {
    fn matches_source_task(&self, task_source: &TaskSource) -> bool {
        self.task_name == task_source.name
    }

    fn matches_task(&self, task: &Task) -> bool {
        self.task_name == task.name
            && self
                .file_name
                .as_ref()
                .is_none_or(|name| name == task.cursor_name())
            && self.index.is_none_or(|index| index == task.cursor.index)
    }
}

fn main() -> anyhow::Result<ExitCode> {
    let args = Cli::parse();

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
    // any other future annotations we add).
    let mut tmp_eval_dir = tempfile::Builder::new()
        .prefix("ty-completion-eval-")
        .tempdir()
        .context("Failed to create temporary directory")?;
    let tmp_eval_path = SystemPath::from_std_path(tmp_eval_dir.path())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Temporary directory path is not valid UTF-8: {}",
                tmp_eval_dir.path().display()
            )
        })?
        .to_path_buf();

    let sources = TaskSource::all(&truth)?;
    match args.command {
        Command::ShowOne(ref cmd) => {
            tmp_eval_dir.disable_cleanup(cmd.keep_tmp_dir);

            let Some(source) = sources
                .iter()
                .find(|source| cmd.matches_source_task(source))
            else {
                anyhow::bail!("could not find task named `{}`", cmd.task_name);
            };
            let tasks = source.to_tasks(&tmp_eval_path)?;
            let matching: Vec<&Task> = tasks.iter().filter(|task| cmd.matches_task(task)).collect();
            anyhow::ensure!(
                !matching.is_empty(),
                "could not find any tasks matching the given criteria",
            );
            anyhow::ensure!(
                matching.len() < 2,
                "found more than one task matching the given criteria",
            );
            let task = &matching[0];
            let completions = task.completions()?;

            let mut stdout = std::io::stdout().lock();
            for (i, c) in completions.iter().enumerate() {
                write!(stdout, "{}", c.name.as_str())?;
                if let Some(module_name) = c.module_name {
                    write!(stdout, " (module: {module_name})")?;
                }
                if task.cursor.answer.matches(c) {
                    write!(stdout, " (*, {}/{})", i + 1, completions.len())?;
                }
                writeln!(stdout)?;
            }
            writeln!(stdout, "-----")?;
            writeln!(stdout, "found {} completions", completions.len())?;
            Ok(ExitCode::SUCCESS)
        }
        Command::All(AllCommand {
            threshold,
            tasks,
            keep_tmp_dir,
        }) => {
            tmp_eval_dir.disable_cleanup(keep_tmp_dir);

            let mut precision_sum = 0.0;
            let mut task_count = 0.0f64;
            let mut results_wtr = None;
            if let Some(ref tasks) = tasks {
                let mut wtr = csv::Writer::from_path(SystemPath::new(tasks))?;
                wtr.serialize(("name", "file", "index", "rank"))?;
                results_wtr = Some(wtr);
            }
            for source in &sources {
                for task in source.to_tasks(&tmp_eval_path)? {
                    task_count += 1.0;

                    let completions = task.completions()?;
                    let rank = task.rank(&completions)?;
                    precision_sum += rank.map(|rank| 1.0 / f64::from(rank)).unwrap_or(0.0);
                    if let Some(ref mut wtr) = results_wtr {
                        wtr.serialize((&task.name, &task.cursor_name(), task.cursor.index, rank))?;
                    }
                }
            }
            let mrr = precision_sum / task_count;
            if let Some(ref mut wtr) = results_wtr {
                wtr.flush()?;
            }

            let mut out = std::io::stdout().lock();
            writeln!(out, "mean reciprocal rank: {mrr:.4}")?;
            if mrr < threshold {
                writeln!(
                    out,
                    "Failure: MRR does not exceed minimum threshold of {threshold}"
                )?;
                Ok(ExitCode::FAILURE)
            } else {
                writeln!(out, "Success: MRR exceeds minimum threshold of {threshold}")?;
                Ok(ExitCode::SUCCESS)
            }
        }
    }
}

/// A single completion task.
///
/// The task is oriented in such a way that we have a single "cursor"
/// position in a Python project. This allows us to ask for completions
/// at that position.
struct Task {
    db: ProjectDatabase,
    dir: SystemPathBuf,
    name: String,
    cursor: Cursor,
    settings: ty_ide::CompletionSettings,
}

impl Task {
    /// Create a new task for the Python project at `project_path`.
    ///
    /// `truth` should correspond to the completion configuration and the
    /// expected answer for completions at the given `cursor` position.
    fn new(
        project_path: &SystemPath,
        truth: &CompletionTruth,
        cursor: Cursor,
    ) -> anyhow::Result<Task> {
        let name = project_path.file_name().ok_or_else(|| {
            anyhow::anyhow!("project directory `{project_path}` does not contain a base name")
        })?;

        let system = OsSystem::new(project_path);
        let mut project_metadata = ProjectMetadata::discover(project_path, &system)?;
        // Explicitly point ty to the .venv to avoid any set VIRTUAL_ENV variable to take precedence.
        project_metadata.apply_options(Options {
            environment: Some(EnvironmentOptions {
                python: Some(RelativePathBuf::cli(".venv")),
                ..EnvironmentOptions::default()
            }),
            ..Options::default()
        });
        project_metadata.apply_configuration_files(&system)?;
        let db = ProjectDatabase::new(project_metadata, system)?;
        Ok(Task {
            db,
            dir: project_path.to_path_buf(),
            name: name.to_string(),
            cursor,
            settings: (&truth.settings).into(),
        })
    }

    /// Returns the rank of the expected answer in the completions
    /// given.
    ///
    /// The rank is the position (one indexed) at which the expected
    /// answer appears in the slice given, or `None` if the answer
    /// isn't found at all. A position of zero is maximally correct. A
    /// missing position is maximally wrong. Anything in the middle is
    /// a grey area with a lower rank being better.
    ///
    /// Because the rank is one indexed, if this returns a rank, then
    /// it is guaranteed to be non-zero.
    fn rank(&self, completions: &[Completion<'_>]) -> anyhow::Result<Option<u32>> {
        completions
            .iter()
            .position(|completion| self.cursor.answer.matches(completion))
            .map(|rank| u32::try_from(rank + 1).context("rank of completion is too big"))
            .transpose()
    }

    /// Return completions for this task.
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

    /// Returns the file name, relative to this project's root
    /// directory, that contains the cursor directive that we
    /// are evaluating.
    fn cursor_name(&self) -> &str {
        self.cursor
            .path
            .strip_prefix(&self.dir)
            .expect("task directory is a parent of cursor")
            .as_str()
    }
}

impl std::fmt::Debug for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Test")
            .field("db", &"<ProjectDatabase>")
            .field("dir", &self.dir)
            .field("name", &self.name)
            .field("cursor", &self.cursor)
            .field("settings", &self.settings)
            .finish()
    }
}

/// Truth data for a single completion evaluation test.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CompletionTruth {
    #[serde(default)]
    settings: CompletionSettings,
}

/// Settings to forward to our completion routine.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CompletionSettings {
    #[serde(default)]
    auto_import: bool,
}

impl From<&CompletionSettings> for ty_ide::CompletionSettings {
    fn from(x: &CompletionSettings) -> ty_ide::CompletionSettings {
        ty_ide::CompletionSettings {
            auto_import: x.auto_import,
        }
    }
}

/// The "source" of a task, as found in ty's git repository.
#[derive(Debug)]
struct TaskSource {
    /// The directory containing this task.
    dir: SystemPathBuf,
    /// The name of this task (the basename of `dir`).
    name: String,
    /// The "truth" data for this task along with any
    /// settings. This is pulled from `{dir}/completion.toml`.
    truth: CompletionTruth,
}

impl TaskSource {
    fn all(src_dir: &SystemPath) -> anyhow::Result<Vec<TaskSource>> {
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
            sources.push(TaskSource::new(dir)?);
        }
        // Sort our sources so that we always run in the same order.
        // And also so that the CSV output is deterministic across
        // all platforms.
        sources.sort_by(|source1, source2| source1.name.cmp(&source2.name));
        Ok(sources)
    }

    fn new(dir: &SystemPath) -> anyhow::Result<TaskSource> {
        let name = dir.file_name().ok_or_else(|| {
            anyhow::anyhow!("truth source directory `{dir}` does not contain a base name")
        })?;

        let truth_path = dir.join("completion.toml");
        let truth_data = std::fs::read(truth_path.as_std_path())
            .with_context(|| format!("failed to read truth data at `{truth_path}`"))?;
        let truth = toml::from_slice(&truth_data).with_context(|| {
            format!("failed to parse TOML completion truth data from `{truth_path}`")
        })?;

        Ok(TaskSource {
            dir: dir.to_path_buf(),
            name: name.to_string(),
            truth,
        })
    }

    /// Convert this "source" task (from the Ruff repository) into
    /// one or more evaluation tasks within a single Python project.
    /// Exactly one task is created for each cursor directive found in
    /// this source task.
    ///
    /// This includes running `uv sync` to set up a full virtual
    /// environment.
    fn to_tasks(&self, parent_dst_dir: &SystemPath) -> anyhow::Result<Vec<Task>> {
        let dir = parent_dst_dir.join(&self.name);
        let cursors = copy_project(&self.dir, &dir)?;
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
        cursors
            .into_iter()
            .map(|cursor| Task::new(&dir, &self.truth, cursor))
            .collect()
    }
}

/// A single cursor directive within a single Python project.
///
/// Each cursor directive looks like:
/// `<CURSOR [expected-module.]expected-symbol>`.
///
/// That is, each cursor directive corresponds to a single completion
/// request, and each request is a single evaluation task.
#[derive(Clone, Debug)]
struct Cursor {
    /// The path to the file containing this directive.
    path: SystemPathBuf,
    /// The index (starting at 0) of this cursor directive
    /// within `path`.
    index: usize,
    /// The byte offset at which this cursor was located
    /// within `path`.
    offset: usize,
    /// The expected symbol (and optionally module) for this
    /// completion request.
    answer: CompletionAnswer,
}

/// The answer for a single completion request.
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CompletionAnswer {
    symbol: String,
    module: Option<String>,
}

impl CompletionAnswer {
    /// Returns true when this answer matches the completion given.
    fn matches(&self, completion: &Completion) -> bool {
        if let Some(ref qualified) = completion.qualified {
            if qualified.as_str() == self.qualified() {
                return true;
            }
        }
        self.symbol == completion.name.as_str()
            && self.module.as_deref() == completion.module_name.map(ModuleName::as_str)
    }

    fn qualified(&self) -> String {
        self.module
            .as_ref()
            .map(|module| format!("{module}.{}", self.symbol))
            .unwrap_or_else(|| self.symbol.clone())
    }
}

/// Copy the Python project from `src_dir` to `dst_dir`.
///
/// This also looks for occurrences of cursor directives among the
/// project files and returns them. The original cursor directives are
/// deleted.
///
/// Hidden files or directories are skipped.
///
/// # Errors
///
/// Any underlying I/O errors are bubbled up. Also, if no cursor
/// directives are found, then an error is returned. This guarantees
/// that the `Vec<Cursor>` is always non-empty.
fn copy_project(src_dir: &SystemPath, dst_dir: &SystemPath) -> anyhow::Result<Vec<Cursor>> {
    std::fs::create_dir_all(dst_dir).with_context(|| dst_dir.to_string())?;

    let mut cursors = vec![];
    for result in walkdir::WalkDir::new(src_dir.as_std_path()) {
        let dent =
            result.with_context(|| format!("failed to get directory entry from {src_dir}"))?;
        if dent
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with('.'))
        {
            continue;
        }

        let src = SystemPath::from_std_path(dent.path()).ok_or_else(|| {
            anyhow::anyhow!("path `{}` is not valid UTF-8", dent.path().display())
        })?;
        let name = src
            .strip_prefix(src_dir)
            .expect("descendent of `src_dir` must start with `src`");
        // let name = src
        // .file_name()
        // .ok_or_else(|| anyhow::anyhow!("path `{src}` is missing a basename"))?;
        let dst = dst_dir.join(name);
        if dent.file_type().is_dir() {
            std::fs::create_dir_all(dst.as_std_path())
                .with_context(|| format!("failed to create directory `{dst}`"))?;
        } else {
            cursors.extend(copy_file(src, &dst)?);
        }
    }
    anyhow::ensure!(
        !cursors.is_empty(),
        "could not find any `<CURSOR>` directives in any of the files in `{src_dir}`",
    );
    Ok(cursors)
}

/// Copies `src` to `dst` while looking for cursor directives.
///
/// Each cursor directive looks like:
/// `<CURSOR [expected-module.]expected-symbol>`.
///
/// When occurrences of cursor directives are found, then they are
/// replaced with the empty string. The position of each occurrence is
/// recorded, which points to the correct place in a document where all
/// cursor directives are omitted.
///
/// # Errors
///
/// When an underlying I/O error occurs.
fn copy_file(src: &SystemPath, dst: &SystemPath) -> anyhow::Result<Vec<Cursor>> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        // Our module/symbol identifier regex here is certainly more
        // permissive than necessary, but I think that should be fine
        // for this silly little syntax. ---AG
        Regex::new(r"<CURSOR:\s*(?:(?<module>[\S--.]+)\.)?(?<symbol>[\S--.]+)>").unwrap()
    });

    let src_data =
        std::fs::read(src).with_context(|| format!("failed to read `{src}` for copying"))?;
    let mut cursors = vec![];
    // The new data, without cursor directives.
    let mut new = Vec::with_capacity(src_data.len());
    // An index into `src_data` corresponding to either the start of
    // the data or the end of the previous cursor directive that we
    // found.
    let mut prev_match_end = 0;
    // The total bytes removed so far by replacing cursor directives
    // with empty strings.
    let mut bytes_removed = 0;
    for (index, caps) in RE.captures_iter(&src_data).enumerate() {
        let overall = caps.get(0).expect("zeroth group is always available");
        new.extend_from_slice(&src_data[prev_match_end..overall.start()]);
        prev_match_end = overall.end();
        let offset = overall.start() - bytes_removed;
        bytes_removed += overall.len();

        let symbol = str::from_utf8(&caps["symbol"])
            .context("expected symbol in cursor directive in `{src}` is not valid UTF-8")?
            .to_string();
        let module = caps
            .name("module")
            .map(|module| {
                str::from_utf8(module.as_bytes())
                    .context("expected module in cursor directive in `{src}` is not valid UTF-8")
            })
            .transpose()?
            .map(ToString::to_string);
        let answer = CompletionAnswer { symbol, module };
        cursors.push(Cursor {
            path: dst.to_path_buf(),
            index,
            offset,
            answer,
        });
    }
    new.extend_from_slice(&src_data[prev_match_end..]);
    std::fs::write(dst, &new)
        .with_context(|| format!("failed to write contents of `{src}` to `{dst}`"))?;
    Ok(cursors)
}
