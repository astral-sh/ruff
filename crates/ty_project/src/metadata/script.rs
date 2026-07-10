use std::sync::{Arc, LazyLock};

use memchr::memmem::Finder;
use ruff_db::Db;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_db::system::SystemPathBuf;
use ruff_python_ast::script::ScriptTag;
use ruff_python_ast::token::TokenKind;
use ruff_ranged_value::ValueSource;
use ruff_text_size::Ranged;

use crate::metadata::pyproject::PyProject;

const SCRIPT_TAG: &str = "# /// script";
static SCRIPT_TAG_FINDER: LazyLock<Finder<'static>> =
    LazyLock::new(|| Finder::new(SCRIPT_TAG.as_bytes()));

/// Returns the PEP 723 metadata embedded in `file`.
///
/// The byte search keeps the overwhelmingly common non-script path cheap. Parsing is only
/// necessary after finding a possible opening tag at the start of a line, where the token stream
/// disambiguates an actual comment from the same text inside a string literal.
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

    let source_bytes = source.as_bytes();
    let mut candidates = SCRIPT_TAG_FINDER
        .find_iter(source_bytes)
        .filter(|&offset| offset == 0 || matches!(source_bytes[offset - 1], b'\r' | b'\n'));
    let first_candidate = candidates.next()?;

    let parsed = parsed_module(db, file).load(db);
    let tokens = parsed.tokens();
    let tag = std::iter::once(first_candidate)
        .chain(candidates)
        .filter(|&offset| {
            let Ok(index) = tokens.binary_search_by_key(&offset, |token| token.start().to_usize())
            else {
                return false;
            };
            let token = &tokens[index];

            token.kind() == TokenKind::Comment && &source[token.range()] == SCRIPT_TAG
        })
        .find_map(|opening| ScriptTag::parse_at(source_bytes, opening))?;
    let value_source = ValueSource::File(Arc::new(SystemPathBuf::from(path.as_str())));
    let source_map = tag.metadata_source_map().clone();

    PyProject::from_toml_str_with_source_map(tag.metadata(), value_source, source_map)
        .map(Box::new)
        .ok()
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::{DbWithTestSystem, DbWithWritableSystem, SystemPath, SystemPathBuf};
    use ruff_python_ast::name::Name;
    use ruff_text_size::{TextRange, TextSize};

    use crate::ProjectMetadata;
    use crate::db::testing::TestDb;

    use super::script_metadata;

    fn setup_db() -> TestDb {
        let project =
            ProjectMetadata::new(Name::new_static("test"), SystemPathBuf::from("/project"));
        let mut db = TestDb::new(project);
        db.memory_file_system()
            .create_directory_all("/project")
            .unwrap();
        db.init_program().unwrap();
        db
    }

    fn write_file(db: &mut TestDb, path: &str, contents: &str) -> ruff_db::files::File {
        let path = SystemPath::new(path);
        db.write_file(path, contents).unwrap();
        system_path_to_file(db, path).unwrap()
    }

    #[test]
    fn maps_metadata_ranges_to_the_script() {
        let mut db = setup_db();
        let contents = "π = 1\r\n# /// script\r\n# [tool.ty.rules]\r\n# unknown-script-rule = \"warn\"\r\n# ///\r\n";
        let file = write_file(&mut db, "/project/script.py", contents);
        let metadata = script_metadata(&db, file).as_ref().unwrap();
        let rules = metadata
            .ty()
            .and_then(|options| options.rules.as_ref())
            .unwrap();
        let mut diagnostics = Vec::new();

        rules.to_rule_selection(&db, &mut diagnostics);

        let diagnostic = diagnostics.pop().unwrap().to_diagnostic();
        let range = diagnostic.expect_primary_span().range().unwrap();
        let start = contents.find("unknown-script-rule").unwrap();
        assert_eq!(
            range,
            TextRange::at(
                TextSize::try_from(start).unwrap(),
                TextSize::try_from("unknown-script-rule".len()).unwrap(),
            )
        );
    }
}
