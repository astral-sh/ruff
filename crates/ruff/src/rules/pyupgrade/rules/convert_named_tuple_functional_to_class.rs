use anyhow::{bail, Result};
use log::debug;
use ruff_text_size::TextRange;
use rustpython_parser::ast::{
    self, Constant, Expr, ExprContext, Identifier, Keyword, Ranged, Stmt,
};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_dunder;
use ruff_python_ast::source_code::Generator;
use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

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
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ConvertNamedTupleFunctionalToClass { name } = self;
        format!("Convert `{name}` from `NamedTuple` functional to class syntax")
    }

    fn autofix_title(&self) -> Option<String> {
        let ConvertNamedTupleFunctionalToClass { name } = self;

        Some(format!("Convert `{name}` to class syntax"))
    }
}

/// Return the typename, args, keywords, and base class.
fn match_named_tuple_assign<'a>(
    targets: &'a [Expr],
    value: &'a Expr,
    semantic: &SemanticModel,
) -> Option<(&'a str, &'a [Expr], &'a [Keyword], &'a Expr)> {
    let target = targets.get(0)?;
    let Expr::Name(ast::ExprName { id: typename, .. }) = target else {
        return None;
    };
    let Expr::Call(ast::ExprCall {
        func,
        args,
        keywords,
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

/// Generate a `Stmt::AnnAssign` representing the provided property
/// definition.
fn create_property_assignment_stmt(property: &str, annotation: &Expr) -> Stmt {
    ast::StmtAnnAssign {
        target: Box::new(
            ast::ExprName {
                id: property.into(),
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

/// Create a list of property assignments from the `NamedTuple` fields argument.
fn create_properties_from_fields_arg(fields: &Expr) -> Result<Vec<Stmt>> {
    let Expr::List(ast::ExprList { elts, .. }) = &fields else {
        bail!("Expected argument to be `Expr::List`");
    };
    if elts.is_empty() {
        let node = Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
        });
        return Ok(vec![node]);
    }
    elts.iter()
        .map(|field| {
            let Expr::Tuple(ast::ExprTuple { elts, .. }) = &field else {
                bail!("Expected `field` to be `Expr::Tuple`")
            };
            let [field_name, annotation] = elts.as_slice() else {
                bail!("Expected `elts` to have exactly two elements")
            };
            let Expr::Constant(ast::ExprConstant {
                value: Constant::Str(property),
                ..
            }) = &field_name
            else {
                bail!("Expected `field_name` to be `Constant::Str`")
            };
            if !is_identifier(property) {
                bail!("Invalid property name: {}", property)
            }
            if is_dunder(property) {
                bail!("Cannot use dunder property name: {}", property)
            }
            Ok(create_property_assignment_stmt(property, annotation))
        })
        .collect()
}

/// Create a list of property assignments from the `NamedTuple` keyword arguments.
fn create_properties_from_keywords(keywords: &[Keyword]) -> Result<Vec<Stmt>> {
    keywords
        .iter()
        .map(|keyword| {
            let Keyword { arg, value, .. } = keyword;
            let Some(arg) = arg else {
                bail!("Expected `keyword` to have an `arg`")
            };
            Ok(create_property_assignment_stmt(arg.as_str(), value))
        })
        .collect()
}

/// Generate a `StmtKind:ClassDef` statement based on the provided body and
/// keywords.
fn create_class_def_stmt(typename: &str, body: Vec<Stmt>, base_class: &Expr) -> Stmt {
    ast::StmtClassDef {
        name: Identifier::new(typename.to_string(), TextRange::default()),
        bases: vec![base_class.clone()],
        keywords: vec![],
        body,
        type_params: vec![],
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
    Fix::suggested(Edit::range_replacement(
        generator.stmt(&create_class_def_stmt(typename, body, base_class)),
        stmt.range(),
    ))
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

    let properties = match (&args[1..], keywords) {
        // Ex) NamedTuple("MyType")
        ([], []) => vec![Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
        })],
        // Ex) NamedTuple("MyType", [("a", int), ("b", str)])
        ([fields], []) => {
            if let Ok(properties) = create_properties_from_fields_arg(fields) {
                properties
            } else {
                debug!("Skipping `NamedTuple` \"{typename}\": unable to parse fields");
                return;
            }
        }
        // Ex) NamedTuple("MyType", a=int, b=str)
        ([], keywords) => {
            if let Ok(properties) = create_properties_from_keywords(keywords) {
                properties
            } else {
                debug!("Skipping `NamedTuple` \"{typename}\": unable to parse keywords");
                return;
            }
        }
        // Unfixable
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
    if checker.patch(diagnostic.kind.rule()) {
        // TODO(charlie): Preserve indentation, to remove the first-column requirement.
        if checker.locator.is_at_start_of_line(stmt.start()) {
            diagnostic.set_fix(convert_to_class(
                stmt,
                typename,
                properties,
                base_class,
                checker.generator(),
            ));
        }
    }
    checker.diagnostics.push(diagnostic);
}
