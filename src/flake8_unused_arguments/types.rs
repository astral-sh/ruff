use crate::registry::{DiagnosticCode, DiagnosticKind};
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

    pub fn check_code(&self) -> &DiagnosticCode {
        match self {
            Argumentable::Function => &DiagnosticCode::ARG001,
            Argumentable::Method => &DiagnosticCode::ARG002,
            Argumentable::ClassMethod => &DiagnosticCode::ARG003,
            Argumentable::StaticMethod => &DiagnosticCode::ARG004,
            Argumentable::Lambda => &DiagnosticCode::ARG005,
        }
    }
}
