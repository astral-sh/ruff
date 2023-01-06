use crate::registry::{CheckCode, CheckKind};
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
    pub fn check_for(&self, name: String) -> CheckKind {
        match self {
            Argumentable::Function => violations::UnusedFunctionArgument(name),
            Argumentable::Method => violations::UnusedMethodArgument(name),
            Argumentable::ClassMethod => violations::UnusedClassMethodArgument(name),
            Argumentable::StaticMethod => violations::UnusedStaticMethodArgument(name),
            Argumentable::Lambda => violations::UnusedLambdaArgument(name),
        }
    }

    pub fn check_code(&self) -> &CheckCode {
        match self {
            Argumentable::Function => &CheckCode::ARG001,
            Argumentable::Method => &CheckCode::ARG002,
            Argumentable::ClassMethod => &CheckCode::ARG003,
            Argumentable::StaticMethod => &CheckCode::ARG004,
            Argumentable::Lambda => &CheckCode::ARG005,
        }
    }
}
