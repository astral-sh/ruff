use ruff_diagnostics::DiagnosticKind;

use crate::registry::Rule;

use super::rules;

/// An AST node that can contain arguments.
#[derive(Copy, Clone)]
pub enum Argumentable {
    Function,
    Method,
    ClassMethod,
    StaticMethod,
    Lambda,
}

impl Argumentable {
    pub fn check_for(self, name: String) -> DiagnosticKind {
        match self {
            Self::Function => rules::UnusedFunctionArgument { name }.into(),
            Self::Method => rules::UnusedMethodArgument { name }.into(),
            Self::ClassMethod => rules::UnusedClassMethodArgument { name }.into(),
            Self::StaticMethod => rules::UnusedStaticMethodArgument { name }.into(),
            Self::Lambda => rules::UnusedLambdaArgument { name }.into(),
        }
    }

    pub const fn rule_code(self) -> Rule {
        match self {
            Self::Function => Rule::UnusedFunctionArgument,
            Self::Method => Rule::UnusedMethodArgument,
            Self::ClassMethod => Rule::UnusedClassMethodArgument,
            Self::StaticMethod => Rule::UnusedStaticMethodArgument,
            Self::Lambda => Rule::UnusedLambdaArgument,
        }
    }
}
