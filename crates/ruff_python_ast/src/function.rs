use crate::node::AnyNodeRef;
use ruff_text_size::TextRange;
use rustpython_ast::{
    Arguments, Decorator, Expr, Identifier, Ranged, StmtAsyncFunctionDef, StmtFunctionDef, Suite,
};

/// Enum that represents any python function definition.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum AnyFunctionDefinition<'a> {
    FunctionDefinition(&'a StmtFunctionDef),
    AsyncFunctionDefinition(&'a StmtAsyncFunctionDef),
}

impl<'a> AnyFunctionDefinition<'a> {
    pub const fn cast_ref(reference: AnyNodeRef<'a>) -> Option<Self> {
        match reference {
            AnyNodeRef::StmtAsyncFunctionDef(definition) => {
                Some(Self::AsyncFunctionDefinition(definition))
            }
            AnyNodeRef::StmtFunctionDef(definition) => Some(Self::FunctionDefinition(definition)),
            _ => None,
        }
    }

    /// Returns `Some` if this is a [`StmtFunctionDef`] and `None` otherwise.
    pub const fn as_function_definition(self) -> Option<&'a StmtFunctionDef> {
        if let Self::FunctionDefinition(definition) = self {
            Some(definition)
        } else {
            None
        }
    }

    /// Returns `Some` if this is a [`StmtAsyncFunctionDef`] and `None` otherwise.
    pub const fn as_async_function_definition(self) -> Option<&'a StmtAsyncFunctionDef> {
        if let Self::AsyncFunctionDefinition(definition) = self {
            Some(definition)
        } else {
            None
        }
    }

    /// Returns the function's name
    pub const fn name(self) -> &'a Identifier {
        match self {
            Self::FunctionDefinition(definition) => &definition.name,
            Self::AsyncFunctionDefinition(definition) => &definition.name,
        }
    }

    /// Returns the function arguments (parameters).
    pub fn arguments(self) -> &'a Arguments {
        match self {
            Self::FunctionDefinition(definition) => definition.args.as_ref(),
            Self::AsyncFunctionDefinition(definition) => definition.args.as_ref(),
        }
    }

    /// Returns the function's body
    pub const fn body(self) -> &'a Suite {
        match self {
            Self::FunctionDefinition(definition) => &definition.body,
            Self::AsyncFunctionDefinition(definition) => &definition.body,
        }
    }

    /// Returns the decorators attributing the function.
    pub fn decorators(self) -> &'a [Decorator] {
        match self {
            Self::FunctionDefinition(definition) => &definition.decorator_list,
            Self::AsyncFunctionDefinition(definition) => &definition.decorator_list,
        }
    }

    pub fn returns(self) -> Option<&'a Expr> {
        match self {
            Self::FunctionDefinition(definition) => definition.returns.as_deref(),
            Self::AsyncFunctionDefinition(definition) => definition.returns.as_deref(),
        }
    }

    pub fn type_comments(self) -> Option<&'a str> {
        match self {
            Self::FunctionDefinition(definition) => definition.type_comment.as_deref(),
            Self::AsyncFunctionDefinition(definition) => definition.type_comment.as_deref(),
        }
    }

    /// Returns `true` if this is [`Self::AsyncFunctionDefinition`]
    pub const fn is_async(self) -> bool {
        matches!(self, Self::AsyncFunctionDefinition(_))
    }
}

impl Ranged for AnyFunctionDefinition<'_> {
    fn range(&self) -> TextRange {
        match self {
            AnyFunctionDefinition::FunctionDefinition(definition) => definition.range(),
            AnyFunctionDefinition::AsyncFunctionDefinition(definition) => definition.range(),
        }
    }
}

impl<'a> From<&'a StmtFunctionDef> for AnyFunctionDefinition<'a> {
    fn from(value: &'a StmtFunctionDef) -> Self {
        Self::FunctionDefinition(value)
    }
}

impl<'a> From<&'a StmtAsyncFunctionDef> for AnyFunctionDefinition<'a> {
    fn from(value: &'a StmtAsyncFunctionDef) -> Self {
        Self::AsyncFunctionDefinition(value)
    }
}

impl<'a> From<AnyFunctionDefinition<'a>> for AnyNodeRef<'a> {
    fn from(value: AnyFunctionDefinition<'a>) -> Self {
        match value {
            AnyFunctionDefinition::FunctionDefinition(function_def) => {
                AnyNodeRef::StmtFunctionDef(function_def)
            }
            AnyFunctionDefinition::AsyncFunctionDefinition(async_def) => {
                AnyNodeRef::StmtAsyncFunctionDef(async_def)
            }
        }
    }
}

impl<'a> From<&'a AnyFunctionDefinition<'a>> for AnyNodeRef<'a> {
    fn from(value: &'a AnyFunctionDefinition<'a>) -> Self {
        (*value).into()
    }
}
