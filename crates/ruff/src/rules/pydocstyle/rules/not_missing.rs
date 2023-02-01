use crate::ast::cast;
use crate::ast::helpers::identifier_range;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{Definition, DefinitionKind};
use crate::message::Location;
use crate::registry::{Diagnostic, Rule};
use crate::violations;
use crate::visibility::{is_call, is_init, is_magic, is_new, is_overload, is_override, Visibility};

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
                    violations::PublicModule,
                    Range::new(Location::new(1, 0), Location::new(1, 0)),
                ));
            }
            false
        }
        DefinitionKind::Package => {
            if checker.settings.rules.enabled(&Rule::PublicPackage) {
                checker.diagnostics.push(Diagnostic::new(
                    violations::PublicPackage,
                    Range::new(Location::new(1, 0), Location::new(1, 0)),
                ));
            }
            false
        }
        DefinitionKind::Class(stmt) => {
            if checker.settings.rules.enabled(&Rule::PublicClass) {
                checker.diagnostics.push(Diagnostic::new(
                    violations::PublicClass,
                    identifier_range(stmt, checker.locator),
                ));
            }
            false
        }
        DefinitionKind::NestedClass(stmt) => {
            if checker.settings.rules.enabled(&Rule::PublicNestedClass) {
                checker.diagnostics.push(Diagnostic::new(
                    violations::PublicNestedClass,
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
                        violations::PublicFunction,
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
                        violations::PublicInit,
                        identifier_range(stmt, checker.locator),
                    ));
                }
                true
            } else if is_new(cast::name(stmt)) || is_call(cast::name(stmt)) {
                if checker.settings.rules.enabled(&Rule::PublicMethod) {
                    checker.diagnostics.push(Diagnostic::new(
                        violations::PublicMethod,
                        identifier_range(stmt, checker.locator),
                    ));
                }
                true
            } else if is_magic(cast::name(stmt)) {
                if checker.settings.rules.enabled(&Rule::MagicMethod) {
                    checker.diagnostics.push(Diagnostic::new(
                        violations::MagicMethod,
                        identifier_range(stmt, checker.locator),
                    ));
                }
                true
            } else {
                if checker.settings.rules.enabled(&Rule::PublicMethod) {
                    checker.diagnostics.push(Diagnostic::new(
                        violations::PublicMethod,
                        identifier_range(stmt, checker.locator),
                    ));
                }
                true
            }
        }
    }
}
