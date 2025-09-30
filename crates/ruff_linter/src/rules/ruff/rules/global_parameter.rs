use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;

/// ## What it does
/// Flags function parameters that are also declared as `global`.
///
/// ## Why is this bad?
/// Declaring a parameter as `global` makes no sense: the parameter name
/// is already bound in the local scope, and using it as a global introduces
/// ambiguity and potential runtime errors.
///
/// ## Example
///
/// ```python
/// def f(a):
///     global a  # error
///
/// def g(a):
///     if True:
///         global a  # error
/// ```

#[derive(ViolationMetadata)]
pub(crate) struct GlobalParameter {
    pub(crate) name: String,
}

impl Violation for GlobalParameter {
    #[derive_message_formats]
    fn message(&self) -> String {
        let GlobalParameter { name } = self;
        format!("name `{name}` is parameter and global")
    }
}
