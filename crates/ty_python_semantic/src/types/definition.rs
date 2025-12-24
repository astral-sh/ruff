use crate::Db;
use crate::semantic_index::definition::Definition;
use ruff_db::files::FileRange;
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_text_size::{TextLen, TextRange};
use ty_module_resolver::Module;

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum TypeDefinition<'db> {
    Module(Module<'db>),
    Class(Definition<'db>),
    Function(Definition<'db>),
    TypeVar(Definition<'db>),
    TypeAlias(Definition<'db>),
    NewType(Definition<'db>),
    SpecialForm(Definition<'db>),
}

impl TypeDefinition<'_> {
    pub fn focus_range(&self, db: &dyn Db) -> Option<FileRange> {
        match self {
            Self::Module(_) => None,
            Self::Class(definition)
            | Self::Function(definition)
            | Self::TypeVar(definition)
            | Self::TypeAlias(definition)
            | Self::SpecialForm(definition)
            | Self::NewType(definition) => {
                let module = parsed_module(db, definition.file(db)).load(db);
                Some(definition.focus_range(db, &module))
            }
        }
    }

    pub fn full_range(&self, db: &dyn Db) -> Option<FileRange> {
        match self {
            Self::Module(module) => {
                let file = module.file(db)?;
                let source = source_text(db, file);
                Some(FileRange::new(file, TextRange::up_to(source.text_len())))
            }
            Self::Class(definition)
            | Self::Function(definition)
            | Self::TypeVar(definition)
            | Self::TypeAlias(definition)
            | Self::SpecialForm(definition)
            | Self::NewType(definition) => {
                let module = parsed_module(db, definition.file(db)).load(db);
                Some(definition.full_range(db, &module))
            }
        }
    }
}
