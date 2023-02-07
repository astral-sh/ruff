use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::cast;
use crate::ast::helpers::identifier_range;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{Definition, DefinitionKind};
use crate::message::Location;
use crate::registry::{Diagnostic, Rule};
use crate::violation::Violation;
use crate::visibility::{is_call, is_init, is_magic, is_new, is_overload, is_override, Visibility};

define_violation!(
    pub struct PublicModule;
);
impl Violation for PublicModule {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public module")
    }
}

define_violation!(
    pub struct PublicClass;
);
impl Violation for PublicClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public class")
    }
}

define_violation!(
    pub struct PublicMethod;
);
impl Violation for PublicMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public method")
    }
}

define_violation!(
    pub struct PublicFunction;
);
impl Violation for PublicFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public function")
    }
}

define_violation!(
    pub struct PublicPackage;
);
impl Violation for PublicPackage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public package")
    }
}

define_violation!(
    pub struct MagicMethod;
);
impl Violation for MagicMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in magic method")
    }
}

define_violation!(
    pub struct PublicNestedClass;
);
impl Violation for PublicNestedClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public nested class")
    }
}

define_violation!(
    pub struct PublicInit;
);
impl Violation for PublicInit {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in `__init__`")
    }
}

/// D100, D101, D102, D103, D104, D105, D106, D107
pub fn not_missing(
    checker: &mut Checker,
    definition: &Definition,
    visibility: &Visibility,
) -> bool {
    if matches!(visibility, Visibility::Private) {
        return true;
    }

    match definition.kind {
        DefinitionKind::Module => {
            if checker.settings.rules.enabled(&Rule::PublicModule) {
                checker.diagnostics.push(Diagnostic::new(
                    PublicModule,
                    Range::new(Location::new(1, 0), Location::new(1, 0)),
                ));
            }
            false
        }
        DefinitionKind::Package => {
            if checker.settings.rules.enabled(&Rule::PublicPackage) {
                checker.diagnostics.push(Diagnostic::new(
                    PublicPackage,
                    Range::new(Location::new(1, 0), Location::new(1, 0)),
                ));
            }
            false
        }
        DefinitionKind::Class(stmt) => {
            if checker.settings.rules.enabled(&Rule::PublicClass) {
                checker.diagnostics.push(Diagnostic::new(
                    PublicClass,
                    identifier_range(stmt, checker.locator),
                ));
            }
            false
        }
        DefinitionKind::NestedClass(stmt) => {
            if checker.settings.rules.enabled(&Rule::PublicNestedClass) {
                checker.diagnostics.push(Diagnostic::new(
                    PublicNestedClass,
                    identifier_range(stmt, checker.locator),
                ));
            }
            false
        }
        DefinitionKind::Function(stmt) | DefinitionKind::NestedFunction(stmt) => {
            if is_overload(checker, cast::decorator_list(stmt)) {
                true
            } else {
                if checker.settings.rules.enabled(&Rule::PublicFunction) {
                    checker.diagnostics.push(Diagnostic::new(
                        PublicFunction,
                        identifier_range(stmt, checker.locator),
                    ));
                }
                false
            }
        }
        DefinitionKind::Method(stmt) => {
            if is_overload(checker, cast::decorator_list(stmt))
                || is_override(checker, cast::decorator_list(stmt))
            {
                true
            } else if is_init(cast::name(stmt)) {
                if checker.settings.rules.enabled(&Rule::PublicInit) {
                    checker.diagnostics.push(Diagnostic::new(
                        PublicInit,
                        identifier_range(stmt, checker.locator),
                    ));
                }
                true
            } else if is_new(cast::name(stmt)) || is_call(cast::name(stmt)) {
                if checker.settings.rules.enabled(&Rule::PublicMethod) {
                    checker.diagnostics.push(Diagnostic::new(
                        PublicMethod,
                        identifier_range(stmt, checker.locator),
                    ));
                }
                true
            } else if is_magic(cast::name(stmt)) {
                if checker.settings.rules.enabled(&Rule::MagicMethod) {
                    checker.diagnostics.push(Diagnostic::new(
                        MagicMethod,
                        identifier_range(stmt, checker.locator),
                    ));
                }
                true
            } else {
                if checker.settings.rules.enabled(&Rule::PublicMethod) {
                    checker.diagnostics.push(Diagnostic::new(
                        PublicMethod,
                        identifier_range(stmt, checker.locator),
                    ));
                }
                true
            }
        }
    }
}
