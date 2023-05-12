use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::cast;
use ruff_python_ast::helpers::identifier_range;
use ruff_python_semantic::analyze::visibility::{
    is_call, is_init, is_magic, is_new, is_overload, is_override, Visibility,
};
use ruff_python_semantic::definition::{Definition, Member, MemberKind, Module, ModuleKind};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

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
pub(crate) fn not_missing(
    checker: &mut Checker,
    definition: &Definition,
    visibility: Visibility,
) -> bool {
    if visibility.is_private() {
        return true;
    }

    match definition {
        Definition::Module(Module {
            kind: ModuleKind::Module,
            ..
        }) => {
            if checker
                .settings
                .rules
                .enabled(Rule::UndocumentedPublicModule)
            {
                checker.diagnostics.push(Diagnostic::new(
                    UndocumentedPublicModule,
                    TextRange::default(),
                ));
            }
            false
        }
        Definition::Module(Module {
            kind: ModuleKind::Package,
            ..
        }) => {
            if checker
                .settings
                .rules
                .enabled(Rule::UndocumentedPublicPackage)
            {
                checker.diagnostics.push(Diagnostic::new(
                    UndocumentedPublicPackage,
                    TextRange::default(),
                ));
            }
            false
        }
        Definition::Member(Member {
            kind: MemberKind::Class,
            stmt,
            ..
        }) => {
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
        Definition::Member(Member {
            kind: MemberKind::NestedClass,
            stmt,
            ..
        }) => {
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
        Definition::Member(Member {
            kind: MemberKind::Function | MemberKind::NestedFunction,
            stmt,
            ..
        }) => {
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
        Definition::Member(Member {
            kind: MemberKind::Method,
            stmt,
            ..
        }) => {
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
