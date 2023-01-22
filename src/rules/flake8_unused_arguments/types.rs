use crate::registry::{DiagnosticKind, Rule};
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
