use std::fmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{Binding, BindingKind};
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for the use of non-ASCII characters in variable names.
///
/// ## Why is this bad?
/// The use of non-ASCII characters in variable names can cause confusion
/// and compatibility issues (see: [PEP 672]).
///
/// ## Example
/// ```python
/// ápple_count: int
/// ```
///
/// Use instead:
/// ```python
/// apple_count: int
/// ```
///
/// [PEP 672]: https://peps.python.org/pep-0672/
#[violation]
pub struct NonAsciiName {
    name: String,
    kind: Kind,
}

impl Violation for NonAsciiName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { name, kind } = self;
        format!("{kind} name `{name}` contains a non-ASCII character")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Rename the variable using ASCII characters".to_string())
    }
}

/// PLC2401
pub(crate) fn non_ascii_name(binding: &Binding, locator: &Locator) -> Option<Diagnostic> {
    let name = binding.name(locator);
    if name.is_ascii() {
        return None;
    }

    let kind = match binding.kind {
        BindingKind::Annotation => Kind::Annotation,
        BindingKind::Argument => Kind::Argument,
        BindingKind::NamedExprAssignment => Kind::NamedExprAssignment,
        BindingKind::Assignment => Kind::Assignment,
        BindingKind::TypeParam => Kind::TypeParam,
        BindingKind::LoopVar => Kind::LoopVar,
        BindingKind::WithItemVar => Kind::WithItemVar,
        BindingKind::Global(_) => Kind::Global,
        BindingKind::Nonlocal(_, _) => Kind::Nonlocal,
        BindingKind::ClassDefinition(_) => Kind::ClassDefinition,
        BindingKind::FunctionDefinition(_) => Kind::FunctionDefinition,
        BindingKind::BoundException => Kind::BoundException,

        BindingKind::Builtin
        | BindingKind::Export(_)
        | BindingKind::FutureImport
        | BindingKind::Import(_)
        | BindingKind::FromImport(_)
        | BindingKind::SubmoduleImport(_)
        | BindingKind::Deletion
        | BindingKind::ConditionalDeletion(_)
        | BindingKind::UnboundException(_) => {
            return None;
        }
    };

    Some(Diagnostic::new(
        NonAsciiName {
            name: name.to_string(),
            kind,
        },
        binding.range(),
    ))
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum Kind {
    Annotation,
    Argument,
    NamedExprAssignment,
    Assignment,
    TypeParam,
    LoopVar,
    WithItemVar,
    Global,
    Nonlocal,
    ClassDefinition,
    FunctionDefinition,
    BoundException,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Annotation => f.write_str("Annotation"),
            Kind::Argument => f.write_str("Argument"),
            Kind::NamedExprAssignment => f.write_str("Variable"),
            Kind::Assignment => f.write_str("Variable"),
            Kind::TypeParam => f.write_str("Type parameter"),
            Kind::LoopVar => f.write_str("Variable"),
            Kind::WithItemVar => f.write_str("Variable"),
            Kind::Global => f.write_str("Global"),
            Kind::Nonlocal => f.write_str("Nonlocal"),
            Kind::ClassDefinition => f.write_str("Class"),
            Kind::FunctionDefinition => f.write_str("Function"),
            Kind::BoundException => f.write_str("Exception"),
        }
    }
}
