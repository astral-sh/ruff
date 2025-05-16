use crate::semantic_index::definition::Definition;
use crate::{Db, Module};
use ruff_db::files::FileRange;
use ruff_db::source::source_text;
use ruff_text_size::{TextLen, TextRange};

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum TypeDefinition<'db> {
    Module(Module),
    Class(Definition<'db>),
    Function(Definition<'db>),
    TypeVar(Definition<'db>),
    TypeAlias(Definition<'db>),
}

impl TypeDefinition<'_> {
    pub fn focus_range(&self, db: &dyn Db) -> Option<FileRange> {
        match self {
            Self::Module(_) => None,
            Self::Class(definition)
            | Self::Function(definition)
            | Self::TypeVar(definition)
            | Self::TypeAlias(definition) => Some(definition.focus_range(db)),
        }
    }

    pub fn full_range(&self, db: &dyn Db) -> Option<FileRange> {
        match self {
            Self::Module(module) => {
                let file = module.file()?;
                let source = source_text(db.upcast(), file);
                Some(FileRange::new(file, TextRange::up_to(source.text_len())))
            }
            Self::Class(definition)
            | Self::Function(definition)
            | Self::TypeVar(definition)
            | Self::TypeAlias(definition) => Some(definition.full_range(db)),
        }
    }
}
