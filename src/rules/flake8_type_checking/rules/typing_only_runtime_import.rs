use std::path::Path;

use ruff_macros::derive_message_formats;
use rustpython_ast::Stmt;

use crate::ast::types::{Binding, BindingKind, Range};
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::rules::isort::{categorize, ImportType};
use crate::settings::Settings;
use crate::violation::Violation;

define_violation!(
    pub struct TypingOnlyFirstPartyImport {
        pub full_name: String,
    }
);
impl Violation for TypingOnlyFirstPartyImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move application import `{}` into a type-checking block",
            self.full_name
        )
    }
}

define_violation!(
    pub struct TypingOnlyThirdPartyImport {
        pub full_name: String,
    }
);
impl Violation for TypingOnlyThirdPartyImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move third-party import `{}` into a type-checking block",
            self.full_name
        )
    }
}

define_violation!(
    pub struct TypingOnlyStandardLibraryImport {
        pub full_name: String,
    }
);
impl Violation for TypingOnlyStandardLibraryImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move standard library import `{}` into a type-checking block",
            self.full_name
        )
    }
}

/// TCH001
pub fn typing_only_runtime_import(
    binding: &Binding,
    blocks: &[&Stmt],
    package: Option<&Path>,
    settings: &Settings,
) -> Option<Diagnostic> {
    let full_name = match &binding.kind {
        BindingKind::Importation(.., full_name) => full_name,
        BindingKind::FromImportation(.., full_name) => full_name.as_str(),
        BindingKind::SubmoduleImportation(.., full_name) => full_name,
        _ => return None,
    };

    let defined_in_type_checking = blocks
        .iter()
        .any(|block| Range::from_located(block).contains(&binding.range));
    if !defined_in_type_checking {
        if binding.typing_usage.is_some()
            && binding.runtime_usage.is_none()
            && binding.synthetic_usage.is_none()
        {
            // Extract the module base and level from the full name.
            // Ex) `foo.bar.baz` -> `foo`, `0`
            // Ex) `.foo.bar.baz` -> `foo`, `1`
            let module_base = full_name.split('.').next().unwrap();
            let level = full_name.chars().take_while(|c| *c == '.').count();

            // Categorize the import.
            match categorize(
                module_base,
                Some(&level),
                &settings.src,
                package,
                &settings.isort.known_first_party,
                &settings.isort.known_third_party,
                &settings.isort.extra_standard_library,
            ) {
                ImportType::LocalFolder | ImportType::FirstParty => {
                    return Some(Diagnostic::new(
                        TypingOnlyFirstPartyImport {
                            full_name: full_name.to_string(),
                        },
                        binding.range,
                    ));
                }
                ImportType::ThirdParty => {
                    return Some(Diagnostic::new(
                        TypingOnlyThirdPartyImport {
                            full_name: full_name.to_string(),
                        },
                        binding.range,
                    ));
                }
                ImportType::StandardLibrary => {
                    return Some(Diagnostic::new(
                        TypingOnlyStandardLibraryImport {
                            full_name: full_name.to_string(),
                        },
                        binding.range,
                    ));
                }
                ImportType::Future => unreachable!("`__future__` imports should be marked as used"),
            }
        }
    }

    None
}
