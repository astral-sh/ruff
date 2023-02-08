use std::path::Path;

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::{Binding, BindingKind, ExecutionContext};
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

/// Return `true` if `this` is implicitly loaded via importing `that`.
fn is_implicit_import(this: &Binding, that: &Binding) -> bool {
    match &this.kind {
        BindingKind::Importation(.., this_name)
        | BindingKind::SubmoduleImportation(this_name, ..) => match &that.kind {
            BindingKind::FromImportation(.., that_name) => {
                // Ex) `pkg.A` vs. `pkg`
                this_name
                    .rfind('.')
                    .map_or(false, |i| this_name[..i] == *that_name)
            }
            BindingKind::Importation(.., that_name)
            | BindingKind::SubmoduleImportation(that_name, ..) => {
                // Ex) `pkg.A` vs. `pkg.B`
                this_name == that_name
            }
            _ => false,
        },
        BindingKind::FromImportation(.., this_name) => match &that.kind {
            BindingKind::Importation(.., that_name)
            | BindingKind::SubmoduleImportation(that_name, ..) => {
                // Ex) `pkg.A` vs. `pkg`
                this_name
                    .rfind('.')
                    .map_or(false, |i| &this_name[..i] == *that_name)
            }
            BindingKind::FromImportation(.., that_name) => {
                // Ex) `pkg.A` vs. `pkg.B`
                this_name.rfind('.').map_or(false, |i| {
                    that_name
                        .rfind('.')
                        .map_or(false, |j| this_name[..i] == that_name[..j])
                })
            }
            _ => false,
        },
        _ => false,
    }
}

/// Return `true` if `name` is exempt from typing-only enforcement.
fn is_exempt(name: &str, exempt_modules: &[&str]) -> bool {
    let mut name = name;
    loop {
        if exempt_modules.contains(&name) {
            return true;
        }
        match name.rfind('.') {
            Some(idx) => {
                name = &name[..idx];
            }
            None => return false,
        }
    }
}

/// TCH001
pub fn typing_only_runtime_import(
    binding: &Binding,
    runtime_imports: &[&Binding],
    package: Option<&Path>,
    settings: &Settings,
) -> Option<Diagnostic> {
    // If we're in un-strict mode, don't flag typing-only imports that are
    // implicitly loaded by way of a valid runtime import.
    if !settings.flake8_type_checking.strict
        && runtime_imports
            .iter()
            .any(|import| is_implicit_import(binding, import))
    {
        return None;
    }

    let full_name = match &binding.kind {
        BindingKind::Importation(.., full_name) => full_name,
        BindingKind::FromImportation(.., full_name) => full_name.as_str(),
        BindingKind::SubmoduleImportation(.., full_name) => full_name,
        _ => return None,
    };

    if is_exempt(
        full_name,
        &settings
            .flake8_type_checking
            .exempt_modules
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    ) {
        return None;
    }

    if matches!(binding.context, ExecutionContext::Runtime)
        && binding.typing_usage.is_some()
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
            settings.target_version,
        ) {
            ImportType::LocalFolder | ImportType::FirstParty => Some(Diagnostic::new(
                TypingOnlyFirstPartyImport {
                    full_name: full_name.to_string(),
                },
                binding.range,
            )),
            ImportType::ThirdParty => Some(Diagnostic::new(
                TypingOnlyThirdPartyImport {
                    full_name: full_name.to_string(),
                },
                binding.range,
            )),
            ImportType::StandardLibrary => Some(Diagnostic::new(
                TypingOnlyStandardLibraryImport {
                    full_name: full_name.to_string(),
                },
                binding.range,
            )),
            ImportType::Future => unreachable!("`__future__` imports should be marked as used"),
        }
    } else {
        None
    }
}
