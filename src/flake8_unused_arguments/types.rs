use crate::checks::{CheckCode, CheckKind};

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
            Argumentable::Function => CheckKind::UnusedFunctionArgument(name),
            Argumentable::Method => CheckKind::UnusedMethodArgument(name),
            Argumentable::ClassMethod => CheckKind::UnusedClassMethodArgument(name),
            Argumentable::StaticMethod => CheckKind::UnusedStaticMethodArgument(name),
            Argumentable::Lambda => CheckKind::UnusedLambdaArgument(name),
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
