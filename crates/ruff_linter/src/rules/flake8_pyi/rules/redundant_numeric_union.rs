use bitflags::bitflags;

use anyhow::Result;

use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    name::Name, AnyParameterRef, Expr, ExprBinOp, ExprContext, ExprName, ExprSubscript, ExprTuple,
    Operator, Parameters,
};
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::{Ranged, TextRange};

use crate::{checkers::ast::Checker, importer::ImportRequest};

/// ## What it does
/// Checks for parameter annotations that contain redundant unions between
/// builtin numeric types (e.g., `int | float`).
///
/// ## Why is this bad?
/// The [typing specification] states:
///
/// > Python’s numeric types `complex`, `float` and `int` are not subtypes of
/// > each other, but to support common use cases, the type system contains a
/// > straightforward shortcut: when an argument is annotated as having type
/// > `float`, an argument of type `int` is acceptable; similar, for an
/// > argument annotated as having type `complex`, arguments of type `float` or
/// > `int` are acceptable.
///
/// As such, a union that includes both `int` and `float` is redundant in the
/// specific context of a parameter annotation, as it is equivalent to a union
/// that only includes `float`. For readability and clarity, unions should omit
/// redundant elements.
///
/// ## Example
///
/// ```pyi
/// def foo(x: float | int | str) -> None: ...
/// ```
///
/// Use instead:
///
/// ```pyi
/// def foo(x: float | str) -> None: ...
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as safe, unless the type annotation contains comments.
///
/// Note that while the fix may flatten nested unions into a single top-level union,
/// the semantics of the annotation will remain unchanged.
///
/// ## References
/// - [Python documentation: The numeric tower](https://docs.python.org/3/library/numbers.html#the-numeric-tower)
/// - [PEP 484: The numeric tower](https://peps.python.org/pep-0484/#the-numeric-tower)
///
/// [typing specification]: https://typing.readthedocs.io/en/latest/spec/special-types.html#special-cases-for-float-and-complex
#[derive(ViolationMetadata)]
pub(crate) struct RedundantNumericUnion {
    redundancy: Redundancy,
}

impl Violation for RedundantNumericUnion {
    // Always fixable, but currently under preview.
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let (subtype, supertype) = match self.redundancy {
            Redundancy::IntFloatComplex => ("int | float", "complex"),
            Redundancy::FloatComplex => ("float", "complex"),
            Redundancy::IntComplex => ("int", "complex"),
            Redundancy::IntFloat => ("int", "float"),
        };
        format!("Use `{supertype}` instead of `{subtype} | {supertype}`")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove redundant type".to_string())
    }
}

/// PYI041
pub(crate) fn redundant_numeric_union(checker: &Checker, parameters: &Parameters) {
    for annotation in parameters.iter().filter_map(AnyParameterRef::annotation) {
        check_annotation(checker, annotation);
    }
}

fn check_annotation<'a>(checker: &Checker, annotation: &'a Expr) {
    let mut numeric_flags = NumericFlags::empty();

    let mut find_numeric_type = |expr: &Expr, _parent: &Expr| {
        let Some(builtin_type) = checker.semantic().resolve_builtin_symbol(expr) else {
            return;
        };

        numeric_flags.seen_builtin_type(builtin_type);
    };

    // Traverse the union, and remember which numeric types are found.
    traverse_union(&mut find_numeric_type, checker.semantic(), annotation);

    let Some(redundancy) = Redundancy::from_numeric_flags(numeric_flags) else {
        return;
    };

    // Traverse the union a second time to construct the fix.
    let mut necessary_nodes: Vec<&Expr> = Vec::new();

    let mut union_type = UnionKind::TypingUnion;
    let mut remove_numeric_type = |expr: &'a Expr, parent: &'a Expr| {
        let Some(builtin_type) = checker.semantic().resolve_builtin_symbol(expr) else {
            // Keep type annotations that are not numeric.
            necessary_nodes.push(expr);
            return;
        };

        if matches!(parent, Expr::BinOp(_)) {
            union_type = UnionKind::PEP604;
        }

        // `int` is always dropped, since `float` or `complex` must be present.
        // `float` is only dropped if `complex`` is present.
        if (builtin_type == "float" && !numeric_flags.contains(NumericFlags::COMPLEX))
            || (builtin_type != "float" && builtin_type != "int")
        {
            necessary_nodes.push(expr);
        }
    };

    // Traverse the union a second time to construct a [`Fix`].
    traverse_union(&mut remove_numeric_type, checker.semantic(), annotation);

    let mut diagnostic = Diagnostic::new(RedundantNumericUnion { redundancy }, annotation.range());

    // Mark [`Fix`] as unsafe when comments are in range.
    let applicability = if checker.comment_ranges().intersects(annotation.range()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    // Generate the flattened fix once.
    let fix = if let &[edit_expr] = necessary_nodes.as_slice() {
        // Generate a [`Fix`] for a single type expression, e.g. `int`.
        Some(Fix::applicable_edit(
            Edit::range_replacement(checker.generator().expr(edit_expr), annotation.range()),
            applicability,
        ))
    } else {
        match union_type {
            UnionKind::PEP604 => Some(generate_pep604_fix(
                checker,
                necessary_nodes,
                annotation,
                applicability,
            )),
            UnionKind::TypingUnion => {
                generate_union_fix(checker, necessary_nodes, annotation, applicability).ok()
            }
        }
    };

    if let Some(fix) = fix {
        diagnostic.set_fix(fix);
    }

    checker.report_diagnostic(diagnostic);
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Redundancy {
    IntFloatComplex,
    FloatComplex,
    IntComplex,
    IntFloat,
}

impl Redundancy {
    pub(super) fn from_numeric_flags(numeric_flags: NumericFlags) -> Option<Self> {
        if numeric_flags == NumericFlags::INT | NumericFlags::FLOAT | NumericFlags::COMPLEX {
            Some(Self::IntFloatComplex)
        } else if numeric_flags == NumericFlags::FLOAT | NumericFlags::COMPLEX {
            Some(Self::FloatComplex)
        } else if numeric_flags == NumericFlags::INT | NumericFlags::COMPLEX {
            Some(Self::IntComplex)
        } else if numeric_flags == NumericFlags::FLOAT | NumericFlags::INT {
            Some(Self::IntFloat)
        } else {
            None
        }
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub(super) struct NumericFlags: u8 {
        /// `int`
        const INT = 1 << 0;
        /// `float`
        const FLOAT = 1 << 1;
        /// `complex`
        const COMPLEX = 1 << 2;
    }
}

impl NumericFlags {
    pub(super) fn seen_builtin_type(&mut self, name: &str) {
        let flag: NumericFlags = match name {
            "int" => NumericFlags::INT,
            "float" => NumericFlags::FLOAT,
            "complex" => NumericFlags::COMPLEX,
            _ => {
                return;
            }
        };
        self.insert(flag);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnionKind {
    /// E.g., `typing.Union[int, str]`
    TypingUnion,
    /// E.g., `int | str`
    PEP604,
}

/// Generate a [`Fix`] for two or more type expressions, e.g. `int | float | complex`.
fn generate_pep604_fix(
    checker: &Checker,
    nodes: Vec<&Expr>,
    annotation: &Expr,
    applicability: Applicability,
) -> Fix {
    debug_assert!(nodes.len() >= 2, "At least two nodes required");

    let new_expr = nodes
        .into_iter()
        .fold(None, |acc: Option<Expr>, right: &Expr| {
            if let Some(left) = acc {
                Some(Expr::BinOp(ExprBinOp {
                    left: Box::new(left),
                    op: Operator::BitOr,
                    right: Box::new(right.clone()),
                    range: TextRange::default(),
                }))
            } else {
                Some(right.clone())
            }
        })
        .unwrap();

    Fix::applicable_edit(
        Edit::range_replacement(checker.generator().expr(&new_expr), annotation.range()),
        applicability,
    )
}

/// Generate a [`Fix`] for two or more type expressions, e.g. `typing.Union[int, float, complex]`.
fn generate_union_fix(
    checker: &Checker,
    nodes: Vec<&Expr>,
    annotation: &Expr,
    applicability: Applicability,
) -> Result<Fix> {
    debug_assert!(nodes.len() >= 2, "At least two nodes required");

    // Request `typing.Union`
    let (import_edit, binding) = checker.importer().get_or_import_symbol(
        &ImportRequest::import_from("typing", "Union"),
        annotation.start(),
        checker.semantic(),
    )?;

    // Construct the expression as `Subscript[typing.Union, Tuple[expr, [expr, ...]]]`
    let new_expr = Expr::Subscript(ExprSubscript {
        range: TextRange::default(),
        value: Box::new(Expr::Name(ExprName {
            id: Name::new(binding),
            ctx: ExprContext::Store,
            range: TextRange::default(),
        })),
        slice: Box::new(Expr::Tuple(ExprTuple {
            elts: nodes.into_iter().cloned().collect(),
            range: TextRange::default(),
            ctx: ExprContext::Load,
            parenthesized: false,
        })),
        ctx: ExprContext::Load,
    });

    Ok(Fix::applicable_edits(
        Edit::range_replacement(checker.generator().expr(&new_expr), annotation.range()),
        [import_edit],
        applicability,
    ))
}
