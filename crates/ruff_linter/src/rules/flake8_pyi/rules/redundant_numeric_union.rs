use bitflags::bitflags;
use std::fmt;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, AnyParameterRef, Expr, ExprBinOp, Operator, Parameters, PythonVersion};
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::preview::is_resolve_string_annotation_pyi041_enabled;
use crate::{Applicability, Edit, Fix, FixAvailability, Violation};

use super::generate_union_fix;

#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.279")]
pub(crate) struct RedundantNumericUnion {
    redundancy: Redundancy,
}

impl Violation for RedundantNumericUnion {
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

/// PYI041 - Entry point for function parameters
pub(crate) fn redundant_numeric_union(checker: &Checker, parameters: &Parameters) {
    for annotation in parameters.iter().filter_map(AnyParameterRef::annotation) {
        check_annotation(checker, annotation);
    }
}

/// PYI041 - Core logic for any type annotation (Class Fields, Variables, etc.)
pub(crate) fn check_annotation<'a, 'b>(checker: &Checker<'a>, unresolved_annotation: &'b Expr)
where
    'a: 'b,
{
    let mut numeric_flags = NumericFlags::empty();

    let mut find_numeric_type = |expr: &Expr, _parent: &Expr| {
        let Some(builtin_type) = checker.semantic().resolve_builtin_symbol(expr) else {
            return;
        };
        numeric_flags.seen_builtin_type(builtin_type);
    };

    let annotation = map_maybe_stringized_annotation(checker, unresolved_annotation);

    traverse_union(&mut find_numeric_type, checker.semantic(), &annotation);

    let Some(redundancy) = Redundancy::from_numeric_flags(numeric_flags) else {
        return;
    };

    let mut necessary_nodes: Vec<&Expr> = Vec::new();
    let mut union_type = UnionKind::TypingUnion;

    let mut remove_numeric_type = |expr: &'b Expr, parent: &'b Expr| {
        let Some(builtin_type) = checker.semantic().resolve_builtin_symbol(expr) else {
            necessary_nodes.push(expr);
            return;
        };

        if matches!(parent, Expr::BinOp(_)) {
            union_type = UnionKind::PEP604;
        }

        let is_redundant = match builtin_type {
            "int" => numeric_flags.intersects(NumericFlags::FLOAT | NumericFlags::COMPLEX),
            "float" => numeric_flags.contains(NumericFlags::COMPLEX),
            _ => false,
        };

        if !is_redundant {
            necessary_nodes.push(expr);
        }
    };

    traverse_union(&mut remove_numeric_type, checker.semantic(), &annotation);

    let mut diagnostic =
        checker.report_diagnostic(RedundantNumericUnion { redundancy }, annotation.range());

    if !checker.semantic().execution_context().is_typing()
        && !checker.source_type.is_stub()
        && fix_starts_with_none_none(&necessary_nodes)
    {
        return;
    }

    if annotation.is_complex() {
        return;
    }

    let applicability =
        if annotation.is_string() || checker.comment_ranges().intersects(annotation.range()) {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        };

    let fix = if let &[edit_expr] = necessary_nodes.as_slice() {
        Some(Fix::applicable_edit(
            Edit::range_replacement(checker.generator().expr(edit_expr), annotation.range()),
            applicability,
        ))
    } else {
        match union_type {
            UnionKind::PEP604 => Some(generate_pep604_fix(
                checker,
                necessary_nodes,
                &annotation,
                applicability,
            )),
            UnionKind::TypingUnion => {
                let Some(importer) = checker.typing_importer("Union", PythonVersion::lowest())
                else {
                    return;
                };
                generate_union_fix(
                    checker.generator(),
                    &importer,
                    necessary_nodes,
                    &annotation,
                    applicability,
                )
                .ok()
            }
        }
    };

    if let Some(fix) = fix {
        diagnostic.set_fix(fix);
    }
}

fn map_maybe_stringized_annotation<'a, 'b>(
    checker: &Checker<'a>,
    expr: &'b Expr,
) -> AnnotationKind<'b>
where
    'a: 'b,
{
    if !is_resolve_string_annotation_pyi041_enabled(checker.settings()) {
        return AnnotationKind::Simple(expr);
    }

    if let Expr::StringLiteral(string_annotation) = expr
        && let Ok(parsed_annotation) = checker.parse_type_annotation(string_annotation)
    {
        let expr = parsed_annotation.expression();
        return match parsed_annotation.kind() {
            ruff_python_parser::typing::AnnotationKind::Simple => AnnotationKind::String(expr),
            ruff_python_parser::typing::AnnotationKind::Complex => AnnotationKind::Complex(expr),
        };
    }
    AnnotationKind::Simple(expr)
}

enum AnnotationKind<'a> {
    Simple(&'a Expr),
    String(&'a Expr),
    Complex(&'a Expr),
}

impl<'a> std::ops::Deref for AnnotationKind<'a> {
    type Target = &'a Expr;
    fn deref(&self) -> &Self::Target {
        match self {
            AnnotationKind::Simple(expr) | AnnotationKind::String(expr) | AnnotationKind::Complex(expr) => expr,
        }
    }
}

impl AnnotationKind<'_> {
    fn is_string(&self) -> bool { matches!(self, Self::String(_)) }
    fn is_complex(&self) -> bool { matches!(self, Self::Complex(_)) }
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
        const INT = 1 << 0;
        const FLOAT = 1 << 1;
        const COMPLEX = 1 << 2;
    }
}

impl NumericFlags {
    pub(super) fn seen_builtin_type(&mut self, name: &str) {
        let flag = match name {
            "int" => NumericFlags::INT,
            "float" => NumericFlags::FLOAT,
            "complex" => NumericFlags::COMPLEX,
            _ => return,
        };
        self.insert(flag);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnionKind {
    TypingUnion,
    PEP604,
}

fn generate_pep604_fix(
    checker: &Checker,
    nodes: Vec<&Expr>,
    annotation: &Expr,
    applicability: Applicability,
) -> Fix {
    debug_assert!(nodes.len() >= 2);
    let new_expr = nodes
        .into_iter()
        .fold(None, |acc: Option<Expr>, right: &Expr| {
            if let Some(left) = acc {
                Some(Expr::BinOp(ExprBinOp {
                    left: Box::new(left),
                    op: Operator::BitOr,
                    right: Box::new(right.clone()),
                    range: TextRange::default(),
                    node_index: ruff_python_ast::AtomicNodeIndex::NONE,
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

fn fix_starts_with_none_none(nodes: &[&Expr]) -> bool {
    nodes.len() >= 2 && nodes.iter().take(2).all(|node| node.is_none_literal_expr())
}