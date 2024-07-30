#![allow(clippy::disallowed_names)]

use std::io::Write;
use std::time::Duration;

use anyhow::{anyhow, Context};
use salsa::Setter;

use red_knot::db::RootDatabase;
use red_knot::watch;
use red_knot::watch::{directory_watcher, WorkspaceWatcher};
use red_knot::workspace::WorkspaceMetadata;
use red_knot_module_resolver::{resolve_module, ModuleName};
use ruff_db::files::{system_path_to_file, File, FileError};
use ruff_db::program::{Program, ProgramSettings, SearchPathSettings, TargetVersion};
use ruff_db::source::source_text;
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf};
use ruff_db::Upcast;

struct TestCase {
    db: RootDatabase,
    watcher: Option<WorkspaceWatcher>,
    changes_receiver: crossbeam::channel::Receiver<Vec<watch::ChangeEvent>>,
    _temp_dir: tempfile::TempDir,
    root_dir: SystemPathBuf,
}

impl TestCase {
    fn workspace_path(&self, relative: impl AsRef<SystemPath>) -> SystemPathBuf {
        SystemPath::absolute(relative, self.db.workspace().root(&self.db))
    }

    fn root_path(&self) -> &SystemPath {
        &self.root_dir
    }

    fn db(&self) -> &RootDatabase {
        &self.db
    }

    fn db_mut(&mut self) -> &mut RootDatabase {
        &mut self.db
    }

    fn stop_watch(&mut self) -> Vec<watch::ChangeEvent> {
        self.try_stop_watch(Duration::from_secs(10))
            .expect("Expected watch changes but observed none.")
    }

    fn try_stop_watch(&mut self, timeout: Duration) -> Option<Vec<watch::ChangeEvent>> {
        let watcher = self.watcher.take().unwrap();

        let mut all_events = self
            .changes_receiver
            .recv_timeout(timeout)
            .unwrap_or_default();
        watcher.flush();
        watcher.stop();

        for event in &self.changes_receiver {
            all_events.extend(event);
        }

        if all_events.is_empty() {
            return None;
        }

        Some(all_events)
    }

    #[cfg(unix)]
    fn take_watch_changes(&self) -> Vec<watch::ChangeEvent> {
        self.try_take_watch_changes(Duration::from_secs(10))
            .expect("Expected watch changes but observed none.")
    }

    fn try_take_watch_changes(&self, timeout: Duration) -> Option<Vec<watch::ChangeEvent>> {
        let Some(watcher) = &self.watcher else {
            return None;
        };

        let mut all_events = self
            .changes_receiver
            .recv_timeout(timeout)
            .unwrap_or_default();
        watcher.flush();

        while let Ok(event) = self
            .changes_receiver
            .recv_timeout(Duration::from_millis(10))
        {
            all_events.extend(event);
            watcher.flush();
        }

        if all_events.is_empty() {
            return None;
        }
        Some(all_events)
    }

    fn update_search_path_settings(
        &mut self,
        f: impl FnOnce(&SearchPathSettings) -> SearchPathSettings,
    ) {
        let program = Program::get(self.db());
        let search_path_settings = program.search_paths(self.db());

        let new_settings = f(search_path_settings);

        program.set_search_paths(&mut self.db).to(new_settings);

        if let Some(watcher) = &mut self.watcher {
            watcher.update(&self.db);
            assert!(!watcher.has_errored_paths());
        }
    }

    fn collect_package_files(&self, path: &SystemPath) -> Vec<File> {
        let package = self.db().workspace().package(self.db(), path).unwrap();
        let files = package.files(self.db());
        let files = files.read();
        let mut collected: Vec<_> = files.into_iter().collect();
        collected.sort_unstable_by_key(|file| file.path(self.db()).as_system_path().unwrap());
        collected
    }

    fn system_file(&self, path: impl AsRef<SystemPath>) -> Result<File, FileError> {
        system_path_to_file(self.db(), path.as_ref())
    }
}

trait SetupFiles {
    fn setup(self, root_path: &SystemPath, workspace_path: &SystemPath) -> anyhow::Result<()>;
}

impl<const N: usize, P> SetupFiles for [(P, &'static str); N]
where
    P: AsRef<SystemPath>,
{
    fn setup(self, _root_path: &SystemPath, workspace_path: &SystemPath) -> anyhow::Result<()> {
        for (relative_path, content) in self {
            let relative_path = relative_path.as_ref();
            let absolute_path = workspace_path.join(relative_path);
            if let Some(parent) = absolute_path.parent() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to create parent directory for file '{relative_path}'.",)
                })?;
            }

            let mut file = std::fs::File::create(absolute_path.as_std_path())
                .with_context(|| format!("Failed to open file '{relative_path}'"))?;
            file.write_all(content.as_bytes())
                .with_context(|| format!("Failed to write to file '{relative_path}'"))?;
            file.sync_data()?;
        }

        Ok(())
    }
}

impl<F> SetupFiles for F
where
    F: FnOnce(&SystemPath, &SystemPath) -> anyhow::Result<()>,
{
    fn setup(self, root_path: &SystemPath, workspace_path: &SystemPath) -> anyhow::Result<()> {
        self(root_path, workspace_path)
    }
}

fn setup<F>(setup_files: F) -> anyhow::Result<TestCase>
where
    F: SetupFiles,
{
    setup_with_search_paths(setup_files, |_root, workspace_path| SearchPathSettings {
        extra_paths: vec![],
        workspace_root: workspace_path.to_path_buf(),
        custom_typeshed: None,
        site_packages: None,
    })
}

fn setup_with_search_paths<F>(
    setup_files: F,
    create_search_paths: impl FnOnce(&SystemPath, &SystemPath) -> SearchPathSettings,
) -> anyhow::Result<TestCase>
where
    F: SetupFiles,
{
    let temp_dir = tempfile::tempdir()?;

    let root_path = SystemPath::from_std_path(temp_dir.path()).ok_or_else(|| {
        anyhow!(
            "Temp directory '{}' is not a valid UTF-8 path.",
            temp_dir.path().display()
        )
    })?;

    let root_path = SystemPathBuf::from_utf8_path_buf(
        root_path
            .as_utf8_path()
            .canonicalize_utf8()
            .with_context(|| "Failed to canonicalize root path.")?,
    );

    let workspace_path = root_path.join("workspace");

    std::fs::create_dir_all(workspace_path.as_std_path())
        .with_context(|| format!("Failed to create workspace directory '{workspace_path}'",))?;

    setup_files
        .setup(&root_path, &workspace_path)
        .context("Failed to setup test files")?;

    let system = OsSystem::new(&workspace_path);

    let workspace = WorkspaceMetadata::from_path(&workspace_path, &system)?;
    let search_paths = create_search_paths(&root_path, workspace.root());

    for path in search_paths
        .extra_paths
        .iter()
        .chain(search_paths.site_packages.iter())
        .chain(search_paths.custom_typeshed.iter())
    {
        std::fs::create_dir_all(path.as_std_path())
            .with_context(|| format!("Failed to create search path '{path}'"))?;
    }

    let settings = ProgramSettings {
        target_version: TargetVersion::default(),
        search_paths,
    };

    let db = RootDatabase::new(workspace, settings, system);

    let (sender, receiver) = crossbeam::channel::unbounded();
    let watcher = directory_watcher(move |events| sender.send(events).unwrap())
        .with_context(|| "Failed to create directory watcher")?;

    let watcher = WorkspaceWatcher::new(watcher, &db);
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
    test_case.try_take_watch_changes(Duration::from_millis(100));

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

        filetime::set_file_handle_times(&file, None, Some(filetime::FileTime::now()))?;
    }
}

#[test]
fn new_file() -> anyhow::Result<()> {
    let mut case = setup([("bar.py", "")])?;
    let bar_path = case.workspace_path("bar.py");
    let bar_file = case.system_file(&bar_path).unwrap();
    let foo_path = case.workspace_path("foo.py");

    assert_eq!(case.system_file(&foo_path), Err(FileError::NotFound));
    assert_eq!(&case.collect_package_files(&bar_path), &[bar_file]);

    std::fs::write(foo_path.as_std_path(), "print('Hello')")?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    let foo = case.system_file(&foo_path).expect("foo.py to exist.");

    assert_eq!(&case.collect_package_files(&bar_path), &[bar_file, foo]);

    Ok(())
}

#[test]
fn new_ignored_file() -> anyhow::Result<()> {
    let mut case = setup([("bar.py", ""), (".ignore", "foo.py")])?;
    let bar_path = case.workspace_path("bar.py");
    let bar_file = case.system_file(&bar_path).unwrap();
    let foo_path = case.workspace_path("foo.py");

    assert_eq!(case.system_file(&foo_path), Err(FileError::NotFound));
    assert_eq!(&case.collect_package_files(&bar_path), &[bar_file]);

    std::fs::write(foo_path.as_std_path(), "print('Hello')")?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    assert!(case.system_file(&foo_path).is_ok());
    assert_eq!(&case.collect_package_files(&bar_path), &[bar_file]);

    Ok(())
}

#[test]
fn changed_file() -> anyhow::Result<()> {
    let foo_source = "print('Hello, world!')";
    let mut case = setup([("foo.py", foo_source)])?;
    let foo_path = case.workspace_path("foo.py");

    let foo = case.system_file(&foo_path)?;
    assert_eq!(source_text(case.db(), foo).as_str(), foo_source);
    assert_eq!(&case.collect_package_files(&foo_path), &[foo]);

    update_file(&foo_path, "print('Version 2')")?;

    let changes = case.stop_watch();

    assert!(!changes.is_empty());

    case.db_mut().apply_changes(changes);

    assert_eq!(source_text(case.db(), foo).as_str(), "print('Version 2')");
    assert_eq!(&case.collect_package_files(&foo_path), &[foo]);

    Ok(())
}

#[test]
fn deleted_file() -> anyhow::Result<()> {
    let foo_source = "print('Hello, world!')";
    let mut case = setup([("foo.py", foo_source)])?;
    let foo_path = case.workspace_path("foo.py");

    let foo = case.system_file(&foo_path)?;

    assert!(foo.exists(case.db()));
    assert_eq!(&case.collect_package_files(&foo_path), &[foo]);

    std::fs::remove_file(foo_path.as_std_path())?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    assert!(!foo.exists(case.db()));
    assert_eq!(&case.collect_package_files(&foo_path), &[] as &[File]);

    Ok(())
}

/// Tests the case where a file is moved from inside a watched directory to a directory that is not watched.
///
/// This matches the behavior of deleting a file in VS code.
#[test]
fn move_file_to_trash() -> anyhow::Result<()> {
    let foo_source = "print('Hello, world!')";
    let mut case = setup([("foo.py", foo_source)])?;
    let foo_path = case.workspace_path("foo.py");

    let trash_path = case.root_path().join(".trash");
    std::fs::create_dir_all(trash_path.as_std_path())?;

    let foo = case.system_file(&foo_path)?;

    assert!(foo.exists(case.db()));
    assert_eq!(&case.collect_package_files(&foo_path), &[foo]);

    std::fs::rename(
        foo_path.as_std_path(),
        trash_path.join("foo.py").as_std_path(),
    )?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    assert!(!foo.exists(case.db()));
    assert_eq!(&case.collect_package_files(&foo_path), &[] as &[File]);

    Ok(())
}

/// Move a file from a non-workspace (non-watched) location into the workspace.
#[test]
fn move_file_to_workspace() -> anyhow::Result<()> {
    let mut case = setup([("bar.py", "")])?;
    let bar_path = case.workspace_path("bar.py");
    let bar = case.system_file(&bar_path).unwrap();

    let foo_path = case.root_path().join("foo.py");
    std::fs::write(foo_path.as_std_path(), "")?;

    let foo_in_workspace_path = case.workspace_path("foo.py");

    assert!(case.system_file(&foo_path).is_ok());
    assert_eq!(&case.collect_package_files(&bar_path), &[bar]);
    assert!(case
        .db()
        .workspace()
        .package(case.db(), &foo_path)
        .is_none());

    std::fs::rename(foo_path.as_std_path(), foo_in_workspace_path.as_std_path())?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    let foo_in_workspace = case.system_file(&foo_in_workspace_path)?;

    assert!(foo_in_workspace.exists(case.db()));
    assert_eq!(
        &case.collect_package_files(&foo_in_workspace_path),
        &[bar, foo_in_workspace]
    );

    Ok(())
}

/// Rename a workspace file.
#[test]
fn rename_file() -> anyhow::Result<()> {
    let mut case = setup([("foo.py", "")])?;
    let foo_path = case.workspace_path("foo.py");
    let bar_path = case.workspace_path("bar.py");

    let foo = case.system_file(&foo_path)?;

    assert_eq!(case.collect_package_files(&foo_path), [foo]);

    std::fs::rename(foo_path.as_std_path(), bar_path.as_std_path())?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    assert!(!foo.exists(case.db()));

    let bar = case.system_file(&bar_path)?;

    assert!(bar.exists(case.db()));
    assert_eq!(case.collect_package_files(&foo_path), [bar]);

    Ok(())
}

#[test]
fn directory_moved_to_workspace() -> anyhow::Result<()> {
    let mut case = setup([("bar.py", "import sub.a")])?;
    let bar = case.system_file(case.workspace_path("bar.py")).unwrap();

    let sub_original_path = case.root_path().join("sub");
    let init_original_path = sub_original_path.join("__init__.py");
    let a_original_path = sub_original_path.join("a.py");

    std::fs::create_dir(sub_original_path.as_std_path())
        .with_context(|| "Failed to create sub directory")?;
    std::fs::write(init_original_path.as_std_path(), "")
        .with_context(|| "Failed to create __init__.py")?;
    std::fs::write(a_original_path.as_std_path(), "").with_context(|| "Failed to create a.py")?;

    let sub_a_module = resolve_module(case.db().upcast(), ModuleName::new_static("sub.a").unwrap());

    assert_eq!(sub_a_module, None);
    assert_eq!(
        case.collect_package_files(&case.workspace_path("bar.py")),
        &[bar]
    );

    let sub_new_path = case.workspace_path("sub");
    std::fs::rename(sub_original_path.as_std_path(), sub_new_path.as_std_path())
        .with_context(|| "Failed to move sub directory")?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    let init_file = case
        .system_file(sub_new_path.join("__init__.py"))
        .expect("__init__.py to exist");
    let a_file = case
        .system_file(sub_new_path.join("a.py"))
        .expect("a.py to exist");

    // `import sub.a` should now resolve
    assert!(resolve_module(case.db().upcast(), ModuleName::new_static("sub.a").unwrap()).is_some());

    assert_eq!(
        case.collect_package_files(&case.workspace_path("bar.py")),
        &[bar, init_file, a_file]
    );

    Ok(())
}

#[test]
fn directory_moved_to_trash() -> anyhow::Result<()> {
    let mut case = setup([
        ("bar.py", "import sub.a"),
        ("sub/__init__.py", ""),
        ("sub/a.py", ""),
    ])?;
    let bar = case.system_file(case.workspace_path("bar.py")).unwrap();

    assert!(resolve_module(case.db().upcast(), ModuleName::new_static("sub.a").unwrap()).is_some());

    let sub_path = case.workspace_path("sub");
    let init_file = case
        .system_file(sub_path.join("__init__.py"))
        .expect("__init__.py to exist");
    let a_file = case
        .system_file(sub_path.join("a.py"))
        .expect("a.py to exist");

    assert_eq!(
        case.collect_package_files(&case.workspace_path("bar.py")),
        &[bar, init_file, a_file]
    );

    std::fs::create_dir(case.root_path().join(".trash").as_std_path())?;
    let trashed_sub = case.root_path().join(".trash/sub");
    std::fs::rename(sub_path.as_std_path(), trashed_sub.as_std_path())
        .with_context(|| "Failed to move the sub directory to the trash")?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    // `import sub.a` should no longer resolve
    assert!(resolve_module(case.db().upcast(), ModuleName::new_static("sub.a").unwrap()).is_none());

    assert!(!init_file.exists(case.db()));
    assert!(!a_file.exists(case.db()));

    assert_eq!(
        case.collect_package_files(&case.workspace_path("bar.py")),
        &[bar]
    );

    Ok(())
}

#[test]
fn directory_renamed() -> anyhow::Result<()> {
    let mut case = setup([
        ("bar.py", "import sub.a"),
        ("sub/__init__.py", ""),
        ("sub/a.py", ""),
    ])?;

    let bar = case.system_file(case.workspace_path("bar.py")).unwrap();

    assert!(resolve_module(case.db().upcast(), ModuleName::new_static("sub.a").unwrap()).is_some());
    assert!(resolve_module(
        case.db().upcast(),
        ModuleName::new_static("foo.baz").unwrap()
    )
    .is_none());

    let sub_path = case.workspace_path("sub");
    let sub_init = case
        .system_file(sub_path.join("__init__.py"))
        .expect("__init__.py to exist");
    let sub_a = case
        .system_file(sub_path.join("a.py"))
        .expect("a.py to exist");

    assert_eq!(
        case.collect_package_files(&sub_path),
        &[bar, sub_init, sub_a]
    );

    let foo_baz = case.workspace_path("foo/baz");

    std::fs::create_dir(case.workspace_path("foo").as_std_path())?;
    std::fs::rename(sub_path.as_std_path(), foo_baz.as_std_path())
        .with_context(|| "Failed to move the sub directory")?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    // `import sub.a` should no longer resolve
    assert!(resolve_module(case.db().upcast(), ModuleName::new_static("sub.a").unwrap()).is_none());
    // `import foo.baz` should now resolve
    assert!(resolve_module(
        case.db().upcast(),
        ModuleName::new_static("foo.baz").unwrap()
    )
    .is_some());

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

    assert_eq!(
        case.collect_package_files(&sub_path),
        &[bar, foo_baz_init, foo_baz_a]
    );

    Ok(())
}

#[test]
fn directory_deleted() -> anyhow::Result<()> {
    let mut case = setup([
        ("bar.py", "import sub.a"),
        ("sub/__init__.py", ""),
        ("sub/a.py", ""),
    ])?;

    let bar = case.system_file(case.workspace_path("bar.py")).unwrap();

    assert!(resolve_module(case.db().upcast(), ModuleName::new_static("sub.a").unwrap()).is_some(),);

    let sub_path = case.workspace_path("sub");

    let init_file = case
        .system_file(sub_path.join("__init__.py"))
        .expect("__init__.py to exist");
    let a_file = case
        .system_file(sub_path.join("a.py"))
        .expect("a.py to exist");
    assert_eq!(
        case.collect_package_files(&sub_path),
        &[bar, init_file, a_file]
    );

    std::fs::remove_dir_all(sub_path.as_std_path())
        .with_context(|| "Failed to remove the sub directory")?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    // `import sub.a` should no longer resolve
    assert!(resolve_module(case.db().upcast(), ModuleName::new_static("sub.a").unwrap()).is_none());

    assert!(!init_file.exists(case.db()));
    assert!(!a_file.exists(case.db()));
    assert_eq!(case.collect_package_files(&sub_path), &[bar]);

    Ok(())
}

#[test]
fn search_path() -> anyhow::Result<()> {
    let mut case =
        setup_with_search_paths([("bar.py", "import sub.a")], |root_path, workspace_path| {
            SearchPathSettings {
                extra_paths: vec![],
                workspace_root: workspace_path.to_path_buf(),
                custom_typeshed: None,
                site_packages: Some(root_path.join("site_packages")),
            }
        })?;

    let site_packages = case.root_path().join("site_packages");

    assert_eq!(
        resolve_module(case.db(), ModuleName::new("a").unwrap()),
        None
    );

    std::fs::write(site_packages.join("a.py").as_std_path(), "class A: ...")?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    assert!(resolve_module(case.db().upcast(), ModuleName::new_static("a").unwrap()).is_some());
    assert_eq!(
        case.collect_package_files(&case.workspace_path("bar.py")),
        &[case.system_file(case.workspace_path("bar.py")).unwrap()]
    );

    Ok(())
}

#[test]
fn add_search_path() -> anyhow::Result<()> {
    let mut case = setup([("bar.py", "import sub.a")])?;

    let site_packages = case.workspace_path("site_packages");
    std::fs::create_dir_all(site_packages.as_std_path())?;

    assert!(resolve_module(case.db().upcast(), ModuleName::new_static("a").unwrap()).is_none());

    // Register site-packages as a search path.
    case.update_search_path_settings(|settings| SearchPathSettings {
        site_packages: Some(site_packages.clone()),
        ..settings.clone()
    });

    std::fs::write(site_packages.join("a.py").as_std_path(), "class A: ...")?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    assert!(resolve_module(case.db().upcast(), ModuleName::new_static("a").unwrap()).is_some());

    Ok(())
}

#[test]
fn remove_search_path() -> anyhow::Result<()> {
    let mut case =
        setup_with_search_paths([("bar.py", "import sub.a")], |root_path, workspace_path| {
            SearchPathSettings {
                extra_paths: vec![],
                workspace_root: workspace_path.to_path_buf(),
                custom_typeshed: None,
                site_packages: Some(root_path.join("site_packages")),
            }
        })?;

    // Remove site packages from the search path settings.
    let site_packages = case.root_path().join("site_packages");
    case.update_search_path_settings(|settings| SearchPathSettings {
        site_packages: None,
        ..settings.clone()
    });

    std::fs::write(site_packages.join("a.py").as_std_path(), "class A: ...")?;

    let changes = case.try_stop_watch(Duration::from_millis(100));

    assert_eq!(changes, None);

    Ok(())
}

/// Watch a workspace that contains two files where one file is a hardlink to another.
///
/// Setup:
/// ```text
/// - workspace
///   |- foo.py
///   |- bar.py (hard link to foo.py)
/// ```
///
/// # Linux
/// `inotify` only emits a single change event for the file that was changed.
/// Other files that point to the same inode (hardlinks) won't get updated.
///
/// For reference: VS Code and Py Charm have the same behavior where the results for one of the
/// files are stale.
///
/// # Windows
/// I haven't found any documentation that states the notification behavior on Windows but what
/// we're seeing is that Windows only emits a single event, similar to Linux.
#[test]
fn hard_links_in_workspace() -> anyhow::Result<()> {
    let mut case = setup(|_root: &SystemPath, workspace: &SystemPath| {
        let foo_path = workspace.join("foo.py");
        std::fs::write(foo_path.as_std_path(), "print('Version 1')")?;

        // Create a hardlink to `foo`
        let bar_path = workspace.join("bar.py");
        std::fs::hard_link(foo_path.as_std_path(), bar_path.as_std_path())
            .context("Failed to create hard link from foo.py -> bar.py")?;

        Ok(())
    })?;

    let foo_path = case.workspace_path("foo.py");
    let foo = case.system_file(&foo_path).unwrap();
    let bar_path = case.workspace_path("bar.py");
    let bar = case.system_file(&bar_path).unwrap();

    assert_eq!(source_text(case.db(), foo).as_str(), "print('Version 1')");
    assert_eq!(source_text(case.db(), bar).as_str(), "print('Version 1')");

    // Write to the hard link target.
    update_file(foo_path, "print('Version 2')").context("Failed to update foo.py")?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    assert_eq!(source_text(case.db(), foo).as_str(), "print('Version 2')");

    // macOS is the only platform that emits events for every hardlink.
    if cfg!(target_os = "macos") {
        assert_eq!(source_text(case.db(), bar).as_str(), "print('Version 2')");
    }

    Ok(())
}

/// Watch a workspace that contains one file that is a hardlink to a file outside the workspace.
///
/// Setup:
/// ```text
/// - foo.py
/// - workspace
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
/// hard linked files outside the workspace.
#[test]
#[cfg_attr(
    target_os = "linux",
    ignore = "inotify doesn't support observing changes to hard linked files."
)]
#[cfg_attr(
    target_os = "windows",
    ignore = "windows doesn't support observing changes to hard linked files."
)]
fn hard_links_to_target_outside_workspace() -> anyhow::Result<()> {
    let mut case = setup(|root: &SystemPath, workspace: &SystemPath| {
        let foo_path = root.join("foo.py");
        std::fs::write(foo_path.as_std_path(), "print('Version 1')")?;

        // Create a hardlink to `foo`
        let bar_path = workspace.join("bar.py");
        std::fs::hard_link(foo_path.as_std_path(), bar_path.as_std_path())
            .context("Failed to create hard link from foo.py -> bar.py")?;

        Ok(())
    })?;

    let foo_path = case.root_path().join("foo.py");
    let foo = case.system_file(&foo_path).unwrap();
    let bar_path = case.workspace_path("bar.py");
    let bar = case.system_file(&bar_path).unwrap();

    assert_eq!(source_text(case.db(), foo).as_str(), "print('Version 1')");
    assert_eq!(source_text(case.db(), bar).as_str(), "print('Version 1')");

    // Write to the hard link target.
    update_file(foo_path, "print('Version 2')").context("Failed to update foo.py")?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    assert_eq!(source_text(case.db(), bar).as_str(), "print('Version 2')");

    Ok(())
}

#[cfg(unix)]
mod unix {
    //! Tests that make use of unix specific file-system features.
    use super::*;

    /// Changes the metadata of the only file in the workspace.
    #[test]
    fn changed_metadata() -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let mut case = setup([("foo.py", "")])?;
        let foo_path = case.workspace_path("foo.py");

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

        let changes = case.stop_watch();

        case.db_mut().apply_changes(changes);

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

    /// A workspace path is a symlink to a file outside the workspace.
    ///
    /// Setup:
    /// ```text
    /// - bar
    ///   |- baz.py
    ///
    /// - workspace
    ///   |- foo.py
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
        let mut case = setup(|root: &SystemPath, workspace: &SystemPath| {
            // Set up the symlink target.
            let link_target = root.join("bar");
            std::fs::create_dir_all(link_target.as_std_path())
                .context("Failed to create link target directory")?;
            let baz_original = link_target.join("baz.py");
            std::fs::write(baz_original.as_std_path(), "def baz(): ...")
                .context("Failed to write link target file")?;

            // Create a symlink inside the workspace
            let bar = workspace.join("bar");
            std::os::unix::fs::symlink(link_target.as_std_path(), bar.as_std_path())
                .context("Failed to create symlink to bar package")?;

            Ok(())
        })?;

        let baz = resolve_module(
            case.db().upcast(),
            ModuleName::new_static("bar.baz").unwrap(),
        )
        .expect("Expected bar.baz to exist in site-packages.");
        let baz_workspace = case.workspace_path("bar/baz.py");

        assert_eq!(
            source_text(case.db(), baz.file()).as_str(),
            "def baz(): ..."
        );
        assert_eq!(
            baz.file().path(case.db()).as_system_path(),
            Some(&*baz_workspace)
        );

        let baz_original = case.root_path().join("bar/baz.py");

        // Write to the symlink target.
        update_file(baz_original, "def baz(): print('Version 2')")
            .context("Failed to update bar/baz.py")?;

        let changes = case.take_watch_changes();

        case.db_mut().apply_changes(changes);

        assert_eq!(
            source_text(case.db(), baz.file()).as_str(),
            "def baz(): print('Version 2')"
        );

        // Write to the symlink source.
        update_file(baz_workspace, "def baz(): print('Version 3')")
            .context("Failed to update bar/baz.py")?;

        let changes = case.stop_watch();

        case.db_mut().apply_changes(changes);

        assert_eq!(
            source_text(case.db(), baz.file()).as_str(),
            "def baz(): print('Version 3')"
        );

        Ok(())
    }

    /// Workspace contains a symlink to another directory inside the workspace.
    /// Changes to files in the symlinked directory should be reflected
    /// to all files.
    ///
    /// Setup:
    /// ```text
    /// - workspace
    ///   |-- .venv/lib/python3.12/site-packages
    ///   |  |- bar -> /workspace/patched/bar
    ///   |
    ///   | - patched
    ///   |   |-- bar
    ///   |   |  |- baz.py
    ///   |
    ///   |-- foo.py
    /// ```
    #[test]
    fn symlink_inside_workspace() -> anyhow::Result<()> {
        let mut case = setup_with_search_paths(
            |_root: &SystemPath, workspace: &SystemPath| {
                let site_packages = workspace.join(".venv/lib/python3.12/site-packages");
                std::fs::create_dir_all(site_packages.as_std_path())?;

                // Set up the symlink target.
                let link_target = workspace.join("patched/bar");
                std::fs::create_dir_all(link_target.as_std_path())
                    .context("Failed to create link target directory")?;
                let baz_original = link_target.join("baz.py");
                std::fs::write(baz_original.as_std_path(), "def baz(): ...")
                    .context("Failed to write link target file")?;

                // Create a symlink inside site-packages
                let bar_in_site_packages = site_packages.join("bar");
                std::os::unix::fs::symlink(
                    link_target.as_std_path(),
                    bar_in_site_packages.as_std_path(),
                )
                .context("Failed to create symlink to bar package")?;

                Ok(())
            },
            |_root, workspace| SearchPathSettings {
                extra_paths: vec![],
                workspace_root: workspace.to_path_buf(),
                custom_typeshed: None,
                site_packages: Some(workspace.join(".venv/lib/python3.12/site-packages")),
            },
        )?;

        let baz = resolve_module(
            case.db().upcast(),
            ModuleName::new_static("bar.baz").unwrap(),
        )
        .expect("Expected bar.baz to exist in extra-paths.");
        let baz_site_packages =
            case.workspace_path(".venv/lib/python3.12/site-packages/bar/baz.py");

        let baz_original = case.workspace_path("patched/bar/baz.py");
        let baz_original_file = case.system_file(&baz_original).unwrap();

        assert_eq!(
            source_text(case.db(), baz_original_file).as_str(),
            "def baz(): ..."
        );

        assert_eq!(
            source_text(case.db(), baz.file()).as_str(),
            "def baz(): ..."
        );
        assert_eq!(
            baz.file().path(case.db()).as_system_path(),
            Some(&*baz_site_packages)
        );

        // Write to the symlink target.
        update_file(&baz_original, "def baz(): print('Version 2')")
            .context("Failed to update bar/baz.py")?;

        let changes = case.stop_watch();

        case.db_mut().apply_changes(changes);

        // The file watcher is guaranteed to emit one event for the changed file, but it isn't specified
        // if the event is emitted for the "original" or linked path because both paths are watched.
        // The best we can assert here is that one of the file should have been updated.
        //
        // In a perfect world, the file watcher would emit two events, one for the original file and
        // one for the symlink. I tried parcel/watcher, node's `fs.watch` and `chokidar` and
        // only `chockidar seems to support it (used by Pyright).
        //
        // I further tested how good editor support is for symlinked files and it is not good ;)
        // * VS Code doesn't update the file content if a file gets changed through a symlink
        // * Py Charm doesn't update diagnostics if a symlinked module is changed (same as red knot).
        //
        // That's why I think it's fine to not support this case for now.

        let original_text = source_text(case.db(), baz_original_file);
        let original_updated = original_text.as_str() == "def baz(): print('Version 2')";

        let symlinked_text = source_text(case.db(), baz.file());
        let symlinked_updated = symlinked_text.as_str() == "def baz(): print('Version 2')";

        assert!(original_updated || symlinked_updated, "Expected one of the files to be updated but neither file was updated.\nOriginal: {original_text}\nSymlinked: {symlinked_text}", original_text = original_text.as_str(), symlinked_text = symlinked_text.as_str());

        Ok(())
    }

    /// A module search path is a symlink.
    ///
    /// Setup:
    /// ```text
    /// - site-packages
    ///   | - bar/baz.py
    ///
    /// - workspace
    ///   |-- .venv/lib/python3.12/site-packages -> /site-packages
    ///   |
    ///   |-- foo.py
    /// ```
    #[test]
    // #[ignore = "Requires tracking symlink information. Only emits a single event for the target file."]
    fn symlinked_module_search_path() -> anyhow::Result<()> {
        let mut case = setup_with_search_paths(
            |root: &SystemPath, workspace: &SystemPath| {
                // Set up the symlink target.
                let site_packages = root.join("site-packages");
                let bar = site_packages.join("bar");
                std::fs::create_dir_all(bar.as_std_path())
                    .context("Failed to create bar directory")?;
                let baz_original = bar.join("baz.py");
                std::fs::write(baz_original.as_std_path(), "def baz(): ...")
                    .context("Failed to write baz.py")?;

                // Symlink the site packages in the venv to the global site packages
                let venv_site_packages = workspace.join(".venv/lib/python3.12/site-packages");
                std::fs::create_dir_all(venv_site_packages.parent().unwrap())
                    .context("Failed to create .venv directory")?;
                std::os::unix::fs::symlink(
                    site_packages.as_std_path(),
                    venv_site_packages.as_std_path(),
                )
                .context("Failed to create symlink to site-packages")?;

                Ok(())
            },
            |_root, workspace| SearchPathSettings {
                extra_paths: vec![],
                workspace_root: workspace.to_path_buf(),
                custom_typeshed: None,
                site_packages: Some(workspace.join(".venv/lib/python3.12/site-packages")),
            },
        )?;

        let baz = resolve_module(
            case.db().upcast(),
            ModuleName::new_static("bar.baz").unwrap(),
        )
        .expect("Expected bar.baz to exist in site-packages.");
        let baz_site_packages =
            case.workspace_path(".venv/lib/python3.12/site-packages/bar/baz.py");
        let baz_original = case.root_path().join("site-packages/bar/baz.py");
        let baz_original_file = case.system_file(&baz_original).unwrap();

        assert_eq!(
            source_text(case.db(), baz_original_file).as_str(),
            "def baz(): ..."
        );

        assert_eq!(
            source_text(case.db(), baz.file()).as_str(),
            "def baz(): ..."
        );
        assert_eq!(
            baz.file().path(case.db()).as_system_path(),
            Some(&*baz_site_packages)
        );

        // Write to the symlink target.
        update_file(&baz_original, "def baz(): print('Version 2')")
            .context("Failed to update bar/baz.py")?;

        let changes = case.stop_watch();

        case.db_mut().apply_changes(changes);

        assert_eq!(
            source_text(case.db(), baz.file()).as_str(),
            "def baz(): print('Version 2')"
        );

        // It would be nice if this is supported but the underlying file system watchers
        // only emit a single event. For reference
        // * VS Code doesn't update the file content if a file gets changed through a symlink
        // * PyCharm doesn't update diagnostics if a symlinked module is changed (same as red knot).
        // We could add support for it by keeping a reverse map from `real_path` to symlinked path but
        // it doesn't seem worth doing considering that as prominent tools like PyCharm don't support it.
        // PyRight does support it, thanks to chokidar.
        assert_ne!(
            source_text(case.db(), baz_original_file).as_str(),
            "def baz(): print('Version 2')"
        );

        Ok(())
    }
}
