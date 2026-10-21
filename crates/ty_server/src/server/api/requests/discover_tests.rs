use std::sync::Mutex;

use lsp_types::{LspRequestMethod, MessageDirection, Range, Request, Uri};
use ruff_db::Db as _;
use ruff_db::files::{File, system_path_to_directory, system_path_to_file};
use ruff_db::system::walk_directory::WalkState;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use ty_ide::{DiscoveredPytestTestKind, discover_pytest_tests};
use ty_project::{Db as _, ProjectDatabase, SemanticDb as _};
use ty_python_semantic::is_default_pytest_test_file_name;

use crate::PositionEncoding;
use crate::document::ToRangeExt;
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::SessionSnapshot;
use crate::session::client::Client;
use crate::system::file_to_uri;

pub(in crate::server::api) enum DiscoverTestsRequest {}

impl Request for DiscoverTestsRequest {
    type Params = DiscoverTestsParams;
    type Result = DiscoverTestsResult;
    const METHOD: LspRequestMethod<'static> = LspRequestMethod::Custom("ty/discoverTests");
    const MESSAGE_DIRECTION: MessageDirection = MessageDirection::ClientToServer;
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::server::api) struct DiscoverTestsParams {
    /// The document to discover tests in or under.
    ///
    /// A URI pointing at a directory discovers every test under that directory. A URI
    /// pointing at a file discovers only the tests in that file. An absent `uri`
    /// discovers every test in every project in the session.
    ///
    /// Test files are found the way pytest finds them by default, not by the project's `include`
    /// and `exclude` settings, so files that are excluded from type checking are discovered too.
    /// A URI inside a directory that pytest skips by default, below the project root, gives an
    /// empty result. See
    /// <https://docs.pytest.org/en/stable/explanation/goodpractices.html#test-discovery>.
    #[serde(default)]
    uri: Option<Uri>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::server::api) struct DiscoverTestsResult {
    #[serde(default)]
    tests: Vec<TestItem>,
}

impl DiscoverTestsResult {
    fn new(tests: impl IntoIterator<Item = TestItem>) -> Self {
        let tests: Vec<TestItem> = tests.into_iter().collect();
        Self { tests }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub(in crate::server::api) struct TestItem {
    /// Pytest node id for this item
    id: String,
    /// The kind of this item.
    kind: TestItemKind,
    /// The display name of this item, e.g. `test_bar`.
    label: String,
    /// The id of the parent of this item, or `None` if it is a top-level item.
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
    /// The range of this item's name in its file, if it has one (directories don't).
    #[serde(skip_serializing_if = "Option::is_none")]
    range: Option<Range>,
    /// uri of the test item
    #[serde(skip_serializing_if = "Option::is_none")]
    uri: Option<Uri>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub(in crate::server::api) enum TestItemKind {
    Directory,
    File,
    Class,
    Function,
}
pub(crate) struct DiscoverTestsRequestHandler;

impl RequestHandler for DiscoverTestsRequestHandler {
    type RequestType = DiscoverTestsRequest;
}

impl BackgroundRequestHandler for DiscoverTestsRequestHandler {
    fn run(
        snapshot: &SessionSnapshot,
        _client: &Client,
        params: DiscoverTestsParams,
    ) -> crate::server::Result<DiscoverTestsResult> {
        match &params.uri {
            Some(uri) => tracing::debug!("Received `ty/discoverTests` for `{uri}`"),
            None => tracing::debug!("Received `ty/discoverTests` for every open project"),
        }

        let encoding = snapshot.position_encoding();
        let mut test_items = FxHashSet::default();

        let Some(uri) = params.uri else {
            for db in snapshot.projects() {
                collect_directory_tests(db, db.project().root(db), encoding, &mut test_items);
            }
            return Ok(DiscoverTestsResult::new(test_items));
        };

        let Ok(path) = uri.to_file_path() else {
            tracing::warn!("`{uri}` is not a file path.");
            return Ok(DiscoverTestsResult::new(vec![]));
        };
        let Ok(path) = SystemPathBuf::from_path_buf(path) else {
            tracing::warn!("`{uri}` is not valid UTF-8.");
            return Ok(DiscoverTestsResult::new(vec![]));
        };

        for db in snapshot.projects() {
            let project_root = db.project().root(db);
            if !path_is_in_project(db, project_root, &path) {
                tracing::debug!("`{path}` is not discoverable in `{project_root}`.");
                continue;
            }
            if db.system().is_directory(&path) {
                tracing::debug!(
                    "Discovering the tests under directory `{path}` of project `{project_root}`"
                );
                collect_directory_tests(db, &path, encoding, &mut test_items);
            } else {
                let Ok(file) = system_path_to_file(db, &path) else {
                    continue;
                };
                tracing::debug!(
                    "Discovering the tests in file `{path}` of project `{project_root}`"
                );
                collect_file_pytest_tests(db, file, encoding, &mut test_items);
            }
        }

        tracing::debug!("Discovered {} test items for `{path}`", test_items.len());
        Ok(DiscoverTestsResult::new(test_items))
    }
}

impl RetriableRequestHandler for DiscoverTestsRequestHandler {}

fn path_label(path: &SystemPath) -> String {
    path.file_name()
        .map(ToString::to_string)
        .unwrap_or_else(|| path.to_string())
}

fn test_id(file_path: &str, qualified_name: &str) -> String {
    format!("{file_path}::{qualified_name}")
}

/// Returns `true` if discovering the tests under `root` reaches `path`: `path` is inside
/// `root`, exists, and neither `path` nor one of its parent directories below `root` is a
/// directory that pytest skips by default.
pub(super) fn path_is_in_project(
    db: &ProjectDatabase,
    root: &SystemPath,
    path: &SystemPath,
) -> bool {
    let system = db.system();
    path.starts_with(root)
        && system.path_exists(path)
        && !path
            .ancestors()
            .take_while(|directory| *directory != root)
            .any(|directory| {
                system.is_directory(directory) && is_default_pytest_skipped_dir(system, directory)
            })
}

fn collect_directory_tests(
    db: &ProjectDatabase,
    directory: &SystemPath,
    encoding: PositionEncoding,
    items: &mut FxHashSet<TestItem>,
) {
    for path in walk_test_files(db, directory) {
        if let Ok(file) = system_path_to_file(db, &path) {
            collect_file_pytest_tests(db, file, encoding, items);
        }
    }
}

/// Returns the files under `directory` that follow pytest's default test file naming, without
/// descending into the directories that pytest skips by default. Like pytest, the walk ignores
/// the project's `include` and `exclude` settings and ignore files such as `.gitignore`. See
/// <https://docs.pytest.org/en/stable/explanation/goodpractices.html#conventions-for-python-test-discovery>.
fn walk_test_files(db: &ProjectDatabase, directory: &SystemPath) -> Vec<SystemPathBuf> {
    let system = db.system();
    let paths = Mutex::new(Vec::new());

    system
        .walk_directory(directory)
        .standard_filters(false)
        .run(|| {
            let paths = &paths;
            Box::new(move |entry| {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        tracing::debug!(
                            "Failed to walk a directory while discovering tests: {error}"
                        );
                        return WalkState::Continue;
                    }
                };

                if entry.depth() == 0 {
                    return WalkState::Continue;
                }

                if entry.file_type().is_directory()
                    && is_default_pytest_skipped_dir(system, entry.path())
                {
                    return WalkState::Skip;
                }

                if entry
                    .path()
                    .file_name()
                    .is_some_and(is_default_pytest_test_file_name)
                {
                    paths.lock().unwrap().push(entry.into_path());
                }

                WalkState::Continue
            })
        });

    paths.into_inner().unwrap()
}

/// Returns `true` if pytest doesn't collect tests under the directory at `path` by default.
///
/// These are the directories matching pytest's default `norecursedirs` patterns, plus
/// `__pycache__` and virtual environments, which pytest always skips:
/// <https://docs.pytest.org/en/stable/reference/reference.html#confval-norecursedirs>
/// <https://github.com/pytest-dev/pytest/blob/9.1.1/src/_pytest/main.py#L416-L472>
fn is_default_pytest_skipped_dir(system: &dyn System, path: &SystemPath) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };

    name.starts_with('.')
        || path.extension() == Some("egg")
        || matches!(
            name,
            "__pycache__"
                | "_darcs"
                | "build"
                | "CVS"
                | "dist"
                | "node_modules"
                | "venv"
                | "{arch}"
        )
        || system.is_file(&path.join("pyvenv.cfg"))
        || system.is_file(&path.join("conda-meta").join("history"))
}

/// Adds the pytest tests in `file`, and an item for each of its parent directories up to the
/// project root, which becomes a top-level item.
fn collect_file_pytest_tests(
    db: &ProjectDatabase,
    file: File,
    encoding: PositionEncoding,
    items: &mut FxHashSet<TestItem>,
) {
    let tests = discover_pytest_tests(db, db.program_file(file));
    let stop_at: &SystemPath = db.project().root(db);
    if tests.is_empty() {
        return;
    }

    let Some(absolute_path) = file.path(db).as_system_path() else {
        return;
    };

    let text_document = file_to_uri(db, file);

    let mut parent_dir = absolute_path.parent();
    while let Some(dir) = parent_dir {
        if system_path_to_directory(db, dir).is_err() {
            break;
        }

        let is_stop = dir == stop_at;
        let uri = Uri::from_directory_path(dir);
        debug_assert!(uri.is_ok(), "Parent directory must be convertible to Uri");
        items.insert(TestItem {
            id: dir.to_string(),
            kind: TestItemKind::Directory,
            label: path_label(dir),
            parent: if is_stop {
                None
            } else {
                dir.parent().map(ToString::to_string)
            },
            range: None,
            uri: uri.ok(),
        });

        if is_stop {
            break;
        }
        parent_dir = dir.parent();
    }

    items.insert(TestItem {
        id: absolute_path.to_string(),
        kind: TestItemKind::File,
        label: path_label(absolute_path),
        parent: absolute_path.parent().map(ToString::to_string),
        range: None,
        uri: text_document.clone(),
    });

    for test in tests {
        let Some(range) = test.range.to_lsp_range(db, file, encoding) else {
            continue;
        };

        let parent = match &test.parent {
            Some(class_name) => test_id(&absolute_path.to_string(), class_name),
            None => absolute_path.to_string(),
        };

        let id = test_id(&absolute_path.to_string(), &test.id);
        items.insert(TestItem {
            id,
            kind: match test.kind {
                DiscoveredPytestTestKind::Class => TestItemKind::Class,
                DiscoveredPytestTestKind::Function => TestItemKind::Function,
            },
            label: test.label,
            parent: Some(parent),
            range: Some(range.local_range()),
            uri: text_document.clone(),
        });
    }
}
