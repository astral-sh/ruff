use std::collections::HashSet;
use std::io::Write;
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow};
use ruff_db::files::{File, FileError, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::{
    OsSystem, System, SystemPath, SystemPathBuf, UserConfigDirectoryOverrideGuard, file_time_now,
};
use ruff_db::{Db as _, Upcast};
use ruff_python_ast::PythonVersion;
use ty_project::metadata::options::{EnvironmentOptions, Options};
use ty_project::metadata::pyproject::{PyProject, Tool};
use ty_project::metadata::value::{RangedValue, RelativePathBuf};
use ty_project::watch::{ChangeEvent, ProjectWatcher, directory_watcher};
use ty_project::{Db, ProjectDatabase, ProjectMetadata};
use ty_python_semantic::{ModuleName, PythonPlatform, resolve_module};

struct TestCase {
    db: ProjectDatabase,
    watcher: Option<ProjectWatcher>,
    changes_receiver: crossbeam::channel::Receiver<Vec<ChangeEvent>>,
    /// The temporary directory that contains the test files.
    /// We need to hold on to it in the test case or the temp files get deleted.
    _temp_dir: tempfile::TempDir,
    root_dir: SystemPathBuf,
}

impl TestCase {
    fn project_path(&self, relative: impl AsRef<SystemPath>) -> SystemPathBuf {
        SystemPath::absolute(relative, self.db.project().root(&self.db))
    }

    fn root_path(&self) -> &SystemPath {
        &self.root_dir
    }

    fn db(&self) -> &ProjectDatabase {
        &self.db
    }

    #[track_caller]
    fn stop_watch<M>(&mut self, matcher: M) -> Vec<ChangeEvent>
    where
        M: MatchEvent,
    {
        // track_caller is unstable for lambdas -> That's why this is a fn
        #[track_caller]
        fn panic_with_formatted_events(events: Vec<ChangeEvent>) -> Vec<ChangeEvent> {
            panic!(
                "Didn't observe the expected event. The following events occurred:\n{}",
                events
                    .into_iter()
                    .map(|event| format!("  - {event:?}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }

        self.try_stop_watch(matcher, Duration::from_secs(10))
            .unwrap_or_else(panic_with_formatted_events)
    }

    fn try_stop_watch<M>(
        &mut self,
        mut matcher: M,
        timeout: Duration,
    ) -> Result<Vec<ChangeEvent>, Vec<ChangeEvent>>
    where
        M: MatchEvent,
    {
        tracing::debug!("Try stopping watch with timeout {:?}", timeout);

        let watcher = self
            .watcher
            .take()
            .expect("Cannot call `stop_watch` more than once");

        let start = Instant::now();
        let mut all_events = Vec::new();

        loop {
            let events = self
                .changes_receiver
                .recv_timeout(Duration::from_millis(100))
                .unwrap_or_default();

            if events
                .iter()
                .any(|event| matcher.match_event(event) || event.is_rescan())
            {
                all_events.extend(events);
                break;
            }

            all_events.extend(events);

            if start.elapsed() > timeout {
                return Err(all_events);
            }
        }

        watcher.flush();
        tracing::debug!("Flushed file watcher");
        watcher.stop();
        tracing::debug!("Stopping file watcher");

        // Consume remaining events
        for event in &self.changes_receiver {
            all_events.extend(event);
        }

        Ok(all_events)
    }

    fn take_watch_changes<M: MatchEvent>(&self, matcher: M) -> Vec<ChangeEvent> {
        self.try_take_watch_changes(matcher, Duration::from_secs(10))
            .expect("Expected watch changes but observed none")
    }

    fn try_take_watch_changes<M: MatchEvent>(
        &self,
        mut matcher: M,
        timeout: Duration,
    ) -> Result<Vec<ChangeEvent>, Vec<ChangeEvent>> {
        let watcher = self
            .watcher
            .as_ref()
            .expect("Cannot call `try_take_watch_changes` after `stop_watch`");

        let start = Instant::now();
        let mut all_events = Vec::new();

        loop {
            let events = self
                .changes_receiver
                .recv_timeout(Duration::from_millis(100))
                .unwrap_or_default();

            if events
                .iter()
                .any(|event| matcher.match_event(event) || event.is_rescan())
            {
                all_events.extend(events);
                break;
            }

            all_events.extend(events);

            if start.elapsed() > timeout {
                return Err(all_events);
            }
        }

        while let Ok(event) = self
            .changes_receiver
            .recv_timeout(Duration::from_millis(10))
        {
            all_events.extend(event);
            watcher.flush();
        }

        Ok(all_events)
    }

    fn apply_changes(&mut self, changes: Vec<ChangeEvent>) {
        self.db.apply_changes(changes, None);
    }

    fn update_options(&mut self, options: Options) -> anyhow::Result<()> {
        std::fs::write(
            self.project_path("pyproject.toml").as_std_path(),
            toml::to_string(&PyProject {
                project: None,
                tool: Some(Tool { ty: Some(options) }),
            })
            .context("Failed to serialize options")?,
        )
        .context("Failed to write configuration")?;

        let changes = self.take_watch_changes(event_for_file("pyproject.toml"));
        self.apply_changes(changes);

        if let Some(watcher) = &mut self.watcher {
            watcher.update(&self.db);
            assert!(!watcher.has_errored_paths());
        }

        Ok(())
    }

    #[track_caller]
    fn assert_indexed_project_files(&self, expected: impl IntoIterator<Item = File>) {
        let mut expected: HashSet<_> = expected.into_iter().collect();

        let actual = self.db().project().files(self.db());
        for file in &actual {
            assert!(
                expected.remove(&file),
                "Indexed project files contains '{}' which was not expected.",
                file.path(self.db())
            );
        }

        if !expected.is_empty() {
            let paths: Vec<_> = expected
                .iter()
                .map(|file| file.path(self.db()).as_str())
                .collect();
            panic!(
                "Indexed project files are missing the following files: {:?}",
                paths.join(", ")
            );
        }
    }

    fn system_file(&self, path: impl AsRef<SystemPath>) -> Result<File, FileError> {
        system_path_to_file(self.db(), path.as_ref())
    }
}

trait MatchEvent {
    fn match_event(&mut self, event: &ChangeEvent) -> bool;
}

fn event_for_file(name: &str) -> impl MatchEvent + '_ {
    |event: &ChangeEvent| event.file_name() == Some(name)
}

impl<F> MatchEvent for F
where
    F: FnMut(&ChangeEvent) -> bool,
{
    fn match_event(&mut self, event: &ChangeEvent) -> bool {
        (*self)(event)
    }
}

trait Setup {
    fn setup(self, context: &mut SetupContext) -> anyhow::Result<()>;
}

struct SetupContext<'a> {
    system: &'a OsSystem,
    root_path: &'a SystemPath,
    options: Option<Options>,
    included_paths: Option<Vec<SystemPathBuf>>,
}

impl<'a> SetupContext<'a> {
    fn system(&self) -> &'a OsSystem {
        self.system
    }

    fn join_project_path(&self, relative: impl AsRef<SystemPath>) -> SystemPathBuf {
        self.project_path().join(relative)
    }

    fn project_path(&self) -> &SystemPath {
        self.system.current_directory()
    }

    fn root_path(&self) -> &'a SystemPath {
        self.root_path
    }

    fn join_root_path(&self, relative: impl AsRef<SystemPath>) -> SystemPathBuf {
        self.root_path().join(relative)
    }

    fn write_project_file(
        &self,
        relative_path: impl AsRef<SystemPath>,
        content: &str,
    ) -> anyhow::Result<()> {
        let relative_path = relative_path.as_ref();
        let absolute_path = self.join_project_path(relative_path);
        Self::write_file_impl(absolute_path, content)
    }

    fn write_file(
        &self,
        relative_path: impl AsRef<SystemPath>,
        content: &str,
    ) -> anyhow::Result<()> {
        let relative_path = relative_path.as_ref();
        let absolute_path = self.join_root_path(relative_path);
        Self::write_file_impl(absolute_path, content)
    }

    fn write_file_impl(path: impl AsRef<SystemPath>, content: &str) -> anyhow::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create parent directory for file `{path}`"))?;
        }

        let mut file = std::fs::File::create(path.as_std_path())
            .with_context(|| format!("Failed to open file `{path}`"))?;
        file.write_all(content.as_bytes())
            .with_context(|| format!("Failed to write to file `{path}`"))?;
        file.sync_data()?;

        Ok(())
    }

    fn set_options(&mut self, options: Options) {
        self.options = Some(options);
    }

    fn set_included_paths(&mut self, paths: Vec<SystemPathBuf>) {
        self.included_paths = Some(paths);
    }
}

impl<const N: usize, P> Setup for [(P, &'static str); N]
where
    P: AsRef<SystemPath>,
{
    fn setup(self, context: &mut SetupContext) -> anyhow::Result<()> {
        for (relative_path, content) in self {
            context.write_project_file(relative_path, content)?;
        }

        Ok(())
    }
}

impl<F> Setup for F
where
    F: FnOnce(&mut SetupContext) -> anyhow::Result<()>,
{
    fn setup(self, context: &mut SetupContext) -> anyhow::Result<()> {
        self(context)
    }
}

fn setup<F>(setup_files: F) -> anyhow::Result<TestCase>
where
    F: Setup,
{
    let temp_dir = tempfile::tempdir()?;

    let root_path = SystemPath::from_std_path(temp_dir.path()).ok_or_else(|| {
        anyhow!(
            "Temporary directory `{}` is not a valid UTF-8 path.",
            temp_dir.path().display()
        )
    })?;

    let root_path = SystemPathBuf::from_utf8_path_buf(
        root_path
            .as_utf8_path()
            .canonicalize_utf8()
            .with_context(|| "Failed to canonicalize root path.")?,
    )
    .simplified()
    .to_path_buf();

    let project_path = root_path.join("project");

    std::fs::create_dir_all(project_path.as_std_path())
        .with_context(|| format!("Failed to create project directory `{project_path}`"))?;

    let system = OsSystem::new(&project_path);
    let mut setup_context = SetupContext {
        system: &system,
        root_path: &root_path,
        options: None,
        included_paths: None,
    };

    setup_files
        .setup(&mut setup_context)
        .context("Failed to setup test files")?;

    if let Some(options) = setup_context.options {
        std::fs::write(
            project_path.join("pyproject.toml").as_std_path(),
            toml::to_string(&PyProject {
                project: None,
                tool: Some(Tool { ty: Some(options) }),
            })
            .context("Failed to serialize options")?,
        )
        .context("Failed to write configuration")?;
    }

    let included_paths = setup_context.included_paths;

    let mut project = ProjectMetadata::discover(&project_path, &system)?;
    project.apply_configuration_files(&system)?;

    let program_settings = project.to_program_settings(&system);

    for path in program_settings
        .search_paths
        .extra_paths
        .iter()
        .chain(program_settings.search_paths.custom_typeshed.as_ref())
    {
        std::fs::create_dir_all(path.as_std_path())
            .with_context(|| format!("Failed to create search path `{path}`"))?;
    }

    let mut db = ProjectDatabase::new(project, system)?;

    if let Some(included_paths) = included_paths {
        db.project().set_included_paths(&mut db, included_paths);
    }

    let (sender, receiver) = crossbeam::channel::unbounded();
    let watcher = directory_watcher(move |events| sender.send(events).unwrap())
        .with_context(|| "Failed to create directory watcher")?;

    let watcher = ProjectWatcher::new(watcher, &db);
    assert!(!watcher.has_errored_paths());

    let test_case = TestCase {
        db,
        changes_receiver: receiver,
        watcher: Some(watcher),
        _temp_dir: temp_dir,
        root_dir: root_path,
    };

    // Sometimes the file watcher reports changes for events that happened before the watcher was started.
    // Do a best effort at dropping these events.
    let _ =
        test_case.try_take_watch_changes(|_event: &ChangeEvent| true, Duration::from_millis(100));

    Ok(test_case)
}

/// Updates the content of a file and ensures that the last modified file time is updated.
fn update_file(path: impl AsRef<SystemPath>, content: &str) -> anyhow::Result<()> {
    let path = path.as_ref().as_std_path();

    let metadata = path.metadata()?;
    let last_modified_time = filetime::FileTime::from_last_modification_time(&metadata);

    let mut file = std::fs::OpenOptions::new()
        .create(false)
        .write(true)
        .truncate(true)
        .open(path)?;
    file.write_all(content.as_bytes())?;

    loop {
        file.sync_all()?;

        let modified_time = filetime::FileTime::from_last_modification_time(&path.metadata()?);

        if modified_time != last_modified_time {
            break Ok(());
        }

        std::thread::sleep(Duration::from_nanos(10));

        filetime::set_file_handle_times(&file, None, Some(file_time_now()))?;
    }
}

#[test]
fn new_file() -> anyhow::Result<()> {
    let mut case = setup([("bar.py", "")])?;
    let bar_path = case.project_path("bar.py");
    let bar_file = case.system_file(&bar_path).unwrap();
    let foo_path = case.project_path("foo.py");

    assert_eq!(case.system_file(&foo_path), Err(FileError::NotFound));
    case.assert_indexed_project_files([bar_file]);

    std::fs::write(foo_path.as_std_path(), "print('Hello')")?;

    let changes = case.stop_watch(event_for_file("foo.py"));

    case.apply_changes(changes);

    let foo = case.system_file(&foo_path).expect("foo.py to exist.");

    case.assert_indexed_project_files([bar_file, foo]);

    Ok(())
}

#[test]
fn new_ignored_file() -> anyhow::Result<()> {
    let mut case = setup([("bar.py", ""), (".ignore", "foo.py")])?;
    let bar_path = case.project_path("bar.py");
    let bar_file = case.system_file(&bar_path).unwrap();
    let foo_path = case.project_path("foo.py");

    assert_eq!(case.system_file(&foo_path), Err(FileError::NotFound));
    case.assert_indexed_project_files([bar_file]);

    std::fs::write(foo_path.as_std_path(), "print('Hello')")?;

    let changes = case.stop_watch(event_for_file("foo.py"));

    case.apply_changes(changes);

    assert!(case.system_file(&foo_path).is_ok());
    case.assert_indexed_project_files([bar_file]);

    Ok(())
}

#[test]
fn new_non_project_file() -> anyhow::Result<()> {
    let mut case = setup(|context: &mut SetupContext| {
        context.write_project_file("bar.py", "")?;
        context.set_options(Options {
            environment: Some(EnvironmentOptions {
                extra_paths: Some(vec![RelativePathBuf::cli(
                    context.join_root_path("site_packages"),
                )]),
                ..EnvironmentOptions::default()
            }),
            ..Options::default()
        });

        Ok(())
    })?;

    let bar_path = case.project_path("bar.py");
    let bar_file = case.system_file(&bar_path).unwrap();

    case.assert_indexed_project_files([bar_file]);

    // Add a file to site packages
    let black_path = case.root_path().join("site_packages/black.py");
    std::fs::write(black_path.as_std_path(), "print('Hello')")?;

    let changes = case.stop_watch(event_for_file("black.py"));

    case.apply_changes(changes);

    assert!(case.system_file(&black_path).is_ok());

    // The file should not have been added to the project files
    case.assert_indexed_project_files([bar_file]);

    Ok(())
}

#[test]
fn new_files_with_explicit_included_paths() -> anyhow::Result<()> {
    let mut case = setup(|context: &mut SetupContext| {
        context.write_project_file("src/main.py", "")?;
        context.write_project_file("src/sub/__init__.py", "")?;
        context.write_project_file("src/test.py", "")?;
        context.set_included_paths(vec![
            context.join_project_path("src/main.py"),
            context.join_project_path("src/sub"),
        ]);
        Ok(())
    })?;

    let main_path = case.project_path("src/main.py");
    let main_file = case.system_file(&main_path).unwrap();

    let sub_init_path = case.project_path("src/sub/__init__.py");
    let sub_init = case.system_file(&sub_init_path).unwrap();

    case.assert_indexed_project_files([main_file, sub_init]);

    // Write a new file to `sub` which is an included path
    let sub_a_path = case.project_path("src/sub/a.py");
    std::fs::write(sub_a_path.as_std_path(), "print('Hello')")?;

    // and write a second file in the root directory -- this should not be included
    let test2_path = case.project_path("src/test2.py");
    std::fs::write(test2_path.as_std_path(), "print('Hello')")?;

    let changes = case.stop_watch(event_for_file("test2.py"));

    case.apply_changes(changes);

    let sub_a_file = case.system_file(&sub_a_path).expect("sub/a.py to exist");

    case.assert_indexed_project_files([main_file, sub_init, sub_a_file]);

    Ok(())
}

#[test]
fn new_file_in_included_out_of_project_directory() -> anyhow::Result<()> {
    let mut case = setup(|context: &mut SetupContext| {
        context.write_project_file("src/main.py", "")?;
        context.write_project_file("script.py", "")?;
        context.write_file("outside_project/a.py", "")?;

        context.set_included_paths(vec![
            context.join_root_path("outside_project"),
            context.join_project_path("src"),
        ]);
        Ok(())
    })?;

    let main_path = case.project_path("src/main.py");
    let main_file = case.system_file(&main_path).unwrap();

    let outside_a_path = case.root_path().join("outside_project/a.py");
    let outside_a = case.system_file(&outside_a_path).unwrap();

    case.assert_indexed_project_files([outside_a, main_file]);

    // Write a new file to `src` which should be watched
    let src_a = case.project_path("src/a.py");
    std::fs::write(src_a.as_std_path(), "print('Hello')")?;

    // and write a second file to `outside_project` which should be watched too
    let outside_b_path = case.root_path().join("outside_project/b.py");
    std::fs::write(outside_b_path.as_std_path(), "print('Hello')")?;

    // and a third file in the project's root that should not be included
    let script2_path = case.project_path("script2.py");
    std::fs::write(script2_path.as_std_path(), "print('Hello')")?;

    let changes = case.stop_watch(event_for_file("script2.py"));

    case.apply_changes(changes);

    let src_a_file = case.system_file(&src_a).unwrap();
    let outside_b_file = case.system_file(&outside_b_path).unwrap();

    // The file should not have been added to the project files
    case.assert_indexed_project_files([main_file, outside_a, outside_b_file, src_a_file]);

    Ok(())
}

#[test]
fn changed_file() -> anyhow::Result<()> {
    let foo_source = "print('Hello, world!')";
    let mut case = setup([("foo.py", foo_source)])?;
    let foo_path = case.project_path("foo.py");

    let foo = case.system_file(&foo_path)?;
    assert_eq!(source_text(case.db(), foo).as_str(), foo_source);
    case.assert_indexed_project_files([foo]);

    update_file(&foo_path, "print('Version 2')")?;

    let changes = case.stop_watch(event_for_file("foo.py"));

    assert!(!changes.is_empty());

    case.apply_changes(changes);

    assert_eq!(source_text(case.db(), foo).as_str(), "print('Version 2')");
    case.assert_indexed_project_files([foo]);

    Ok(())
}

#[test]
fn deleted_file() -> anyhow::Result<()> {
    let foo_source = "print('Hello, world!')";
    let mut case = setup([("foo.py", foo_source)])?;
    let foo_path = case.project_path("foo.py");

    let foo = case.system_file(&foo_path)?;

    assert!(foo.exists(case.db()));
    case.assert_indexed_project_files([foo]);

    std::fs::remove_file(foo_path.as_std_path())?;

    let changes = case.stop_watch(event_for_file("foo.py"));

    case.apply_changes(changes);

    assert!(!foo.exists(case.db()));
    case.assert_indexed_project_files([]);

    Ok(())
}

/// Tests the case where a file is moved from inside a watched directory to a directory that is not watched.
///
/// This matches the behavior of deleting a file in VS code.
#[test]
fn move_file_to_trash() -> anyhow::Result<()> {
    let foo_source = "print('Hello, world!')";
    let mut case = setup([("foo.py", foo_source)])?;
    let foo_path = case.project_path("foo.py");

    let trash_path = case.root_path().join(".trash");
    std::fs::create_dir_all(trash_path.as_std_path())?;

    let foo = case.system_file(&foo_path)?;

    assert!(foo.exists(case.db()));
    case.assert_indexed_project_files([foo]);

    std::fs::rename(
        foo_path.as_std_path(),
        trash_path.join("foo.py").as_std_path(),
    )?;

    let changes = case.stop_watch(event_for_file("foo.py"));

    case.apply_changes(changes);

    assert!(!foo.exists(case.db()));
    case.assert_indexed_project_files([]);

    Ok(())
}

/// Move a file from a non-project (non-watched) location into the project.
#[test]
fn move_file_to_project() -> anyhow::Result<()> {
    let mut case = setup([("bar.py", "")])?;
    let bar_path = case.project_path("bar.py");
    let bar = case.system_file(&bar_path).unwrap();

    let foo_path = case.root_path().join("foo.py");
    std::fs::write(foo_path.as_std_path(), "")?;

    let foo_in_project = case.project_path("foo.py");

    assert!(case.system_file(&foo_path).is_ok());
    case.assert_indexed_project_files([bar]);

    std::fs::rename(foo_path.as_std_path(), foo_in_project.as_std_path())?;

    let changes = case.stop_watch(event_for_file("foo.py"));

    case.apply_changes(changes);

    let foo_in_project = case.system_file(&foo_in_project)?;

    assert!(foo_in_project.exists(case.db()));
    case.assert_indexed_project_files([bar, foo_in_project]);

    Ok(())
}

/// Rename a project file.
#[test]
fn rename_file() -> anyhow::Result<()> {
    let mut case = setup([("foo.py", "")])?;
    let foo_path = case.project_path("foo.py");
    let bar_path = case.project_path("bar.py");

    let foo = case.system_file(&foo_path)?;

    case.assert_indexed_project_files([foo]);

    std::fs::rename(foo_path.as_std_path(), bar_path.as_std_path())?;

    let changes = case.stop_watch(event_for_file("bar.py"));

    case.apply_changes(changes);

    assert!(!foo.exists(case.db()));

    let bar = case.system_file(&bar_path)?;

    assert!(bar.exists(case.db()));
    case.assert_indexed_project_files([bar]);

    Ok(())
}

#[test]
fn directory_moved_to_project() -> anyhow::Result<()> {
    let mut case = setup([("bar.py", "import sub.a")])?;
    let bar = case.system_file(case.project_path("bar.py")).unwrap();

    let sub_original_path = case.root_path().join("sub");
    let init_original_path = sub_original_path.join("__init__.py");
    let a_original_path = sub_original_path.join("a.py");

    std::fs::create_dir(sub_original_path.as_std_path())
        .with_context(|| "Failed to create sub directory")?;
    std::fs::write(init_original_path.as_std_path(), "")
        .with_context(|| "Failed to create __init__.py")?;
    std::fs::write(a_original_path.as_std_path(), "").with_context(|| "Failed to create a.py")?;

    let sub_a_module = resolve_module(
        case.db().upcast(),
        &ModuleName::new_static("sub.a").unwrap(),
    );

    assert_eq!(sub_a_module, None);
    case.assert_indexed_project_files([bar]);

    let sub_new_path = case.project_path("sub");
    std::fs::rename(sub_original_path.as_std_path(), sub_new_path.as_std_path())
        .with_context(|| "Failed to move sub directory")?;

    let changes = case.stop_watch(event_for_file("sub"));

    case.apply_changes(changes);

    let init_file = case
        .system_file(sub_new_path.join("__init__.py"))
        .expect("__init__.py to exist");
    let a_file = case
        .system_file(sub_new_path.join("a.py"))
        .expect("a.py to exist");

    // `import sub.a` should now resolve
    assert!(
        resolve_module(
            case.db().upcast(),
            &ModuleName::new_static("sub.a").unwrap()
        )
        .is_some()
    );

    case.assert_indexed_project_files([bar, init_file, a_file]);

    Ok(())
}

#[test]
fn directory_moved_to_trash() -> anyhow::Result<()> {
    let mut case = setup([
        ("bar.py", "import sub.a"),
        ("sub/__init__.py", ""),
        ("sub/a.py", ""),
    ])?;
    let bar = case.system_file(case.project_path("bar.py")).unwrap();

    assert!(
        resolve_module(
            case.db().upcast(),
            &ModuleName::new_static("sub.a").unwrap()
        )
        .is_some()
    );

    let sub_path = case.project_path("sub");
    let init_file = case
        .system_file(sub_path.join("__init__.py"))
        .expect("__init__.py to exist");
    let a_file = case
        .system_file(sub_path.join("a.py"))
        .expect("a.py to exist");

    case.assert_indexed_project_files([bar, init_file, a_file]);

    std::fs::create_dir(case.root_path().join(".trash").as_std_path())?;
    let trashed_sub = case.root_path().join(".trash/sub");
    std::fs::rename(sub_path.as_std_path(), trashed_sub.as_std_path())
        .with_context(|| "Failed to move the sub directory to the trash")?;

    let changes = case.stop_watch(event_for_file("sub"));

    case.apply_changes(changes);

    // `import sub.a` should no longer resolve
    assert!(
        resolve_module(
            case.db().upcast(),
            &ModuleName::new_static("sub.a").unwrap()
        )
        .is_none()
    );

    assert!(!init_file.exists(case.db()));
    assert!(!a_file.exists(case.db()));

    case.assert_indexed_project_files([bar]);

    Ok(())
}

#[test]
fn directory_renamed() -> anyhow::Result<()> {
    let mut case = setup([
        ("bar.py", "import sub.a"),
        ("sub/__init__.py", ""),
        ("sub/a.py", ""),
    ])?;

    let bar = case.system_file(case.project_path("bar.py")).unwrap();

    assert!(
        resolve_module(
            case.db().upcast(),
            &ModuleName::new_static("sub.a").unwrap()
        )
        .is_some()
    );
    assert!(
        resolve_module(
            case.db().upcast(),
            &ModuleName::new_static("foo.baz").unwrap()
        )
        .is_none()
    );

    let sub_path = case.project_path("sub");
    let sub_init = case
        .system_file(sub_path.join("__init__.py"))
        .expect("__init__.py to exist");
    let sub_a = case
        .system_file(sub_path.join("a.py"))
        .expect("a.py to exist");

    case.assert_indexed_project_files([bar, sub_init, sub_a]);

    let foo_baz = case.project_path("foo/baz");

    std::fs::create_dir(case.project_path("foo").as_std_path())?;
    std::fs::rename(sub_path.as_std_path(), foo_baz.as_std_path())
        .with_context(|| "Failed to move the sub directory")?;

    // Linux and windows only emit an event for the newly created root directory, but not for every new component.
    let changes = case.stop_watch(event_for_file("sub"));

    case.apply_changes(changes);

    // `import sub.a` should no longer resolve
    assert!(
        resolve_module(
            case.db().upcast(),
            &ModuleName::new_static("sub.a").unwrap()
        )
        .is_none()
    );
    // `import foo.baz` should now resolve
    assert!(
        resolve_module(
            case.db().upcast(),
            &ModuleName::new_static("foo.baz").unwrap()
        )
        .is_some()
    );

    // The old paths are no longer tracked
    assert!(!sub_init.exists(case.db()));
    assert!(!sub_a.exists(case.db()));

    let foo_baz_init = case
        .system_file(foo_baz.join("__init__.py"))
        .expect("__init__.py to exist");
    let foo_baz_a = case
        .system_file(foo_baz.join("a.py"))
        .expect("a.py to exist");

    // The new paths are synced

    assert!(foo_baz_init.exists(case.db()));
    assert!(foo_baz_a.exists(case.db()));

    case.assert_indexed_project_files([bar, foo_baz_init, foo_baz_a]);

    Ok(())
}

#[test]
fn directory_deleted() -> anyhow::Result<()> {
    let mut case = setup([
        ("bar.py", "import sub.a"),
        ("sub/__init__.py", ""),
        ("sub/a.py", ""),
    ])?;

    let bar = case.system_file(case.project_path("bar.py")).unwrap();

    assert!(
        resolve_module(
            case.db().upcast(),
            &ModuleName::new_static("sub.a").unwrap()
        )
        .is_some()
    );

    let sub_path = case.project_path("sub");

    let init_file = case
        .system_file(sub_path.join("__init__.py"))
        .expect("__init__.py to exist");
    let a_file = case
        .system_file(sub_path.join("a.py"))
        .expect("a.py to exist");
    case.assert_indexed_project_files([bar, init_file, a_file]);

    std::fs::remove_dir_all(sub_path.as_std_path())
        .with_context(|| "Failed to remove the sub directory")?;

    let changes = case.stop_watch(event_for_file("sub"));

    case.apply_changes(changes);

    // `import sub.a` should no longer resolve
    assert!(
        resolve_module(
            case.db().upcast(),
            &ModuleName::new_static("sub.a").unwrap()
        )
        .is_none()
    );

    assert!(!init_file.exists(case.db()));
    assert!(!a_file.exists(case.db()));
    case.assert_indexed_project_files([bar]);

    Ok(())
}

#[test]
fn search_path() -> anyhow::Result<()> {
    let mut case = setup(|context: &mut SetupContext| {
        context.write_project_file("bar.py", "import sub.a")?;

        context.set_options(Options {
            environment: Some(EnvironmentOptions {
                extra_paths: Some(vec![RelativePathBuf::cli(
                    context.join_root_path("site_packages"),
                )]),
                ..EnvironmentOptions::default()
            }),
            ..Options::default()
        });
        Ok(())
    })?;

    let site_packages = case.root_path().join("site_packages");

    assert_eq!(
        resolve_module(case.db(), &ModuleName::new("a").unwrap()),
        None
    );

    std::fs::write(site_packages.join("a.py").as_std_path(), "class A: ...")?;

    let changes = case.stop_watch(event_for_file("a.py"));

    case.apply_changes(changes);

    assert!(resolve_module(case.db().upcast(), &ModuleName::new_static("a").unwrap()).is_some());
    case.assert_indexed_project_files([case.system_file(case.project_path("bar.py")).unwrap()]);

    Ok(())
}

#[test]
fn add_search_path() -> anyhow::Result<()> {
    let mut case = setup([("bar.py", "import sub.a")])?;

    let site_packages = case.project_path("site_packages");
    std::fs::create_dir_all(site_packages.as_std_path())?;

    assert!(resolve_module(case.db().upcast(), &ModuleName::new_static("a").unwrap()).is_none());

    // Register site-packages as a search path.
    case.update_options(Options {
        environment: Some(EnvironmentOptions {
            extra_paths: Some(vec![RelativePathBuf::cli("site_packages")]),
            ..EnvironmentOptions::default()
        }),
        ..Options::default()
    })
    .expect("Search path settings to be valid");

    std::fs::write(site_packages.join("a.py").as_std_path(), "class A: ...")?;

    let changes = case.stop_watch(event_for_file("a.py"));

    case.apply_changes(changes);

    assert!(resolve_module(case.db().upcast(), &ModuleName::new_static("a").unwrap()).is_some());

    Ok(())
}

#[test]
fn remove_search_path() -> anyhow::Result<()> {
    let mut case = setup(|context: &mut SetupContext| {
        context.write_project_file("bar.py", "import sub.a")?;
        context.set_options(Options {
            environment: Some(EnvironmentOptions {
                extra_paths: Some(vec![RelativePathBuf::cli(
                    context.join_root_path("site_packages"),
                )]),
                ..EnvironmentOptions::default()
            }),
            ..Options::default()
        });

        Ok(())
    })?;

    // Remove site packages from the search path settings.
    let site_packages = case.root_path().join("site_packages");

    case.update_options(Options {
        environment: None,
        ..Options::default()
    })
    .expect("Search path settings to be valid");

    std::fs::write(site_packages.join("a.py").as_std_path(), "class A: ...")?;

    let changes = case.try_stop_watch(|_: &ChangeEvent| true, Duration::from_millis(100));

    assert_eq!(changes, Err(vec![]));

    Ok(())
}

#[test]
fn change_python_version_and_platform() -> anyhow::Result<()> {
    let mut case = setup(|context: &mut SetupContext| {
        // `sys.last_exc` is a Python 3.12 only feature
        // `os.getegid()` is Unix only
        context.write_project_file(
            "bar.py",
            r#"
import sys
import os
print(sys.last_exc, os.getegid())
"#,
        )?;
        context.set_options(Options {
            environment: Some(EnvironmentOptions {
                python_version: Some(RangedValue::cli(PythonVersion::PY311)),
                python_platform: Some(RangedValue::cli(PythonPlatform::Identifier(
                    "win32".to_string(),
                ))),
                ..EnvironmentOptions::default()
            }),
            ..Options::default()
        });

        Ok(())
    })?;

    let diagnostics = case.db.check().context("Failed to check project.")?;

    assert_eq!(diagnostics.len(), 2);
    assert_eq!(
        diagnostics[0].primary_message(),
        "Type `<module 'sys'>` has no attribute `last_exc`"
    );
    assert_eq!(
        diagnostics[1].primary_message(),
        "Type `<module 'os'>` has no attribute `getegid`"
    );

    // Change the python version
    case.update_options(Options {
        environment: Some(EnvironmentOptions {
            python_version: Some(RangedValue::cli(PythonVersion::PY312)),
            python_platform: Some(RangedValue::cli(PythonPlatform::Identifier(
                "linux".to_string(),
            ))),
            ..EnvironmentOptions::default()
        }),
        ..Options::default()
    })
    .expect("Search path settings to be valid");

    let diagnostics = case.db.check().context("Failed to check project.")?;
    assert!(diagnostics.is_empty());

    Ok(())
}

#[test]
fn changed_versions_file() -> anyhow::Result<()> {
    let mut case = setup(|context: &mut SetupContext| {
        std::fs::write(
            context.join_project_path("bar.py").as_std_path(),
            "import sub.a",
        )?;
        std::fs::create_dir_all(context.join_root_path("typeshed/stdlib").as_std_path())?;
        std::fs::write(
            context
                .join_root_path("typeshed/stdlib/VERSIONS")
                .as_std_path(),
            "",
        )?;
        std::fs::write(
            context
                .join_root_path("typeshed/stdlib/os.pyi")
                .as_std_path(),
            "# not important",
        )?;

        context.set_options(Options {
            environment: Some(EnvironmentOptions {
                typeshed: Some(RelativePathBuf::cli(context.join_root_path("typeshed"))),
                ..EnvironmentOptions::default()
            }),
            ..Options::default()
        });

        Ok(())
    })?;

    // Unset the custom typeshed directory.
    assert_eq!(
        resolve_module(case.db(), &ModuleName::new("os").unwrap()),
        None
    );

    std::fs::write(
        case.root_path()
            .join("typeshed/stdlib/VERSIONS")
            .as_std_path(),
        "os: 3.0-",
    )?;

    let changes = case.stop_watch(event_for_file("VERSIONS"));

    case.apply_changes(changes);

    assert!(resolve_module(case.db(), &ModuleName::new("os").unwrap()).is_some());

    Ok(())
}

/// Watch a project that contains two files where one file is a hardlink to another.
///
/// Setup:
/// ```text
/// - project
///   |- foo.py
///   |- bar.py (hard link to foo.py)
/// ```
///
/// # Linux
/// `inotify` only emits a single change event for the file that was changed.
/// Other files that point to the same inode (hardlinks) won't get updated.
///
/// For reference: VS Code and PyCharm have the same behavior where the results for one of the
/// files are stale.
///
/// # Windows
/// I haven't found any documentation that states the notification behavior on Windows but what
/// we're seeing is that Windows only emits a single event, similar to Linux.
#[test]
fn hard_links_in_project() -> anyhow::Result<()> {
    let mut case = setup(|context: &mut SetupContext| {
        let foo_path = context.join_project_path("foo.py");
        std::fs::write(foo_path.as_std_path(), "print('Version 1')")?;

        // Create a hardlink to `foo`
        let bar_path = context.join_project_path("bar.py");
        std::fs::hard_link(foo_path.as_std_path(), bar_path.as_std_path())
            .context("Failed to create hard link from foo.py -> bar.py")?;

        Ok(())
    })?;

    let foo_path = case.project_path("foo.py");
    let foo = case.system_file(&foo_path).unwrap();
    let bar_path = case.project_path("bar.py");
    let bar = case.system_file(&bar_path).unwrap();

    assert_eq!(source_text(case.db(), foo).as_str(), "print('Version 1')");
    assert_eq!(source_text(case.db(), bar).as_str(), "print('Version 1')");
    case.assert_indexed_project_files([bar, foo]);

    // Write to the hard link target.
    update_file(foo_path, "print('Version 2')").context("Failed to update foo.py")?;

    let changes = case.stop_watch(event_for_file("foo.py"));

    case.apply_changes(changes);

    assert_eq!(source_text(case.db(), foo).as_str(), "print('Version 2')");

    // macOS is the only platform that emits events for every hardlink.
    if cfg!(target_os = "macos") {
        assert_eq!(source_text(case.db(), bar).as_str(), "print('Version 2')");
    }

    Ok(())
}

/// Watch a project that contains one file that is a hardlink to a file outside the project.
///
/// Setup:
/// ```text
/// - foo.py
/// - project
///   |- bar.py (hard link to /foo.py)
/// ```
///
/// # Linux
/// inotiyf doesn't support observing changes to hard linked files.
///
/// > Note: when monitoring a directory, events are not generated for
/// > the files inside the directory when the events are performed via
/// > a pathname (i.e., a link) that lies outside the monitored
/// > directory. [source](https://man7.org/linux/man-pages/man7/inotify.7.html)
///
/// # Windows
/// > Retrieves information that describes the changes within the specified directory.
///
/// [source](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesw)
///
/// My interpretation of this is that Windows doesn't support observing changes made to
/// hard linked files outside the project.
#[test]
#[cfg_attr(
    target_os = "linux",
    ignore = "inotify doesn't support observing changes to hard linked files."
)]
#[cfg_attr(
    target_os = "windows",
    ignore = "windows doesn't support observing changes to hard linked files."
)]
fn hard_links_to_target_outside_project() -> anyhow::Result<()> {
    let mut case = setup(|context: &mut SetupContext| {
        let foo_path = context.join_root_path("foo.py");
        std::fs::write(foo_path.as_std_path(), "print('Version 1')")?;

        // Create a hardlink to `foo`
        let bar_path = context.join_project_path("bar.py");
        std::fs::hard_link(foo_path.as_std_path(), bar_path.as_std_path())
            .context("Failed to create hard link from foo.py -> bar.py")?;

        Ok(())
    })?;

    let foo_path = case.root_path().join("foo.py");
    let foo = case.system_file(&foo_path).unwrap();
    let bar_path = case.project_path("bar.py");
    let bar = case.system_file(&bar_path).unwrap();

    assert_eq!(source_text(case.db(), foo).as_str(), "print('Version 1')");
    assert_eq!(source_text(case.db(), bar).as_str(), "print('Version 1')");

    // Write to the hard link target.
    update_file(foo_path, "print('Version 2')").context("Failed to update foo.py")?;

    let changes = case.stop_watch(ChangeEvent::is_changed);

    case.apply_changes(changes);

    assert_eq!(source_text(case.db(), bar).as_str(), "print('Version 2')");

    Ok(())
}

#[cfg(unix)]
mod unix {
    //! Tests that make use of unix specific file-system features.
    use super::*;

    /// Changes the metadata of the only file in the project.
    #[test]
    fn changed_metadata() -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let mut case = setup([("foo.py", "")])?;
        let foo_path = case.project_path("foo.py");

        let foo = case.system_file(&foo_path)?;
        assert_eq!(
            foo.permissions(case.db()),
            Some(
                std::fs::metadata(foo_path.as_std_path())
                    .unwrap()
                    .permissions()
                    .mode()
            )
        );

        std::fs::set_permissions(
            foo_path.as_std_path(),
            std::fs::Permissions::from_mode(0o777),
        )
        .with_context(|| "Failed to set file permissions.")?;

        let changes = case.stop_watch(event_for_file("foo.py"));

        case.apply_changes(changes);

        assert_eq!(
            foo.permissions(case.db()),
            Some(
                std::fs::metadata(foo_path.as_std_path())
                    .unwrap()
                    .permissions()
                    .mode()
            )
        );

        Ok(())
    }

    /// A project path is a symlink to a file outside the project.
    ///
    /// Setup:
    /// ```text
    /// - bar
    ///   |- baz.py
    ///
    /// - project
    ///   |- bar -> /bar
    /// ```
    ///
    /// # macOS
    /// This test case isn't supported on macOS.
    /// macOS uses `FSEvents` and `FSEvents` doesn't emit an event if a file in a symlinked directory is changed.
    ///
    /// > Generally speaking, when working with file system event notifications, you will probably want to use lstat,
    /// > because changes to the underlying file will not result in a change notification for the directory containing
    /// > the symbolic link to that file. However, if you are working with a controlled file structure in
    /// > which symbolic links always point within your watched tree, you might have reason to use stat.
    ///
    /// [source](https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html#//apple_ref/doc/uid/TP40005289-CH4-SW4)
    ///
    /// Pyright also does not support this case.
    #[test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "FSEvents doesn't emit change events for symlinked directories outside of the watched paths."
    )]
    fn symlink_target_outside_watched_paths() -> anyhow::Result<()> {
        let mut case = setup(|context: &mut SetupContext| {
            // Set up the symlink target.
            let link_target = context.join_root_path("bar");
            std::fs::create_dir_all(link_target.as_std_path())
                .context("Failed to create link target directory")?;
            let baz_original = link_target.join("baz.py");
            std::fs::write(baz_original.as_std_path(), "def baz(): ...")
                .context("Failed to write link target file")?;

            // Create a symlink inside the project
            let bar = context.join_project_path("bar");
            std::os::unix::fs::symlink(link_target.as_std_path(), bar.as_std_path())
                .context("Failed to create symlink to bar package")?;

            Ok(())
        })?;

        let baz = resolve_module(
            case.db().upcast(),
            &ModuleName::new_static("bar.baz").unwrap(),
        )
        .expect("Expected bar.baz to exist in site-packages.");
        let baz_project = case.project_path("bar/baz.py");
        let baz_file = baz.file().unwrap();

        assert_eq!(source_text(case.db(), baz_file).as_str(), "def baz(): ...");
        assert_eq!(
            baz_file.path(case.db()).as_system_path(),
            Some(&*baz_project)
        );

        let baz_original = case.root_path().join("bar/baz.py");

        // Write to the symlink target.
        update_file(baz_original, "def baz(): print('Version 2')")
            .context("Failed to update bar/baz.py")?;

        let changes = case.take_watch_changes(event_for_file("baz.py"));

        case.apply_changes(changes);

        assert_eq!(
            source_text(case.db(), baz_file).as_str(),
            "def baz(): print('Version 2')"
        );

        // Write to the symlink source.
        update_file(baz_project, "def baz(): print('Version 3')")
            .context("Failed to update bar/baz.py")?;

        let changes = case.stop_watch(event_for_file("baz.py"));

        case.apply_changes(changes);

        assert_eq!(
            source_text(case.db(), baz_file).as_str(),
            "def baz(): print('Version 3')"
        );

        Ok(())
    }

    /// Project contains a symlink to another directory inside the project.
    /// Changes to files in the symlinked directory should be reflected
    /// to all files.
    ///
    /// Setup:
    /// ```text
    /// - project
    ///   | - bar -> /project/patched/bar
    ///   |
    ///   | - patched
    ///   |   |-- bar
    ///   |   |  |- baz.py
    ///   |
    ///   |-- foo.py
    /// ```
    #[test]
    fn symlink_inside_project() -> anyhow::Result<()> {
        let mut case = setup(|context: &mut SetupContext| {
            // Set up the symlink target.
            let link_target = context.join_project_path("patched/bar");
            std::fs::create_dir_all(link_target.as_std_path())
                .context("Failed to create link target directory")?;
            let baz_original = link_target.join("baz.py");
            std::fs::write(baz_original.as_std_path(), "def baz(): ...")
                .context("Failed to write link target file")?;

            // Create a symlink inside site-packages
            let bar_in_project = context.join_project_path("bar");
            std::os::unix::fs::symlink(link_target.as_std_path(), bar_in_project.as_std_path())
                .context("Failed to create symlink to bar package")?;

            Ok(())
        })?;

        let baz = resolve_module(
            case.db().upcast(),
            &ModuleName::new_static("bar.baz").unwrap(),
        )
        .expect("Expected bar.baz to exist in site-packages.");
        let baz_file = baz.file().unwrap();
        let bar_baz = case.project_path("bar/baz.py");

        let patched_bar_baz = case.project_path("patched/bar/baz.py");
        let patched_bar_baz_file = case.system_file(&patched_bar_baz).unwrap();

        assert_eq!(
            source_text(case.db(), patched_bar_baz_file).as_str(),
            "def baz(): ..."
        );

        assert_eq!(source_text(case.db(), baz_file).as_str(), "def baz(): ...");
        assert_eq!(baz_file.path(case.db()).as_system_path(), Some(&*bar_baz));

        case.assert_indexed_project_files([patched_bar_baz_file]);

        // Write to the symlink target.
        update_file(&patched_bar_baz, "def baz(): print('Version 2')")
            .context("Failed to update bar/baz.py")?;

        let changes = case.stop_watch(event_for_file("baz.py"));

        case.apply_changes(changes);

        // The file watcher is guaranteed to emit one event for the changed file, but it isn't specified
        // if the event is emitted for the "original" or linked path because both paths are watched.
        // The best we can assert here is that one of the files should have been updated.
        //
        // In a perfect world, the file watcher would emit two events, one for the original file and
        // one for the symlink. I tried parcel/watcher, node's `fs.watch` and `chokidar` and
        // only `chokidar seems to support it (used by Pyright).
        //
        // I further tested how good editor support is for symlinked files and it is not good ;)
        // * VS Code doesn't update the file content if a file gets changed through a symlink
        // * PyCharm doesn't update diagnostics if a symlinked module is changed (same as ty).
        //
        // That's why I think it's fine to not support this case for now.

        let patched_baz_text = source_text(case.db(), patched_bar_baz_file);
        let did_update_patched_baz = patched_baz_text.as_str() == "def baz(): print('Version 2')";

        let bar_baz_text = source_text(case.db(), baz_file);
        let did_update_bar_baz = bar_baz_text.as_str() == "def baz(): print('Version 2')";

        assert!(
            did_update_patched_baz || did_update_bar_baz,
            "Expected one of the files to be updated but neither file was updated.\nOriginal: {patched_baz_text}\nSymlinked: {bar_baz_text}",
            patched_baz_text = patched_baz_text.as_str(),
            bar_baz_text = bar_baz_text.as_str()
        );

        case.assert_indexed_project_files([patched_bar_baz_file]);
        Ok(())
    }

    /// A module search path is a symlink.
    ///
    /// Setup:
    /// ```text
    /// - site-packages
    ///   | - bar/baz.py
    ///
    /// - project
    ///   |-- .venv/lib/python3.12/site-packages -> /site-packages
    ///   |
    ///   |-- foo.py
    /// ```
    #[test]
    fn symlinked_module_search_path() -> anyhow::Result<()> {
        let mut case = setup(|context: &mut SetupContext| {
            // Set up the symlink target.
            let site_packages = context.join_root_path("site-packages");
            let bar = site_packages.join("bar");
            std::fs::create_dir_all(bar.as_std_path()).context("Failed to create bar directory")?;
            let baz_original = bar.join("baz.py");
            std::fs::write(baz_original.as_std_path(), "def baz(): ...")
                .context("Failed to write baz.py")?;

            // Symlink the site packages in the venv to the global site packages
            let venv_site_packages =
                context.join_project_path(".venv/lib/python3.12/site-packages");
            std::fs::create_dir_all(venv_site_packages.parent().unwrap())
                .context("Failed to create .venv directory")?;
            std::os::unix::fs::symlink(
                site_packages.as_std_path(),
                venv_site_packages.as_std_path(),
            )
            .context("Failed to create symlink to site-packages")?;

            context.set_options(Options {
                environment: Some(EnvironmentOptions {
                    extra_paths: Some(vec![RelativePathBuf::cli(
                        ".venv/lib/python3.12/site-packages",
                    )]),
                    python_version: Some(RangedValue::cli(PythonVersion::PY312)),
                    ..EnvironmentOptions::default()
                }),
                ..Options::default()
            });

            Ok(())
        })?;

        let baz = resolve_module(
            case.db().upcast(),
            &ModuleName::new_static("bar.baz").unwrap(),
        )
        .expect("Expected bar.baz to exist in site-packages.");
        let baz_site_packages_path =
            case.project_path(".venv/lib/python3.12/site-packages/bar/baz.py");
        let baz_site_packages = case.system_file(&baz_site_packages_path).unwrap();
        let baz_original = case.root_path().join("site-packages/bar/baz.py");
        let baz_original_file = case.system_file(&baz_original).unwrap();

        assert_eq!(
            source_text(case.db(), baz_original_file).as_str(),
            "def baz(): ..."
        );

        assert_eq!(
            source_text(case.db(), baz_site_packages).as_str(),
            "def baz(): ..."
        );
        assert_eq!(
            baz.file().unwrap().path(case.db()).as_system_path(),
            Some(&*baz_original)
        );

        case.assert_indexed_project_files([]);

        // Write to the symlink target.
        update_file(&baz_original, "def baz(): print('Version 2')")
            .context("Failed to update bar/baz.py")?;

        let changes = case.stop_watch(event_for_file("baz.py"));

        case.apply_changes(changes);

        assert_eq!(
            source_text(case.db(), baz_original_file).as_str(),
            "def baz(): print('Version 2')"
        );

        // It would be nice if this is supported but the underlying file system watchers
        // only emit a single event. For reference
        // * VS Code doesn't update the file content if a file gets changed through a symlink
        // * PyCharm doesn't update diagnostics if a symlinked module is changed (same as ty).
        // We could add support for it by keeping a reverse map from `real_path` to symlinked path but
        // it doesn't seem worth doing considering that as prominent tools like PyCharm don't support it.
        // Pyright does support it, thanks to chokidar.
        assert_ne!(
            source_text(case.db(), baz_site_packages).as_str(),
            "def baz(): print('Version 2')"
        );

        case.assert_indexed_project_files([]);

        Ok(())
    }
}

#[test]
fn nested_projects_delete_root() -> anyhow::Result<()> {
    let mut case = setup(|context: &mut SetupContext| {
        std::fs::write(
            context.join_project_path("pyproject.toml").as_std_path(),
            r#"
            [project]
            name = "inner"

            [tool.ty]
            "#,
        )?;

        std::fs::write(
            context.join_root_path("pyproject.toml").as_std_path(),
            r#"
            [project]
            name = "outer"

            [tool.ty]
            "#,
        )?;

        Ok(())
    })?;

    assert_eq!(case.db().project().root(case.db()), &*case.project_path(""));

    std::fs::remove_file(case.project_path("pyproject.toml").as_std_path())?;

    let changes = case.stop_watch(ChangeEvent::is_deleted);

    case.apply_changes(changes);

    // It should now pick up the outer project.
    assert_eq!(case.db().project().root(case.db()), case.root_path());

    Ok(())
}

#[test]
fn changes_to_user_configuration() -> anyhow::Result<()> {
    let mut _config_dir_override: Option<UserConfigDirectoryOverrideGuard> = None;

    let mut case = setup(|context: &mut SetupContext| {
        std::fs::write(
            context.join_project_path("pyproject.toml").as_std_path(),
            r#"
            [project]
            name = "test"
            "#,
        )?;

        std::fs::write(
            context.join_project_path("foo.py").as_std_path(),
            "a = 10 / 0",
        )?;

        let config_directory = context.join_root_path("home/.config");
        std::fs::create_dir_all(config_directory.join("ty").as_std_path())?;
        std::fs::write(
            config_directory.join("ty/ty.toml").as_std_path(),
            r#"
            [rules]
            division-by-zero = "ignore"
            "#,
        )?;

        _config_dir_override = Some(
            context
                .system()
                .with_user_config_directory(Some(config_directory)),
        );

        Ok(())
    })?;

    let foo = case
        .system_file(case.project_path("foo.py"))
        .expect("foo.py to exist");
    let diagnostics = case
        .db()
        .check_file(foo)
        .context("Failed to check project.")?;

    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics but got: {diagnostics:#?}"
    );

    // Enable division-by-zero in the user configuration with warning severity
    update_file(
        case.root_path().join("home/.config/ty/ty.toml"),
        r#"
        [rules]
        division-by-zero = "warn"
        "#,
    )?;

    let changes = case.stop_watch(event_for_file("ty.toml"));

    case.apply_changes(changes);

    let diagnostics = case
        .db()
        .check_file(foo)
        .context("Failed to check project.")?;

    assert!(
        diagnostics.len() == 1,
        "Expected exactly one diagnostic but got: {diagnostics:#?}"
    );

    Ok(())
}

/// Tests that renaming a file from `lib.py` to `Lib.py` is correctly reflected.
///
/// This test currently fails on case-insensitive systems because `Files` is case-sensitive
/// but the `System::metadata` call isn't. This means that
/// ty considers both `Lib.py` and `lib.py` to exist when only `lib.py` does
///
/// The incoming change events then are no-ops because they don't change either file's
/// status nor does it update their last modified time (renaming a file doesn't bump it's
/// last modified timestamp).
///
/// Fixing this requires to either make `Files` case-insensitive and store the
/// real-case path (if it differs) on `File` or make `Files` use a
/// case-sensitive `System::metadata` call. This does open the question if all
/// `System` calls should be case sensitive. This would be the most consistent
/// but might be hard to pull off.
///
/// What the right solution is also depends on if ty itself should be case
/// sensitive or not. E.g. should `include="src"` be case sensitive on all systems
/// or only on case-sensitive systems?
///
/// Lastly, whatever solution we pick must also work well with VS Code which,
/// unfortunately ,doesn't propagate casing-only renames.
/// <https://github.com/rust-lang/rust-analyzer/issues/9581>
#[ignore]
#[test]
fn rename_files_casing_only() -> anyhow::Result<()> {
    let mut case = setup([("lib.py", "class Foo: ...")])?;

    assert!(
        resolve_module(case.db(), &ModuleName::new("lib").unwrap()).is_some(),
        "Expected `lib` module to exist."
    );
    assert_eq!(
        resolve_module(case.db(), &ModuleName::new("Lib").unwrap()),
        None,
        "Expected `Lib` module not to exist"
    );

    // Now rename `lib.py` to `Lib.py`
    if case.db().system().case_sensitivity().is_case_sensitive() {
        std::fs::rename(
            case.project_path("lib.py").as_std_path(),
            case.project_path("Lib.py").as_std_path(),
        )
        .context("Failed to rename `lib.py` to `Lib.py`")?;
    } else {
        // On case-insensitive file systems, renaming a file to a different casing is a no-op.
        // Rename to a different name first
        std::fs::rename(
            case.project_path("lib.py").as_std_path(),
            case.project_path("temp.py").as_std_path(),
        )
        .context("Failed to rename `lib.py` to `temp.py`")?;

        std::fs::rename(
            case.project_path("temp.py").as_std_path(),
            case.project_path("Lib.py").as_std_path(),
        )
        .context("Failed to rename `temp.py` to `Lib.py`")?;
    }

    let changes = case.stop_watch(event_for_file("Lib.py"));
    case.apply_changes(changes);

    // Resolving `lib` should now fail but `Lib` should now succeed
    assert_eq!(
        resolve_module(case.db(), &ModuleName::new("lib").unwrap()),
        None,
        "Expected `lib` module to no longer exist."
    );

    assert!(
        resolve_module(case.db(), &ModuleName::new("Lib").unwrap()).is_some(),
        "Expected `Lib` module to exist"
    );

    Ok(())
}
