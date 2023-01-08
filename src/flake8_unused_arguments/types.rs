use crate::registry::{DiagnosticKind, RuleCode};
use crate::violations;

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
            Argumentable::Function => violations::UnusedFunctionArgument(name).into(),
            Argumentable::Method => violations::UnusedMethodArgument(name).into(),
            Argumentable::ClassMethod => violations::UnusedClassMethodArgument(name).into(),
            Argumentable::StaticMethod => violations::UnusedStaticMethodArgument(name).into(),
            Argumentable::Lambda => violations::UnusedLambdaArgument(name).into(),
        }
    }

    pub fn rule_code(&self) -> &RuleCode {
        match self {
            Argumentable::Function => &RuleCode::ARG001,
            Argumentable::Method => &RuleCode::ARG002,
            Argumentable::ClassMethod => &RuleCode::ARG003,
            Argumentable::StaticMethod => &RuleCode::ARG004,
            Argumentable::Lambda => &RuleCode::ARG005,
        }
    }
}
