use super::rules;
use crate::registry::{DiagnosticKind, Rule};

/// An AST node that can contain arguments.
pub enum Argumentable {
    Function,
    Method,
    ClassMethod,
    StaticMethod,
    Lambda,
}

impl Argumentable {
    pub fn check_for(&self, name: String) -> DiagnosticKind {
        match self {
            Argumentable::Function => rules::UnusedFunctionArgument { name }.into(),
            Argumentable::Method => rules::UnusedMethodArgument { name }.into(),
            Argumentable::ClassMethod => rules::UnusedClassMethodArgument { name }.into(),
            Argumentable::StaticMethod => rules::UnusedStaticMethodArgument { name }.into(),
            Argumentable::Lambda => rules::UnusedLambdaArgument { name }.into(),
        }
    }

    pub fn rule_code(&self) -> &Rule {
        match self {
            Argumentable::Function => &Rule::UnusedFunctionArgument,
            Argumentable::Method => &Rule::UnusedMethodArgument,
            Argumentable::ClassMethod => &Rule::UnusedClassMethodArgument,
            Argumentable::StaticMethod => &Rule::UnusedStaticMethodArgument,
            Argumentable::Lambda => &Rule::UnusedLambdaArgument,
        }
    }
}
