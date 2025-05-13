use ruff_diagnostics::{Diagnostic, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};

use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use crate::rules::sqlalchemy::helpers;
use ruff_python_semantic::Modules;

/// ## What it does
/// Checks for existence of `Mapped` or other ORM container class type annotations in SQLAlchemy
/// models.
///
/// ## Why is this bad?
/// If an annotation is missing, type checkers will treat the corresponding field as type `Any`.
///
/// ## Example
/// ```python
/// from sqlalchemy import Integer
/// from sqlalchemy.orm import DeclarativeBase
/// from sqlalchemy.orm import Mapped
/// from sqlalchemy.orm import mapped_column
///
/// class Base(DeclarativeBase):
///     pass
///
///
/// class MyModel(Base):
///     __tablename__ = "my_model"
///     id: Mapped[int] = mapped_column(primary_key=True)
///
///     count = mapped_column(Integer)
///
///
/// m = MyModel()
/// reveal_type(m.count)  #  note: Revealed type is "Any"
/// ```
///
/// Use instead:
/// ```python
/// from sqlalchemy.orm import DeclarativeBase
/// from sqlalchemy.orm import Mapped
/// from sqlalchemy.orm import mapped_column
///
/// class Base(DeclarativeBase):
///     pass
///
///
/// class MyModel(Base):
///     __tablename__ = "my_model"
///     id: Mapped[int] = mapped_column(primary_key=True)
///
///     count: Mapped[int]
///
///
/// m = MyModel()
/// reveal_type(m.count)  #  note: Revealed type is "builtins.int"
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct SQLAlchemyMissingMappedTypeAnnotation;

impl Violation for SQLAlchemyMissingMappedTypeAnnotation {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Missing `Mapped` or other ORM container class type annotation".to_string()
    }
}

/// SA001
pub(crate) fn missing_mapped_type_annotation(checker: &mut Checker, body: &[Stmt]) {
    if !checker.semantic().seen_module(Modules::SQLALCHEMY) {
        return;
    }

    for statement in body {
        let Stmt::Assign(ast::StmtAssign { value, targets, .. }) = statement else {
            continue;
        };

        if let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() {
            if helpers::is_mapped_attribute(func, checker.semantic()) {
                // SQLAlchemy does not allow multiple targets for column assignments.
                let [target] = targets.as_slice() else {
                    continue;
                };

                checker.report_diagnostic(Diagnostic::new(
                    SQLAlchemyMissingMappedTypeAnnotation {},
                    target.range(),
                ));
            }
        }
    }
}
