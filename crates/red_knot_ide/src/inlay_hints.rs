use crate::{Db, RangedValue};
use red_knot_python_semantic::types::{get_types, Type};
use ruff_db::files::File;
use ruff_db::source::source_text;
use ruff_text_size::Ranged;
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

    let source = source_text(db.upcast(), file);

    let source_len = source.len();

    let hints = types
        .iter()
        .map(|(definition, type_and_qualifiers)| RangedValue {
            range: definition.focus_range(db.upcast()),
            value: InlayHintContent::Type(*type_and_qualifiers),
        })
        .filter(|hint| filter_hint(hint, &source, source_len))
        .collect();

    hints
}

fn has_type_annotation(
    hint: &RangedValue<InlayHintContent>,
    source: &str,
    source_len: usize,
) -> bool {
    let end_offset = hint.range.range().end().to_usize();

    let mut current_char_offset = end_offset;

    while current_char_offset < source_len
        && source[current_char_offset..].starts_with(|c: char| c.is_whitespace())
    {
        current_char_offset += 1;
    }

    current_char_offset < source_len && source[current_char_offset..].starts_with(':')
}

fn filter_type(ty: &InlayHintContent) -> bool {
    match ty {
        InlayHintContent::Type(ty) => match ty {
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
            | Type::TypeVar(_) => false,
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
            | Type::Tuple(_) => true,
        },
    }
}

fn filter_hint(hint: &RangedValue<InlayHintContent>, source: &str, source_len: usize) -> bool {
    if has_type_annotation(hint, source, source_len) {
        return false;
    }

    filter_type(&hint.value)
}
