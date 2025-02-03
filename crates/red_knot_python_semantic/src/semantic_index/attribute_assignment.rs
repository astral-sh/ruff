use crate::semantic_index::expression::Expression;

use ruff_python_ast::name::Name;

use rustc_hash::FxHashMap;

/// Describes an (annotated) attribute assignment that we discovered in a method
/// body, typically of the form `self.x: int`, `self.x: int = …` or `self.x = …`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttributeAssignment<'db> {
    /// An attribute assignment with an explicit type annotation, either
    /// `self.x: <annotation>` or `self.x: <annotation> = …`.
    Annotated { annotation: Expression<'db> },

    /// An attribute assignment without a type annotation, e.g. `self.x = <value>`.
    Unannotated { value: Expression<'db> },
}

pub(crate) type AttributeAssignments<'db> = FxHashMap<Name, Vec<AttributeAssignment<'db>>>;
