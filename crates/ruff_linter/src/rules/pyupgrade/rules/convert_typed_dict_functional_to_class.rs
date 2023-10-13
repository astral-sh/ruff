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
/// Checks for `TypedDict` declarations that use functional syntax.
///
/// ## Why is this bad?
/// `TypedDict` subclasses can be defined either through a functional syntax
/// (`Foo = TypedDict(...)`) or a class syntax (`class Foo(TypedDict): ...`).
///
/// The class syntax is more readable and generally preferred over the
/// functional syntax.
///
/// ## Example
/// ```python
/// from typing import TypedDict
///
/// Foo = TypedDict("Foo", {"a": int, "b": str})
/// ```
///
/// Use instead:
/// ```python
/// from typing import TypedDict
///
///
/// class Foo(TypedDict):
///     a: int
///     b: str
/// ```
///
/// ## References
/// - [Python documentation: `typing.TypedDict`](https://docs.python.org/3/library/typing.html#typing.TypedDict)
#[violation]
pub struct ConvertTypedDictFunctionalToClass {
    name: String,
}

impl Violation for ConvertTypedDictFunctionalToClass {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ConvertTypedDictFunctionalToClass { name } = self;
        format!("Convert `{name}` from `TypedDict` functional to class syntax")
    }

    fn fix_title(&self) -> Option<String> {
        let ConvertTypedDictFunctionalToClass { name } = self;
        Some(format!("Convert `{name}` to class syntax"))
    }
}

/// UP013
pub(crate) fn convert_typed_dict_functional_to_class(
    checker: &mut Checker,
    stmt: &Stmt,
    targets: &[Expr],
    value: &Expr,
) {
    let Some((class_name, arguments, base_class)) =
        match_typed_dict_assign(targets, value, checker.semantic())
    else {
        return;
    };

    let Some((body, total_keyword)) = match_fields_and_total(arguments) else {
        return;
    };

    let mut diagnostic = Diagnostic::new(
        ConvertTypedDictFunctionalToClass {
            name: class_name.to_string(),
        },
        stmt.range(),
    );
    // TODO(charlie): Preserve indentation, to remove the first-column requirement.
    if checker.locator().is_at_start_of_line(stmt.start()) {
        diagnostic.set_fix(convert_to_class(
            stmt,
            class_name,
            body,
            total_keyword,
            base_class,
            checker.generator(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

/// Return the class name, arguments, keywords and base class for a `TypedDict`
/// assignment.
fn match_typed_dict_assign<'a>(
    targets: &'a [Expr],
    value: &'a Expr,
    semantic: &SemanticModel,
) -> Option<(&'a str, &'a Arguments, &'a Expr)> {
    let [Expr::Name(ast::ExprName { id: class_name, .. })] = targets else {
        return None;
    };
    let Expr::Call(ast::ExprCall {
        func,
        arguments,
        range: _,
    }) = value
    else {
        return None;
    };
    if !semantic.match_typing_expr(func, "TypedDict") {
        return None;
    }
    Some((class_name, arguments, func))
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

/// Generate a `StmtKind:ClassDef` statement based on the provided body, keywords, and base class.
fn create_class_def_stmt(
    class_name: &str,
    body: Vec<Stmt>,
    total_keyword: Option<&Keyword>,
    base_class: &Expr,
) -> Stmt {
    ast::StmtClassDef {
        name: Identifier::new(class_name.to_string(), TextRange::default()),
        arguments: Some(Box::new(Arguments {
            args: vec![base_class.clone()],
            keywords: match total_keyword {
                Some(keyword) => vec![keyword.clone()],
                None => vec![],
            },
            range: TextRange::default(),
        })),
        body,
        type_params: None,
        decorator_list: vec![],
        range: TextRange::default(),
    }
    .into()
}

fn fields_from_dict_literal(keys: &[Option<Expr>], values: &[Expr]) -> Option<Vec<Stmt>> {
    if keys.is_empty() {
        let node = Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
        });
        Some(vec![node])
    } else {
        keys.iter()
            .zip(values.iter())
            .map(|(key, value)| match key {
                Some(Expr::Constant(ast::ExprConstant {
                    value: Constant::Str(ast::StringConstant { value: field, .. }),
                    ..
                })) => {
                    if !is_identifier(field) {
                        return None;
                    }
                    if is_dunder(field) {
                        return None;
                    }
                    Some(create_field_assignment_stmt(field, value))
                }
                _ => None,
            })
            .collect()
    }
}

fn fields_from_dict_call(func: &Expr, keywords: &[Keyword]) -> Option<Vec<Stmt>> {
    let ast::ExprName { id, .. } = func.as_name_expr()?;
    if id != "dict" {
        return None;
    }

    if keywords.is_empty() {
        let node = Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
        });
        Some(vec![node])
    } else {
        fields_from_keywords(keywords)
    }
}

// Deprecated in Python 3.11, removed in Python 3.13.
fn fields_from_keywords(keywords: &[Keyword]) -> Option<Vec<Stmt>> {
    if keywords.is_empty() {
        let node = Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
        });
        return Some(vec![node]);
    }

    keywords
        .iter()
        .map(|keyword| {
            keyword
                .arg
                .as_ref()
                .map(|field| create_field_assignment_stmt(field, &keyword.value))
        })
        .collect()
}

/// Match the fields and `total` keyword from a `TypedDict` call.
fn match_fields_and_total(arguments: &Arguments) -> Option<(Vec<Stmt>, Option<&Keyword>)> {
    match (arguments.args.as_slice(), arguments.keywords.as_slice()) {
        // Ex) `TypedDict("MyType", {"a": int, "b": str})`
        ([_typename, fields], [..]) => {
            let total = arguments.find_keyword("total");
            match fields {
                Expr::Dict(ast::ExprDict {
                    keys,
                    values,
                    range: _,
                }) => Some((fields_from_dict_literal(keys, values)?, total)),
                Expr::Call(ast::ExprCall {
                    func,
                    arguments: Arguments { keywords, .. },
                    range: _,
                }) => Some((fields_from_dict_call(func, keywords)?, total)),
                _ => None,
            }
        }
        // Ex) `TypedDict("MyType")`
        ([_typename], []) => {
            let node = Stmt::Pass(ast::StmtPass {
                range: TextRange::default(),
            });
            Some((vec![node], None))
        }
        // Ex) `TypedDict("MyType", a=int, b=str)`
        ([_typename], fields) => Some((fields_from_keywords(fields)?, None)),
        // Ex) `TypedDict()`
        _ => None,
    }
}

/// Generate a `Fix` to convert a `TypedDict` from functional to class.
fn convert_to_class(
    stmt: &Stmt,
    class_name: &str,
    body: Vec<Stmt>,
    total_keyword: Option<&Keyword>,
    base_class: &Expr,
    generator: Generator,
) -> Fix {
    Fix::safe_edit(Edit::range_replacement(
        generator.stmt(&create_class_def_stmt(
            class_name,
            body,
            total_keyword,
            base_class,
        )),
        stmt.range(),
    ))
}
