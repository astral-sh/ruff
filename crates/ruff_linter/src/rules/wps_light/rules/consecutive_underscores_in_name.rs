use std::fmt;

use itertools::Itertools;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{Binding, BindingKind};
use ruff_text_size::Ranged;

use crate::Locator;

/// ## What it does
/// Checks for consecutive underscores in name
/// (variables, attributes, functions, and methods)
///
/// ## Why is this bad?
/// More consecutive underscoress lowers readability.
///
/// ## Example
/// ```python
/// long___variable__name: int = 3
/// ```
///
/// Use instead:
/// ```python
/// long_variable_name: int = 3
/// ```
#[violation]
pub struct ConsecutiveUnderscoresInName {
    name: String,
    replacement: String,
    kind: Kind,
}

impl Violation for ConsecutiveUnderscoresInName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self {
            name,
            replacement: _,
            kind,
        } = self;
        format!("{kind} name {name} contains consecutive underscors inside.")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Rename variable to {}", self.replacement))
    }
}

/// WPS116
pub(crate) fn consecutive_underscores_in_name(
    locator: &Locator,
    binding: &Binding,
) -> Option<Diagnostic> {
    let name = binding.name(locator.contents());
    if !name.len() < 3 || !name.contains("__") {
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

    let prefix_under = name.chars().take_while(|&c| c == '_').count();
    let suffix_under = name.chars().rev().take_while(|&c| c == '_').count();
    let trimmed = &name[prefix_under..name.len() - suffix_under];

    if !trimmed.contains("__") {
        return None;
    }

    let mut replacement = String::with_capacity(name.len());
    replacement.push_str(&"_".repeat(prefix_under));
    replacement.push_str(&trimmed.split('_').filter(|part| !part.is_empty()).join("_"));
    replacement.push_str(&"_".repeat(suffix_under));

    Some(Diagnostic::new(
        ConsecutiveUnderscoresInName {
            name: name.to_string(),
            replacement: replacement.to_string(),
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
