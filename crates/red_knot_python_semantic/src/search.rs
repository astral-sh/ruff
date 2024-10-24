use crate::{
    location::{location_from_definition, CanLocate},
    semantic_index::{semantic_index, SemanticIndex},
    types::Type,
};
use ruff_db::{
    files::{location::Location, File},
    parsed::parsed_module,
    source::{line_index, source_text},
};
use ruff_python_ast::Identifier;
use ruff_source_file::SourceLocation;

use crate::Db;

pub fn location_of_definition_of_item_at_location(
    file: File,
    location: &SourceLocation,
    db: &dyn Db,
) -> Option<Location> {
    // XXX now this returns one or none. It could return an iterator of locations
    let index = semantic_index(db, file);
    // let's try and look up the relevant AST node
    let module = parsed_module(db.upcast(), file);

    let source = source_text(db.upcast(), file);
    let li = line_index(db.upcast(), file);

    let text_size = li.offset(location.row, location.column, &source);

    return module.syntax().locate_def(text_size, index, db, file);
}

pub(crate) fn locate_name_on_type<'db>(
    db: &'db dyn Db,
    index: &SemanticIndex<'db>,
    typ: &Type<'db>,
    attr: &Identifier,
) -> Option<Location> {
    let def = typ.member_def(db, &attr.id)?;
    Some(location_from_definition(def, index, db))
}
