use std::sync::Arc;

use ruff_db::Db;
use ruff_db::files::File;
use ruff_db::source::source_text;
use ruff_db::system::SystemPathBuf;
use ruff_python_ast::script::ScriptTag;
use ruff_ranged_value::ValueSource;

use crate::metadata::pyproject::PyProject;

/// Returns the PEP 723 metadata embedded in `file`.
#[salsa::tracked(returns(ref))]
pub(crate) fn script_metadata(db: &dyn Db, file: File) -> Option<Box<PyProject>> {
    let path = file.path(db);
    if path.is_vendored_path() {
        return None;
    }

    let source = source_text(db, file);
    if source.is_notebook() {
        return None;
    }

    let tag = ScriptTag::parse(source.as_bytes())?;
    let value_source = ValueSource::File(Arc::new(SystemPathBuf::from(path.as_str())));

    PyProject::from_toml_str_without_spans(tag.metadata(), value_source)
        .map(Box::new)
        .ok()
}
