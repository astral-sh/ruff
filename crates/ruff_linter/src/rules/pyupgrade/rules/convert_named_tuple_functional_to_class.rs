use log::debug;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_dunder;
use ruff_python_ast::{
    self as ast, Arguments, Constant, Expr, ExprContext, Identifier, Keyword, Stmt,
};
use ruff_python_codegen::Generator;
use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `NamedTuple` declarations that use functional syntax.
///
/// ## Why is this bad?
/// `NamedTuple` subclasses can be defined either through a functional syntax
/// (`Foo = NamedTuple(...)`) or a class syntax (`class Foo(NamedTuple): ...`).
///
/// The class syntax is more readable and generally preferred over the
/// functional syntax, which exists primarily for backwards compatibility
/// with `collections.namedtuple`.
///
/// ## Example
/// ```python
/// from typing import NamedTuple
///
/// Foo = NamedTuple("Foo", [("a", int), ("b", str)])
/// ```
///
/// Use instead:
/// ```python
/// from typing import NamedTuple
///
///
/// class Foo(NamedTuple):
///     a: int
///     b: str
/// ```
///
/// ## References
/// - [Python documentation: `typing.NamedTuple`](https://docs.python.org/3/library/typing.html#typing.NamedTuple)
#[violation]
pub struct ConvertNamedTupleFunctionalToClass {
    name: String,
}

impl Violation for ConvertNamedTupleFunctionalToClass {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ConvertNamedTupleFunctionalToClass { name } = self;
        format!("Convert `{name}` from `NamedTuple` functional to class syntax")
    }

    fn fix_title(&self) -> Option<String> {
        let ConvertNamedTupleFunctionalToClass { name } = self;

        Some(format!("Convert `{name}` to class syntax"))
    }
}

/// UP014
pub(crate) fn convert_named_tuple_functional_to_class(
    checker: &mut Checker,
    stmt: &Stmt,
    targets: &[Expr],
    value: &Expr,
) {
    let Some((typename, args, keywords, base_class)) =
        match_named_tuple_assign(targets, value, checker.semantic())
    else {
        return;
    };

    let fields = match (args, keywords) {
        // Ex) `NamedTuple("MyType")`
        ([_typename], []) => vec![Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
        })],
        // Ex) `NamedTuple("MyType", [("a", int), ("b", str)])`
        ([_typename, fields], []) => {
            if let Some(fields) = create_fields_from_fields_arg(fields) {
                fields
            } else {
                debug!("Skipping `NamedTuple` \"{typename}\": unable to parse fields");
                return;
            }
        }
        // Ex) `NamedTuple("MyType", a=int, b=str)`
        ([_typename], keywords) => {
            if let Some(fields) = create_fields_from_keywords(keywords) {
                fields
            } else {
                debug!("Skipping `NamedTuple` \"{typename}\": unable to parse keywords");
                return;
            }
        }
        // Ex) `NamedTuple()`
        _ => {
            debug!("Skipping `NamedTuple` \"{typename}\": mixed fields and keywords");
            return;
        }
    };

    let mut diagnostic = Diagnostic::new(
        ConvertNamedTupleFunctionalToClass {
            name: typename.to_string(),
        },
        stmt.range(),
    );
    // TODO(charlie): Preserve indentation, to remove the first-column requirement.
    if checker.locator().is_at_start_of_line(stmt.start()) {
        diagnostic.set_fix(convert_to_class(
            stmt,
            typename,
            fields,
            base_class,
            checker.generator(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

/// Return the typename, args, keywords, and base class.
fn match_named_tuple_assign<'a>(
    targets: &'a [Expr],
    value: &'a Expr,
    semantic: &SemanticModel,
) -> Option<(&'a str, &'a [Expr], &'a [Keyword], &'a Expr)> {
    let [Expr::Name(ast::ExprName { id: typename, .. })] = targets else {
        return None;
    };
    let Expr::Call(ast::ExprCall {
        func,
        arguments: Arguments { args, keywords, .. },
        range: _,
    }) = value
    else {
        return None;
    };
    if !semantic.match_typing_expr(func, "NamedTuple") {
        return None;
    }
    Some((typename, args, keywords, func))
}

/// Generate a [`Stmt::AnnAssign`] representing the provided field definition.
fn create_field_assignment_stmt(field: &str, annotation: &Expr) -> Stmt {
    ast::StmtAnnAssign {
        target: Box::new(
            ast::ExprName {
                id: field.into(),
                ctx: ExprContext::Load,
                range: TextRange::default(),
            }
            .into(),
        ),
        annotation: Box::new(annotation.clone()),
        value: None,
        simple: true,
        range: TextRange::default(),
    }
    .into()
}

/// Create a list of field assignments from the `NamedTuple` fields argument.
fn create_fields_from_fields_arg(fields: &Expr) -> Option<Vec<Stmt>> {
    let ast::ExprList { elts, .. } = fields.as_list_expr()?;
    if elts.is_empty() {
        let node = Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
        });
        Some(vec![node])
    } else {
        elts.iter()
            .map(|field| {
                let ast::ExprTuple { elts, .. } = field.as_tuple_expr()?;
                let [field, annotation] = elts.as_slice() else {
                    return None;
                };
                let field = field.as_constant_expr()?;
                let Constant::Str(ast::StringConstant { value: field, .. }) = &field.value else {
                    return None;
                };
                if !is_identifier(field) {
                    return None;
                }
                if is_dunder(field) {
                    return None;
                }
                Some(create_field_assignment_stmt(field, annotation))
            })
            .collect()
    }
}

/// Create a list of field assignments from the `NamedTuple` keyword arguments.
fn create_fields_from_keywords(keywords: &[Keyword]) -> Option<Vec<Stmt>> {
    keywords
        .iter()
        .map(|keyword| {
            keyword
                .arg
                .as_ref()
                .map(|field| create_field_assignment_stmt(field.as_str(), &keyword.value))
        })
        .collect()
}

/// Generate a `StmtKind:ClassDef` statement based on the provided body and
/// keywords.
fn create_class_def_stmt(typename: &str, body: Vec<Stmt>, base_class: &Expr) -> Stmt {
    ast::StmtClassDef {
        name: Identifier::new(typename.to_string(), TextRange::default()),
        arguments: Some(Box::new(Arguments {
            args: vec![base_class.clone()],
            keywords: vec![],
            range: TextRange::default(),
        })),
        body,
        type_params: None,
        decorator_list: vec![],
        range: TextRange::default(),
    }
    .into()
}

/// Generate a `Fix` to convert a `NamedTuple` assignment to a class definition.
fn convert_to_class(
    stmt: &Stmt,
    typename: &str,
    body: Vec<Stmt>,
    base_class: &Expr,
    generator: Generator,
) -> Fix {
    Fix::safe_edit(Edit::range_replacement(
        generator.stmt(&create_class_def_stmt(typename, body, base_class)),
        stmt.range(),
    ))
}
