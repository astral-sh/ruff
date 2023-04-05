use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::cast;
use ruff_python_ast::helpers::identifier_range;
use ruff_python_ast::types::Range;
use ruff_python_semantic::analyze::visibility::{
    is_call, is_init, is_magic, is_new, is_overload, is_override, Visibility,
};

use crate::checkers::ast::Checker;
use crate::docstrings::definition::{Definition, DefinitionKind};
use crate::message::Location;
use crate::registry::Rule;

#[violation]
pub struct UndocumentedPublicModule;

impl Violation for UndocumentedPublicModule {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public module")
    }
}

#[violation]
pub struct UndocumentedPublicClass;

impl Violation for UndocumentedPublicClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public class")
    }
}

#[violation]
pub struct UndocumentedPublicMethod;

impl Violation for UndocumentedPublicMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public method")
    }
}

#[violation]
pub struct UndocumentedPublicFunction;

impl Violation for UndocumentedPublicFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public function")
    }
}

#[violation]
pub struct UndocumentedPublicPackage;

impl Violation for UndocumentedPublicPackage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public package")
    }
}

#[violation]
pub struct UndocumentedMagicMethod;

impl Violation for UndocumentedMagicMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in magic method")
    }
}

#[violation]
pub struct UndocumentedPublicNestedClass;

impl Violation for UndocumentedPublicNestedClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public nested class")
    }
}

#[violation]
pub struct UndocumentedPublicInit;

impl Violation for UndocumentedPublicInit {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in `__init__`")
    }
}

/// D100, D101, D102, D103, D104, D105, D106, D107
pub fn not_missing(checker: &mut Checker, definition: &Definition, visibility: Visibility) -> bool {
    if matches!(visibility, Visibility::Private) {
        return true;
    }

    match definition.kind {
        DefinitionKind::Module => {
            if checker
                .settings
                .rules
                .enabled(Rule::UndocumentedPublicModule)
            {
                checker.diagnostics.push(Diagnostic::new(
                    UndocumentedPublicModule,
                    Range::new(Location::new(1, 0), Location::new(1, 0)),
                ));
            }
            false
        }
        DefinitionKind::Package => {
            if checker
                .settings
                .rules
                .enabled(Rule::UndocumentedPublicPackage)
            {
                checker.diagnostics.push(Diagnostic::new(
                    UndocumentedPublicPackage,
                    Range::new(Location::new(1, 0), Location::new(1, 0)),
                ));
            }
            false
        }
        DefinitionKind::Class(stmt) => {
            if checker
                .settings
                .rules
                .enabled(Rule::UndocumentedPublicClass)
            {
                checker.diagnostics.push(Diagnostic::new(
                    UndocumentedPublicClass,
                    identifier_range(stmt, checker.locator),
                ));
            }
            false
        }
        DefinitionKind::NestedClass(stmt) => {
            if checker
                .settings
                .rules
                .enabled(Rule::UndocumentedPublicNestedClass)
            {
                checker.diagnostics.push(Diagnostic::new(
                    UndocumentedPublicNestedClass,
                    identifier_range(stmt, checker.locator),
                ));
            }
            false
        }
        DefinitionKind::Function(stmt) | DefinitionKind::NestedFunction(stmt) => {
            if is_overload(&checker.ctx, cast::decorator_list(stmt)) {
                true
            } else {
                if checker
                    .settings
                    .rules
                    .enabled(Rule::UndocumentedPublicFunction)
                {
                    checker.diagnostics.push(Diagnostic::new(
                        UndocumentedPublicFunction,
                        identifier_range(stmt, checker.locator),
                    ));
                }
                false
            }
        }
        DefinitionKind::Method(stmt) => {
            if is_overload(&checker.ctx, cast::decorator_list(stmt))
                || is_override(&checker.ctx, cast::decorator_list(stmt))
            {
                true
            } else if is_init(cast::name(stmt)) {
                if checker.settings.rules.enabled(Rule::UndocumentedPublicInit) {
                    checker.diagnostics.push(Diagnostic::new(
                        UndocumentedPublicInit,
                        identifier_range(stmt, checker.locator),
                    ));
                }
                true
            } else if is_new(cast::name(stmt)) || is_call(cast::name(stmt)) {
                if checker
                    .settings
                    .rules
                    .enabled(Rule::UndocumentedPublicMethod)
                {
                    checker.diagnostics.push(Diagnostic::new(
                        UndocumentedPublicMethod,
                        identifier_range(stmt, checker.locator),
                    ));
                }
                true
            } else if is_magic(cast::name(stmt)) {
                if checker
                    .settings
                    .rules
                    .enabled(Rule::UndocumentedMagicMethod)
                {
                    checker.diagnostics.push(Diagnostic::new(
                        UndocumentedMagicMethod,
                        identifier_range(stmt, checker.locator),
                    ));
                }
                true
            } else {
                if checker
                    .settings
                    .rules
                    .enabled(Rule::UndocumentedPublicMethod)
                {
                    checker.diagnostics.push(Diagnostic::new(
                        UndocumentedPublicMethod,
                        identifier_range(stmt, checker.locator),
                    ));
                }
                true
            }
        }
    }
}
