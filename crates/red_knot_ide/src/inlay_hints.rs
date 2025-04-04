use crate::{Db, RangedValue};
use red_knot_python_semantic::types::{get_types, Type};
use ruff_db::files::File;
use std::fmt;
use std::fmt::Formatter;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum InlayHintContent<'db> {
    Type(Type<'db>),
}

impl<'db> InlayHintContent<'db> {
    pub const fn display(&self, db: &'db dyn Db) -> DisplayInlayHint<'_, 'db> {
        DisplayInlayHint { db, hint: self }
    }

    pub(crate) fn maybe_from_type(ty: Type<'db>) -> Option<Self> {
        // TODO: Create proper filtering
        match ty {
            Type::ModuleLiteral(_)
            | Type::Dynamic(_)
            | Type::Never
            | Type::FunctionLiteral(_)
            | Type::BoundMethod(_)
            | Type::MethodWrapper(_)
            | Type::WrapperDescriptor(_)
            | Type::Callable(_)
            | Type::ClassLiteral(_)
            | Type::PropertyInstance(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeVar(_) => None,
            Type::IntLiteral(_)
            | Type::SubclassOf(_)
            | Type::KnownInstance(_)
            | Type::Union(_)
            | Type::Intersection(_)
            | Type::Instance(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::BytesLiteral(_)
            | Type::SliceLiteral(_)
            | Type::Tuple(_) => Some(InlayHintContent::Type(ty)),
        }
    }
}

pub struct DisplayInlayHint<'a, 'db> {
    db: &'db dyn Db,
    hint: &'a InlayHintContent<'db>,
}

impl fmt::Display for DisplayInlayHint<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.hint {
            InlayHintContent::Type(ty) => write!(f, ": {}", ty.display(self.db.upcast())),
        }
    }
}

pub fn get_inlay_hints(db: &dyn Db, file: File) -> Vec<RangedValue<InlayHintContent>> {
    let types = get_types(db.upcast(), file);

    let hints = types
        .iter()
        .filter_map(|(definition, type_and_qualifiers)| {
            InlayHintContent::maybe_from_type(*type_and_qualifiers).map(|hint| RangedValue {
                range: definition.focus_range(db.upcast()),
                value: hint,
            })
        });

    hints.collect()
}
