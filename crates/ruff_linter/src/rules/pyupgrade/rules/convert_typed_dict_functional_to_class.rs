use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Arguments, Expr, ExprContext, Identifier, Keyword, Stmt};
use ruff_python_codegen::Generator;
use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_python_trivia::CommentRanges;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `TypedDict` declarations that use functional syntax.
///
/// ## Why is this bad?
/// `TypedDict` types can be defined either through a functional syntax
/// (`Foo = TypedDict(...)`) or a class syntax (`class Foo(TypedDict): ...`).
///
/// The class syntax is more readable and generally preferred over the
/// functional syntax.
///
/// Nonetheless, there are some situations in which it is impossible to use
/// the class-based syntax. This rule will not apply to those cases. Namely,
/// it is impossible to use the class-based syntax if any `TypedDict` fields are:
/// - Not valid [python identifiers] (for example, `@x`)
/// - [Python keywords] such as `in`
/// - [Private names] such as `__id` that would undergo [name mangling] at runtime
///   if the class-based syntax was used
/// - [Dunder names] such as `__int__` that can confuse type checkers if they're used
///   with the class-based syntax.
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
/// ## Fix safety
/// This rule's fix is marked as unsafe if there are any comments within the
/// range of the `TypedDict` definition, as these will be dropped by the
/// autofix.
///
/// ## References
/// - [Python documentation: `typing.TypedDict`](https://docs.python.org/3/library/typing.html#typing.TypedDict)
///
/// [Private names]: https://docs.python.org/3/tutorial/classes.html#private-variables
/// [name mangling]: https://docs.python.org/3/reference/expressions.html#private-name-mangling
/// [python identifiers]: https://docs.python.org/3/reference/lexical_analysis.html#identifiers
/// [Python keywords]: https://docs.python.org/3/reference/lexical_analysis.html#keywords
/// [Dunder names]: https://docs.python.org/3/reference/lexical_analysis.html#reserved-classes-of-identifiers
#[derive(ViolationMetadata)]
pub(crate) struct ConvertTypedDictFunctionalToClass {
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
    checker: &Checker,
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
            checker.comment_ranges(),
        ));
    }
    checker.report_diagnostic(diagnostic);
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
            args: Box::from([base_class.clone()]),
            keywords: match total_keyword {
                Some(keyword) => Box::from([keyword.clone()]),
                None => Box::from([]),
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

fn fields_from_dict_literal(items: &[ast::DictItem]) -> Option<Vec<Stmt>> {
    if items.is_empty() {
        let node = Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
        });
        Some(vec![node])
    } else {
        items
            .iter()
            .map(|ast::DictItem { key, value }| match key {
                Some(Expr::StringLiteral(ast::ExprStringLiteral { value: field, .. })) => {
                    if !is_identifier(field.to_str()) {
                        return None;
                    }
                    // Converting TypedDict to class-based syntax is not safe if fields contain
                    // private or dunder names, because private names will be mangled and dunder
                    // names can confuse type checkers.
                    if field.to_str().starts_with("__") {
                        return None;
                    }
                    Some(create_field_assignment_stmt(field.to_str(), value))
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
    match (&*arguments.args, &*arguments.keywords) {
        // Ex) `TypedDict("MyType", {"a": int, "b": str})`
        ([_typename, fields], [..]) => {
            let total = arguments.find_keyword("total");
            match fields {
                Expr::Dict(ast::ExprDict { items, range: _ }) => {
                    Some((fields_from_dict_literal(items)?, total))
                }
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
    comment_ranges: &CommentRanges,
) -> Fix {
    Fix::applicable_edit(
        Edit::range_replacement(
            generator.stmt(&create_class_def_stmt(
                class_name,
                body,
                total_keyword,
                base_class,
            )),
            stmt.range(),
        ),
        if comment_ranges.intersects(stmt.range()) {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        },
    )
}
