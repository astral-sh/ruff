use crate::{
    location::CanLocate,
    semantic_index::{semantic_index, SemanticIndex},
    types::Type,
};
use lsp_types::Position;
use ruff_db::{
    files::File,
    parsed::parsed_module,
    source::{line_index, source_text},
};
use ruff_python_ast::Identifier;
use ruff_source_file::OneIndexed;

use crate::{
    location::{CPosition, DefLocation},
    Db,
};

pub fn definition_at_location(file: File, location: Position, db: &dyn Db) -> Option<DefLocation> {
    // XXX now this returns one or none. It could return an iterator of locations
    let index = semantic_index(db, file);
    // let's try and look up the relevant AST node
    let module = parsed_module(db.upcast(), file);

    // let's figure out the CPosition
    let source = source_text(db.upcast(), file);
    let li = line_index(db.upcast(), file);

    let text_size = li.offset(
        // XXX bad
        OneIndexed::from_zero_indexed(location.line as usize),
        OneIndexed::from_zero_indexed(location.character as usize),
        &source,
    );

    let cpos = CPosition(text_size.to_u32().into());
    eprintln!("Looking at offset {}", cpos.0);

    let found_dlike =
        module
            .syntax()
            .locate_def(&CPosition(text_size.to_u32().into()), index, db, file);
    eprintln!("FOUND DLIKE {found_dlike:?}");
    found_dlike
}

pub(crate) fn locate_name_on_type<'db>(
    db: &'db dyn Db,
    index: &SemanticIndex<'db>,
    typ: &Type<'db>,
    attr: &Identifier,
) -> Option<DefLocation> {
    let Some(def) = typ.member_def(db, &attr.id) else {
        return None;
    };
    Some(DefLocation::from_definition(def, index, db, def.file(db)))
}
