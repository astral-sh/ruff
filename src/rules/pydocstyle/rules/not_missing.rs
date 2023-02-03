use crate::ast::cast;
use crate::ast::helpers::identifier_range;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{Definition, DefinitionKind};
use crate::message::Location;
use crate::registry::{Diagnostic, Rule};
use crate::violation::Violation;

use crate::define_simple_violation;
use crate::visibility::{is_call, is_init, is_magic, is_new, is_overload, is_override, Visibility};
use ruff_macros::derive_message_formats;

define_simple_violation!(PublicModule, "Missing docstring in public module");

define_simple_violation!(PublicClass, "Missing docstring in public class");

define_simple_violation!(PublicMethod, "Missing docstring in public method");

define_simple_violation!(PublicFunction, "Missing docstring in public function");

define_simple_violation!(PublicPackage, "Missing docstring in public package");

define_simple_violation!(MagicMethod, "Missing docstring in magic method");

define_simple_violation!(
    PublicNestedClass,
    "Missing docstring in public nested class"
);

define_simple_violation!(PublicInit, "Missing docstring in `__init__`");

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
