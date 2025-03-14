use crate::{
    semantic_index::{ast_ids::ScopedExpressionId, expression::Expression},
    unpack::Unpack,
};

use ruff_python_ast::name::Name;

use rustc_hash::FxHashMap;

/// Describes an (annotated) attribute assignment that we discovered in a method
/// body, typically of the form `self.x: int`, `self.x: int = …` or `self.x = …`.
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub(crate) enum AttributeAssignment<'db> {
    /// An attribute assignment with an explicit type annotation, either
    /// `self.x: <annotation>` or `self.x: <annotation> = …`.
    Annotated { annotation: Expression<'db> },

    /// An attribute assignment without a type annotation, e.g. `self.x = <value>`.
    Unannotated { value: Expression<'db> },

    /// An attribute assignment where the right-hand side is an iterable, for example
    /// `for self.x in <iterable>`.
    Iterable { iterable: Expression<'db> },

    /// An attribute assignment where the expression to be assigned is a context manager, for example
    /// `with <context_manager> as self.x`.
    ContextManager { context_manager: Expression<'db> },

    /// An attribute assignment where the left-hand side is an unpacking expression,
    /// e.g. `self.x, self.y = <value>`.
    Unpack {
        attribute_expression_id: ScopedExpressionId,
        unpack: Unpack<'db>,
    },
}

pub(crate) type AttributeAssignments<'db> = FxHashMap<Name, Vec<AttributeAssignment<'db>>>;
