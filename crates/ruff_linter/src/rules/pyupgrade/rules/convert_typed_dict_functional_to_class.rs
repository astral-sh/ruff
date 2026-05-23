use ruff_allocator::{Allocator, Box as ArenaBox, Slice as ArenaSlice};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Arguments, Expr, ExprContext, Identifier, Keyword, Stmt};
use ruff_python_codegen::Generator;
use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_python_trivia::CommentRanges;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::{Applicability, Edit, Fix, FixAvailability, Violation};

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
#[violation_metadata(stable_since = "v0.0.155")]
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

    let Some((body, total_keyword)) = match_fields_and_total(arguments, checker.allocator()) else {
        return;
    };

    let mut diagnostic = checker.report_diagnostic(
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
            checker.allocator(),
        ));
    }
}

/// Return the class name, arguments, keywords and base class for a `TypedDict`
/// assignment.
fn match_typed_dict_assign<'a>(
    targets: &'a [Expr],
    value: &'a Expr,
    semantic: &SemanticModel,
) -> Option<(&'a str, &'a Arguments<'a>, &'a Expr<'a>)> {
    let [Expr::Name(ast::ExprName { id: class_name, .. })] = targets else {
        return None;
    };
    let Expr::Call(ast::ExprCall {
        func,
        arguments,
        range: _,
        node_index: _,
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
fn create_field_assignment_stmt<'a>(
    field: &'a str,
    annotation: &'a Expr<'a>,
    allocator: &'a Allocator,
) -> Stmt<'a> {
    ast::StmtAnnAssign {
        target: ArenaBox::new_in(
            ast::ExprName {
                id: ast::name::AstName::new_in(field, allocator),
                ctx: ExprContext::Load,
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            }
            .into(),
            allocator,
        ),
        annotation: ArenaBox::from_ref(annotation),
        value: None,
        simple: true,
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
    }
    .into()
}

/// Generate a `StmtKind:ClassDef` statement based on the provided body, keywords, and base class.
fn create_class_def_stmt<'a>(
    class_name: &'a str,
    body: Vec<Stmt<'a>>,
    total_keyword: Option<&'a Keyword<'a>>,
    base_class: &'a Expr<'a>,
    allocator: &'a Allocator,
) -> Stmt<'a> {
    ast::StmtClassDef {
        name: Identifier::new_in(class_name, TextRange::default(), allocator),
        arguments: Some(ArenaBox::new_in(
            Arguments {
                args: ArenaSlice::from_iter_in([base_class.clone()], allocator),
                keywords: match total_keyword {
                    Some(keyword) => ArenaSlice::from_iter_in([keyword.clone()], allocator),
                    None => ArenaSlice::new_in(allocator),
                },
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            },
            allocator,
        )),
        body: ArenaSlice::from_vec_in(body, allocator),
        type_params: None,
        decorator_list: ArenaSlice::new_in(allocator),
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
    }
    .into()
}

fn fields_from_dict_literal<'a>(
    items: &'a [ast::DictItem<'a>],
    allocator: &'a Allocator,
) -> Option<Vec<Stmt<'a>>> {
    if items.is_empty() {
        let node = Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
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
                    Some(create_field_assignment_stmt(
                        field.to_str(),
                        value,
                        allocator,
                    ))
                }
                _ => None,
            })
            .collect()
    }
}

fn fields_from_dict_call<'a>(
    func: &'a Expr<'a>,
    keywords: &'a [Keyword<'a>],
    allocator: &'a Allocator,
) -> Option<Vec<Stmt<'a>>> {
    let ast::ExprName { id, .. } = func.as_name_expr()?;
    if id != "dict" {
        return None;
    }

    if keywords.is_empty() {
        let node = Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        });
        Some(vec![node])
    } else {
        fields_from_keywords(keywords, allocator)
    }
}

// Deprecated in Python 3.11, removed in Python 3.13.
fn fields_from_keywords<'a>(
    keywords: &'a [Keyword<'a>],
    allocator: &'a Allocator,
) -> Option<Vec<Stmt<'a>>> {
    if keywords.is_empty() {
        let node = Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        });
        return Some(vec![node]);
    }

    keywords
        .iter()
        .map(|keyword| {
            keyword
                .arg
                .as_ref()
                .map(|field| create_field_assignment_stmt(field, &keyword.value, allocator))
        })
        .collect()
}

/// Match the fields and `total` keyword from a `TypedDict` call.
fn match_fields_and_total<'a>(
    arguments: &'a Arguments<'a>,
    allocator: &'a Allocator,
) -> Option<(Vec<Stmt<'a>>, Option<&'a Keyword<'a>>)> {
    match (&*arguments.args, &*arguments.keywords) {
        // Ex) `TypedDict("MyType", {"a": int, "b": str})`
        ([_typename, fields], [..]) => {
            let total = arguments.find_keyword("total");
            match fields {
                Expr::Dict(ast::ExprDict {
                    items,
                    range: _,
                    node_index: _,
                }) => Some((fields_from_dict_literal(items, allocator)?, total)),
                Expr::Call(ast::ExprCall {
                    func,
                    arguments: Arguments { keywords, .. },
                    range: _,
                    node_index: _,
                }) => Some((fields_from_dict_call(func, keywords, allocator)?, total)),
                _ => None,
            }
        }
        // Ex) `TypedDict("MyType")`
        ([_typename], []) => {
            let node = Stmt::Pass(ast::StmtPass {
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            });
            Some((vec![node], None))
        }
        // Ex) `TypedDict("MyType", a=int, b=str)`
        ([_typename], fields) => Some((fields_from_keywords(fields, allocator)?, None)),
        // Ex) `TypedDict()`
        _ => None,
    }
}

/// Generate a `Fix` to convert a `TypedDict` from functional to class.
#[expect(clippy::too_many_arguments)]
fn convert_to_class<'a>(
    stmt: &Stmt<'a>,
    class_name: &'a str,
    body: Vec<Stmt<'a>>,
    total_keyword: Option<&'a Keyword<'a>>,
    base_class: &'a Expr<'a>,
    generator: Generator,
    comment_ranges: &CommentRanges,
    allocator: &'a Allocator,
) -> Fix {
    Fix::applicable_edit(
        Edit::range_replacement(
            generator.stmt(&create_class_def_stmt(
                class_name,
                body,
                total_keyword,
                base_class,
                allocator,
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
