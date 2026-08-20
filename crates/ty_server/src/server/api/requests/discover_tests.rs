use lsp_types::{LspRequestMethod, MessageDirection, Range, Request, Uri};
use ruff_db::Db as _;
use ruff_db::files::{File, system_path_to_directory, system_path_to_file};
use ruff_db::system::{SystemPath, SystemPathBuf};
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use ty_ide::{DiscoveredTestKind, discover_tests};
use ty_project::{Db as _, ProjectDatabase, SemanticDb as _};

use crate::PositionEncoding;
use crate::document::ToRangeExt;
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::SessionSnapshot;
use crate::session::client::Client;
use crate::system::file_to_uri;

/// Custom `ty/discoverTests` request that lists the tests defined in a project, a
/// directory, or a single file.
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

/// A node in the discovered test tree: a directory, a file, a test class, or a test
/// function/method.
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

/// The kind of a [`TestItem`] in the discovered test tree.
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
                for file in db.project().files(db).iter().copied() {
                    append_file_tests(db, file, encoding, &mut test_items);
                }
            }
            tracing::debug!(
                "Discovered {} test items across all projects",
                test_items.len()
            );
            return Ok(DiscoverTestsResult::new(test_items));
        };

        let Ok(path) = uri.to_file_path() else {
            tracing::debug!("`{uri}` is not a file path; discovering nothing");
            return Ok(DiscoverTestsResult::new(vec![]));
        };
        let Ok(path) = SystemPathBuf::from_path_buf(path) else {
            tracing::debug!("`{uri}` is not valid UTF-8; discovering nothing");
            return Ok(DiscoverTestsResult::new(vec![]));
        };

        for db in snapshot.projects() {
            let project_root = db.project().root(db);
            if !project_includes_path(db, &path) {
                tracing::debug!("Project `{project_root}` has no `{path}`; skipping it");
                continue;
            }

            // NOTE: I cannot rely on file.status() here because FileStatus enum is private. Why is status() method available?
            if db.system().is_directory(&path) {
                tracing::debug!(
                    "Discovering every test under directory `{path}` of project `{project_root}`"
                );
                collect_directory_tests(db, &path, encoding, &mut test_items);
            } else {
                let Ok(file) = system_path_to_file(db, &path) else {
                    continue;
                };
                tracing::debug!(
                    "Discovering the tests in file `{path}` of project `{project_root}`"
                );
                append_file_tests(db, file, encoding, &mut test_items);
            }
        }

        tracing::debug!("Discovered {} test items for `{path}`", test_items.len());
        Ok(DiscoverTestsResult::new(test_items))
    }
}

impl RetriableRequestHandler for DiscoverTestsRequestHandler {}

/// Returns the last path component of `path`, or `path` itself if it has none.
fn path_label(path: &SystemPath) -> String {
    path.file_name()
        .map(ToString::to_string)
        .unwrap_or_else(|| path.to_string())
}

fn test_id(file_path: &str, qualified_name: &str) -> String {
    format!("{file_path}::{qualified_name}")
}

/// Returns whether `path` points at a file or directory that this project checks.
///
/// A relative `path` is resolved against the project root, so the same relative path can
/// belong to more than one open project. An absolute `path` keeps its own root and only
/// gets its `.` and `..` components normalized, which is what stops a `..` from walking
/// out of the project while still matching as if it were underneath it.
///
/// The path has to exist. Include patterns alone say nothing about whether a path is
/// really there, and both callers need a path they can hand to pytest or index as a file.
pub(super) fn project_includes_path(db: &ProjectDatabase, path: &SystemPath) -> bool {
    let project = db.project();
    let path = SystemPath::absolute(path, project.root(db));

    let Ok(metadata) = db.system().path_metadata(&path) else {
        return false;
    };

    if metadata.file_type().is_directory() {
        return project.is_directory_included(db, &path);
    }

    project.is_file_included(db, &path).is_included()
}

/// Appends tests for the given file to `items`, along with an item for each ancestor
/// directory between `file` and project root
/// Caller must make sure the file is within the project, otherwise the parent never finds root directory.
fn append_file_tests(
    db: &ProjectDatabase,
    file: File,
    encoding: PositionEncoding,
    items: &mut FxHashSet<TestItem>,
) {
    if !matches_pytest_naming_convention(db, file) {
        return;
    }
    let tests = discover_tests(db, db.program_file(file));
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
                DiscoveredTestKind::Class => TestItemKind::Class,
                DiscoveredTestKind::Function => TestItemKind::Function,
            },
            label: test.label,
            parent: Some(parent),
            range: Some(range.local_range()),
            uri: text_document.clone(),
        });
    }
}

fn collect_directory_tests(
    db: &ProjectDatabase,
    directory: &SystemPath,
    encoding: PositionEncoding,
    items: &mut FxHashSet<TestItem>,
) {
    for file in db.project().files(db).iter().copied() {
        let Some(path) = file.path(db).as_system_path() else {
            continue;
        };
        if path.starts_with(directory) {
            append_file_tests(db, file, encoding, items);
        }
    }
}

fn matches_pytest_naming_convention(db: &ProjectDatabase, file: File) -> bool {
    file.path(db)
        .as_system_path()
        .and_then(SystemPath::file_name)
        .and_then(|name| name.strip_suffix(".py"))
        .is_some_and(|stem| stem.starts_with("test_") || stem.ends_with("_test"))
}
