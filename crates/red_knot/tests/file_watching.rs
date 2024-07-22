#![allow(clippy::disallowed_names)]

use std::time::Duration;

use anyhow::{anyhow, Context};

use red_knot::db::RootDatabase;
use red_knot::watch;
use red_knot::watch::{directory_watcher, Watcher};
use red_knot::workspace::WorkspaceMetadata;
use red_knot_module_resolver::{resolve_module, ModuleName};
use ruff_db::files::{system_path_to_file, File};
use ruff_db::program::{ProgramSettings, SearchPathSettings, TargetVersion};
use ruff_db::source::source_text;
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf};
use ruff_db::Upcast;

struct TestCase {
    db: RootDatabase,
    watcher: Option<Watcher>,
    changes_receiver: crossbeam::channel::Receiver<Vec<watch::ChangeEvent>>,
    temp_dir: tempfile::TempDir,
}

impl TestCase {
    fn workspace_path(&self, relative: impl AsRef<SystemPath>) -> SystemPathBuf {
        SystemPath::absolute(relative, self.db.workspace().root(&self.db))
    }

    fn root_path(&self) -> &SystemPath {
        SystemPath::from_std_path(self.temp_dir.path()).unwrap()
    }

    fn db(&self) -> &RootDatabase {
        &self.db
    }

    fn db_mut(&mut self) -> &mut RootDatabase {
        &mut self.db
    }

    fn stop_watch(&mut self) -> Vec<watch::ChangeEvent> {
        if let Some(watcher) = self.watcher.take() {
            // Give the watcher some time to catch up.
            std::thread::sleep(Duration::from_millis(10));
            watcher.flush();
            watcher.stop();
        }

        let mut all_events = Vec::new();
        for events in &self.changes_receiver {
            all_events.extend(events);
        }

        all_events
    }

    fn collect_package_files(&self, path: &SystemPath) -> Vec<File> {
        let package = self.db().workspace().package(self.db(), path).unwrap();
        let files = package.files(self.db());
        let files = files.read();
        let mut collected: Vec<_> = files.into_iter().collect();
        collected.sort_unstable_by_key(|file| file.path(self.db()).as_system_path().unwrap());
        collected
    }

    fn system_file(&self, path: impl AsRef<SystemPath>) -> Option<File> {
        system_path_to_file(self.db(), path.as_ref())
    }
}

fn setup<I, P>(workspace_files: I) -> anyhow::Result<TestCase>
where
    I: IntoIterator<Item = (P, &'static str)>,
    P: AsRef<SystemPath>,
{
    let temp_dir = tempfile::tempdir()?;

    let workspace_path = temp_dir.path().join("workspace");

    std::fs::create_dir_all(&workspace_path).with_context(|| {
        format!(
            "Failed to create workspace directory '{}'",
            workspace_path.display()
        )
    })?;

    let workspace_path = SystemPath::from_std_path(&workspace_path).ok_or_else(|| {
        anyhow!(
            "Workspace root '{}' in temp directory is not a valid UTF-8 path.",
            workspace_path.display()
        )
    })?;

    let workspace_path = SystemPathBuf::from_utf8_path_buf(
        workspace_path
            .as_utf8_path()
            .canonicalize_utf8()
            .with_context(|| "Failed to canonzialize workspace path.")?,
    );

    for (relative_path, content) in workspace_files {
        let relative_path = relative_path.as_ref();
        let absolute_path = workspace_path.join(relative_path);
        if let Some(parent) = absolute_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create parent directory for file '{relative_path}'.",)
            })?;
        }

        std::fs::write(absolute_path.as_std_path(), content)
            .with_context(|| format!("Failed to write file '{relative_path}'"))?;
    }

    let system = OsSystem::new(&workspace_path);

    let workspace = WorkspaceMetadata::from_path(&workspace_path, &system)?;
    let settings = ProgramSettings {
        target_version: TargetVersion::default(),
        search_paths: SearchPathSettings {
            extra_paths: vec![],
            workspace_root: workspace.root().to_path_buf(),
            custom_typeshed: None,
            site_packages: None,
        },
    };

    let db = RootDatabase::new(workspace, settings, system);

    let (sender, receiver) = crossbeam::channel::unbounded();
    let mut watcher = directory_watcher(move |events| sender.send(events).unwrap())
        .with_context(|| "Failed to create directory watcher")?;

    watcher
        .watch(&workspace_path)
        .with_context(|| "Failed to set up watcher for workspace directory.")?;

    let test_case = TestCase {
        db,
        changes_receiver: receiver,
        watcher: Some(watcher),
        temp_dir,
    };

    Ok(test_case)
}

#[test]
fn new_file() -> anyhow::Result<()> {
    let mut case = setup([("bar.py", "")])?;
    let bar_path = case.workspace_path("bar.py");
    let bar_file = case.system_file(&bar_path).unwrap();
    let foo_path = case.workspace_path("foo.py");

    assert_eq!(case.system_file(&foo_path), None);
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

    assert_eq!(case.system_file(&foo_path), None);
    assert_eq!(&case.collect_package_files(&bar_path), &[bar_file]);

    std::fs::write(foo_path.as_std_path(), "print('Hello')")?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    assert!(case.system_file(&foo_path).is_some());
    assert_eq!(&case.collect_package_files(&bar_path), &[bar_file]);

    Ok(())
}

#[test]
fn changed_file() -> anyhow::Result<()> {
    let foo_source = "print('Hello, world!')";
    let mut case = setup([("foo.py", foo_source)])?;
    let foo_path = case.workspace_path("foo.py");

    let foo = case
        .system_file(&foo_path)
        .ok_or_else(|| anyhow!("Foo not found"))?;
    assert_eq!(source_text(case.db(), foo).as_str(), foo_source);
    assert_eq!(&case.collect_package_files(&foo_path), &[foo]);

    std::fs::write(foo_path.as_std_path(), "print('Version 2')")?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    assert_eq!(source_text(case.db(), foo).as_str(), "print('Version 2')");
    assert_eq!(&case.collect_package_files(&foo_path), &[foo]);

    Ok(())
}

#[cfg(unix)]
#[test]
fn changed_metadata() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut case = setup([("foo.py", "")])?;
    let foo_path = case.workspace_path("foo.py");

    let foo = case
        .system_file(&foo_path)
        .ok_or_else(|| anyhow!("Foo not found"))?;
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

#[test]
fn deleted_file() -> anyhow::Result<()> {
    let foo_source = "print('Hello, world!')";
    let mut case = setup([("foo.py", foo_source)])?;
    let foo_path = case.workspace_path("foo.py");

    let foo = case
        .system_file(&foo_path)
        .ok_or_else(|| anyhow!("Foo not found"))?;

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

    let foo = case
        .system_file(&foo_path)
        .ok_or_else(|| anyhow!("Foo not found"))?;

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

    assert!(case.system_file(&foo_path).is_some());
    assert_eq!(&case.collect_package_files(&bar_path), &[bar]);
    assert!(case
        .db()
        .workspace()
        .package(case.db(), &foo_path)
        .is_none());

    std::fs::rename(foo_path.as_std_path(), foo_in_workspace_path.as_std_path())?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    let foo_in_workspace = case
        .system_file(&foo_in_workspace_path)
        .ok_or_else(|| anyhow!("Foo not found"))?;

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

    let foo = case
        .system_file(&foo_path)
        .ok_or_else(|| anyhow!("Foo not found"))?;

    assert_eq!(case.collect_package_files(&foo_path), [foo]);

    std::fs::rename(foo_path.as_std_path(), bar_path.as_std_path())?;

    let changes = case.stop_watch();

    case.db_mut().apply_changes(changes);

    assert!(!foo.exists(case.db()));

    let bar = case
        .system_file(&bar_path)
        .ok_or_else(|| anyhow!("Bar not found"))?;

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

    assert!(resolve_module(case.db().upcast(), ModuleName::new_static("sub.a").unwrap()).is_some(),);

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
