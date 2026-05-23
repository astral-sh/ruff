use log::debug;

use ruff_allocator::{Allocator, Box as ArenaBox, Slice as ArenaSlice};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::is_dunder;
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
/// ## Fix safety
/// This rule's fix is marked as unsafe if there are any comments within the
/// range of the `NamedTuple` definition, as these will be dropped by the
/// autofix.
///
/// ## References
/// - [Python documentation: `typing.NamedTuple`](https://docs.python.org/3/library/typing.html#typing.NamedTuple)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.155")]
pub(crate) struct ConvertNamedTupleFunctionalToClass {
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
    checker: &Checker,
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
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        })],
        // Ex) `NamedTuple("MyType", [("a", int), ("b", str)])`
        ([_typename, fields], []) => {
            if let Some(fields) = create_fields_from_fields_arg(fields, checker.allocator()) {
                fields
            } else {
                debug!("Skipping `NamedTuple` \"{typename}\": unable to parse fields");
                return;
            }
        }
        // Ex) `NamedTuple("MyType", a=int, b=str)`
        ([_typename], keywords) => {
            if let Some(fields) = create_fields_from_keywords(keywords, checker.allocator()) {
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

    let mut diagnostic = checker.report_diagnostic(
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
            checker.comment_ranges(),
            checker.allocator(),
        ));
    }
}

/// Return the typename, args, keywords, and base class.
fn match_named_tuple_assign<'a>(
    targets: &'a [Expr],
    value: &'a Expr,
    semantic: &SemanticModel,
) -> Option<(&'a str, &'a [Expr<'a>], &'a [Keyword<'a>], &'a Expr<'a>)> {
    let [Expr::Name(ast::ExprName { id: typename, .. })] = targets else {
        return None;
    };
    let Expr::Call(ast::ExprCall {
        func,
        arguments: Arguments { args, keywords, .. },
        range: _,
        node_index: _,
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
fn create_field_assignment_stmt<'a>(
    field: ast::name::AstName<'a>,
    annotation: &'a Expr<'a>,
    allocator: &'a Allocator,
) -> Stmt<'a> {
    ast::StmtAnnAssign {
        target: ArenaBox::new_in(
            ast::ExprName {
                id: field,
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

/// Create a list of field assignments from the `NamedTuple` fields argument.
fn create_fields_from_fields_arg<'a>(
    fields: &'a Expr<'a>,
    allocator: &'a Allocator,
) -> Option<Vec<Stmt<'a>>> {
    let fields = fields.as_list_expr()?;
    if fields.is_empty() {
        let node = Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        });
        Some(vec![node])
    } else {
        fields
            .iter()
            .map(|field| {
                let ast::ExprTuple { elts, .. } = field.as_tuple_expr()?;
                let [field, annotation] = elts.as_slice() else {
                    return None;
                };
                if annotation.is_starred_expr() {
                    return None;
                }
                let ast::ExprStringLiteral { value: field, .. } = field.as_string_literal_expr()?;
                if !is_identifier(field.to_str()) {
                    return None;
                }
                if is_dunder(field.to_str()) {
                    return None;
                }
                Some(create_field_assignment_stmt(
                    ast::name::AstName::new_in(field.to_str(), allocator),
                    annotation,
                    allocator,
                ))
            })
            .collect()
    }
}

/// Create a list of field assignments from the `NamedTuple` keyword arguments.
fn create_fields_from_keywords<'a>(
    keywords: &'a [Keyword<'a>],
    allocator: &'a Allocator,
) -> Option<Vec<Stmt<'a>>> {
    keywords
        .iter()
        .map(|keyword| {
            keyword.arg.as_ref().map(|field| {
                create_field_assignment_stmt(field.id.clone(), &keyword.value, allocator)
            })
        })
        .collect()
}

/// Generate a `StmtKind:ClassDef` statement based on the provided body and
/// keywords.
fn create_class_def_stmt<'a>(
    typename: &'a str,
    body: Vec<Stmt<'a>>,
    base_class: &'a Expr<'a>,
    allocator: &'a Allocator,
) -> Stmt<'a> {
    ast::StmtClassDef {
        name: Identifier::new_in(typename, TextRange::default(), allocator),
        arguments: Some(ArenaBox::new_in(
            Arguments {
                args: ArenaSlice::from_iter_in([base_class.clone()], allocator),
                keywords: ArenaSlice::new_in(allocator),
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

/// Generate a `Fix` to convert a `NamedTuple` assignment to a class definition.
fn convert_to_class<'a>(
    stmt: &Stmt<'a>,
    typename: &'a str,
    body: Vec<Stmt<'a>>,
    base_class: &'a Expr<'a>,
    generator: Generator,
    comment_ranges: &CommentRanges,
    allocator: &'a Allocator,
) -> Fix {
    Fix::applicable_edit(
        Edit::range_replacement(
            generator.stmt(&create_class_def_stmt(
                typename, body, base_class, allocator,
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
