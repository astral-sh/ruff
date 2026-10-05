use std::cell::RefCell;
use std::convert::Infallible;

use ruff_python_ast::helpers::{ExpressionSearchFrame, any_over_expr};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_parser::parse_expression;

use super::{
    ExpressionSearchCursor, ExpressionSearchVisit, OrdinaryExpressionSearchEffects,
    SynchronousExpressionSearchEffects, contains_string_literal, contains_string_literal_sync,
};

fn label(expression: &Expr) -> &str {
    match expression {
        Expr::BoolOp(_) => "bool-op",
        Expr::Named(_) => "named",
        Expr::BinOp(_) => "binary",
        Expr::UnaryOp(_) => "unary",
        Expr::Lambda(_) => "lambda",
        Expr::If(_) => "if",
        Expr::Dict(_) => "dict",
        Expr::Set(_) => "set",
        Expr::ListComp(_) => "list-comp",
        Expr::SetComp(_) => "set-comp",
        Expr::DictComp(_) => "dict-comp",
        Expr::Generator(_) => "generator",
        Expr::Await(_) => "await",
        Expr::Yield(_) => "yield",
        Expr::YieldFrom(_) => "yield-from",
        Expr::Compare(_) => "compare",
        Expr::Call(_) => "call",
        Expr::FString(_) => "f-string",
        Expr::TString(_) => "t-string",
        Expr::StringLiteral(_) => "string",
        Expr::BytesLiteral(_) => "bytes",
        Expr::NumberLiteral(_) => "number",
        Expr::BooleanLiteral(_) => "boolean",
        Expr::NoneLiteral(_) => "none",
        Expr::EllipsisLiteral(_) => "ellipsis",
        Expr::Attribute(_) => "attribute",
        Expr::Subscript(_) => "subscript",
        Expr::Starred(_) => "starred",
        Expr::Name(name) => name.id.as_str(),
        Expr::List(_) => "list",
        Expr::Tuple(_) => "tuple",
        Expr::Slice(_) => "slice",
        Expr::IpyEscapeCommand(_) => "ipython",
    }
}

#[derive(Default)]
struct Traversal<'ast> {
    found: bool,
    trace: Vec<&'ast str>,
    steps: usize,
    max_parents: usize,
    max_capacity: usize,
}

fn traverse(initial: ExpressionSearchFrame<'_>, stop_at: usize) -> Traversal<'_> {
    let mut cursor = ExpressionSearchCursor::new(initial);
    let mut result = Traversal::default();
    loop {
        result.steps += 1;
        let step = cursor.commit(cursor.plan(), 0);
        let (parents, capacity) = cursor.storage();
        result.max_parents = result.max_parents.max(parents);
        result.max_capacity = result.max_capacity.max(capacity);
        match step {
            Some(ExpressionSearchVisit::Visit(expression)) => {
                result.trace.push(label(expression));
                if result.trace.len() == stop_at {
                    result.found = true;
                    return result;
                }
            }
            Some(ExpressionSearchVisit::Progress) => {}
            None => return result,
        }
    }
}

fn assert_traces(expression: &Expr, expected: &[&str]) {
    for stop_at in 0..=expected.len() {
        let prefix = if stop_at == 0 {
            expected
        } else {
            &expected[..stop_at]
        };
        let mut ordinary = Vec::new();
        let found = any_over_expr(expression, |expression| {
            ordinary.push(label(expression).to_owned());
            ordinary.len() == stop_at
        });
        assert_eq!(ordinary, prefix);
        assert_eq!(found, stop_at != 0);

        let cursor = traverse(ExpressionSearchFrame::expression(expression), stop_at);
        assert_eq!(cursor.trace, prefix);
        assert_eq!(cursor.found, stop_at != 0);
    }
}

#[test]
fn expression_variants_preserve_preorder_and_every_first_match() -> anyhow::Result<()> {
    let fixtures: &[(&str, &[&str])] = &[
        ("a and b and c", &["bool-op", "a", "b", "c"]),
        ("(a := b)", &["named", "a", "b"]),
        ("a + b", &["binary", "a", "b"]),
        ("-a", &["unary", "a"]),
        (
            "lambda a, b=posonly, /, c=positional, *args, d, e=keyword, f, **kwargs: body",
            &["lambda", "posonly", "positional", "keyword", "body"],
        ),
        ("lambda: body", &["lambda", "body"]),
        (
            "body if condition else other",
            &["if", "condition", "body", "other"],
        ),
        (
            "{key: value, **spread, last_key: last_value}",
            &["dict", "value", "key", "spread", "last_value", "last_key"],
        ),
        ("{a, b}", &["set", "a", "b"]),
        (
            "[elt for target in iterable if condition for other in other_iter if other_condition]",
            &[
                "list-comp",
                "elt",
                "target",
                "iterable",
                "condition",
                "other",
                "other_iter",
                "other_condition",
            ],
        ),
        (
            "{elt for target in iterable if condition}",
            &["set-comp", "elt", "target", "iterable", "condition"],
        ),
        (
            "{key: value for target in iterable if condition}",
            &[
                "dict-comp",
                "key",
                "value",
                "target",
                "iterable",
                "condition",
            ],
        ),
        (
            "(elt for target in iterable if condition)",
            &["generator", "elt", "target", "iterable", "condition"],
        ),
        ("await a", &["await", "a"]),
        ("(yield a)", &["yield", "a"]),
        ("(yield)", &["yield"]),
        ("(yield from a)", &["yield-from", "a"]),
        ("a < b < c", &["compare", "a", "b", "c"]),
        (
            "function(a, keyword=kw, *spread, second=other, **rest)",
            &[
                "call", "function", "a", "starred", "spread", "kw", "other", "rest",
            ],
        ),
        (
            r#"f"text {value:{width}.{precision}} end" f"" "plain" f"{'actual'}""#,
            &["f-string", "value", "width", "precision", "string"],
        ),
        (
            r#"t"text {value:{width}.{precision}} end" t"" t"{'actual'}""#,
            &["t-string", "value", "width", "precision", "string"],
        ),
        (r#"f"{value:{'spec'}}""#, &["f-string", "value", "string"]),
        ("'a' 'b'", &["string"]),
        ("b'a' b'b'", &["bytes"]),
        ("123", &["number"]),
        ("True", &["boolean"]),
        ("None", &["none"]),
        ("...", &["ellipsis"]),
        ("value.attr", &["attribute", "value"]),
        (
            "value[lower:upper:step]",
            &["subscript", "value", "slice", "lower", "upper", "step"],
        ),
        ("value[::]", &["subscript", "value", "slice"]),
        ("a", &["a"]),
        ("[a, b]", &["list", "a", "b"]),
        ("(a, b)", &["tuple", "a", "b"]),
    ];
    for (source, expected) in fixtures {
        let parsed = parse_expression(source)?;
        assert_traces(parsed.expr(), expected);
    }

    assert_traces(
        &Expr::IpyEscapeCommand(ast::ExprIpyEscapeCommand {
            node_index: ast::AtomicNodeIndex::NONE,
            range: Default::default(),
            kind: ast::IpyEscapeKind::Shell,
            value: "command".into(),
        }),
        &["ipython"],
    );

    let mut dict = parse_expression("{key: value for target in iterable}")?.into_expr();
    let Expr::DictComp(comprehension) = &mut dict else {
        anyhow::bail!("expected a dictionary comprehension");
    };
    comprehension.key = None;
    assert_traces(&dict, &["dict-comp", "value", "target", "iterable"]);
    Ok(())
}

#[derive(Default)]
struct TraceEffects(RefCell<Vec<String>>);

impl SynchronousExpressionSearchEffects for TraceEffects {
    type Error = Infallible;

    fn start<'ast>(
        &self,
        expressions: &'ast [Expr],
    ) -> Result<ExpressionSearchCursor<'ast>, Self::Error> {
        OrdinaryExpressionSearchEffects.start(expressions)
    }

    fn next<'ast>(
        &self,
        cursor: &mut ExpressionSearchCursor<'ast>,
    ) -> Result<Option<ExpressionSearchVisit<'ast>>, Self::Error> {
        OrdinaryExpressionSearchEffects.next(cursor)
    }

    fn is_string_literal(&self, expression: &Expr) -> Result<bool, Self::Error> {
        self.0.borrow_mut().push(label(expression).to_owned());
        OrdinaryExpressionSearchEffects.is_string_literal(expression)
    }

    fn finish(
        &self,
        cursor: &mut ExpressionSearchCursor<'_>,
        found: bool,
    ) -> Result<bool, Self::Error> {
        OrdinaryExpressionSearchEffects.finish(cursor, found)
    }
}

#[test]
fn shared_search_visits_bases_in_order_and_stops_at_real_string_literals() -> anyhow::Result<()> {
    let fixtures: &[(&[&str], bool, &[&str])] = &[
        (&[], false, &[]),
        (
            &[r#"f"literal text {value}""#, r#"t"more text {other}""#],
            false,
            &["f-string", "value", "t-string", "other"],
        ),
        (
            &[r#"f"{value:{'spec'}}""#, "unvisited()"],
            true,
            &["f-string", "value", "string"],
        ),
        (
            &["first", "Base['forward', unvisited]", "later"],
            true,
            &["first", "subscript", "Base", "tuple", "string"],
        ),
    ];
    for (sources, expected, trace) in fixtures {
        let expressions = sources
            .iter()
            .map(|source| parse_expression(source).map(|parsed| parsed.into_expr()))
            .collect::<Result<Vec<_>, _>>()?;
        let effects = TraceEffects::default();
        let Ok(found) = contains_string_literal_sync(&expressions, &effects);
        assert_eq!(found, *expected);
        assert_eq!(effects.0.into_inner(), *trace);
        assert_eq!(contains_string_literal(&expressions), *expected);
        assert_eq!(
            expressions
                .iter()
                .any(|expression| any_over_expr(expression, Expr::is_string_literal_expr)),
            *expected,
        );
    }
    Ok(())
}

fn name(id: &str) -> Expr {
    Expr::Name(ast::ExprName {
        node_index: ast::AtomicNodeIndex::NONE,
        range: Default::default(),
        id: id.into(),
        ctx: ast::ExprContext::Load,
    })
}

fn interpolation(
    format_spec: Option<Box<ast::InterpolatedStringFormatSpec>>,
) -> ast::InterpolatedStringElement {
    ast::InterpolatedElement {
        range: Default::default(),
        node_index: ast::AtomicNodeIndex::NONE,
        expression: Box::new(name("body")),
        debug_text: None,
        conversion: ast::ConversionFlag::None,
        format_spec,
    }
    .into()
}

#[test]
fn skipped_members_each_require_a_transition() -> anyhow::Result<()> {
    const EXTRA: usize = 256;
    for family in [
        "defaults",
        "literal parts",
        "empty f-strings",
        "empty t-strings",
    ] {
        let mut results = Vec::new();
        for width in [1, EXTRA + 1] {
            let source = match family {
                "defaults" => format!(
                    "lambda {}: body",
                    (0..width)
                        .map(|index| format!("p{index}"))
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                "literal parts" => format!(r#"{} f"{{body}}""#, r#""literal" "#.repeat(width)),
                "empty f-strings" => format!(r#"{} f"{{body}}""#, r#"f"" "#.repeat(width)),
                _ => format!(r#"{} t"{{body}}""#, r#"t"" "#.repeat(width)),
            };
            let parsed = parse_expression(&source)?;
            let result = traverse(ExpressionSearchFrame::expression(parsed.expr()), 2);
            assert_eq!(result.trace.last(), Some(&"body"));
            assert_eq!(result.trace.len(), 2);
            results.push((result.steps, result.max_parents));
        }
        assert!(results[1].0 >= results[0].0 + EXTRA, "{family}");
        assert_eq!(results[0].1, results[1].1, "{family}");
    }

    let literal = ast::InterpolatedStringLiteralElement {
        range: Default::default(),
        node_index: ast::AtomicNodeIndex::NONE,
        value: "text".into(),
    };
    let mut results = Vec::new();
    for width in [1, EXTRA + 1] {
        let mut elements = vec![ast::InterpolatedStringElement::Literal(literal.clone()); width];
        elements.push(interpolation(None));
        let expression = Expr::from(ast::FString {
            range: Default::default(),
            node_index: ast::AtomicNodeIndex::NONE,
            elements: elements.into(),
            flags: ast::FStringFlags::empty(),
        });
        let result = traverse(ExpressionSearchFrame::expression(&expression), 2);
        assert_eq!(result.trace, ["f-string", "body"]);
        results.push((result.steps, result.max_parents));
    }
    assert!(results[1].0 >= results[0].0 + EXTRA);
    assert_eq!(results[0].1, results[1].1);
    Ok(())
}

#[test]
fn wide_siblings_do_not_grow_the_frame_stack() {
    let mut storage = Vec::new();
    for width in [1, 8192] {
        let expression = Expr::List(ast::ExprList {
            node_index: ast::AtomicNodeIndex::NONE,
            range: Default::default(),
            elts: (0..width).map(|_| name("sibling")).collect(),
            ctx: ast::ExprContext::Load,
        });
        let result = traverse(ExpressionSearchFrame::expression(&expression), 0);
        assert_eq!(result.trace.len(), width + 1);
        assert_eq!(result.trace[0], "list");
        assert!(result.trace[1..].iter().all(|label| *label == "sibling"));
        storage.push((result.max_parents, result.max_capacity));

        let mut visits = 0;
        assert!(!any_over_expr(&expression, |_| {
            visits += 1;
            false
        }));
        assert_eq!(visits, width + 1);
    }
    assert_eq!(storage[0], storage[1]);
}

struct DeepUnary(Expr);

impl Drop for DeepUnary {
    fn drop(&mut self) {
        let mut expression = std::mem::replace(&mut self.0, Expr::NoneLiteral(Default::default()));
        while let Expr::UnaryOp(unary) = expression {
            expression = *unary.operand;
        }
    }
}

struct DeepInterpolation(ast::InterpolatedStringElement);

impl Drop for DeepInterpolation {
    fn drop(&mut self) {
        let mut next = self
            .0
            .as_interpolation_mut()
            .and_then(|element| element.format_spec.take());
        while let Some(mut spec) = next {
            next = spec
                .elements
                .first_mut()
                .and_then(ast::InterpolatedStringElement::as_interpolation_mut)
                .and_then(|element| element.format_spec.take());
        }
    }
}

#[test]
fn deep_borrowed_frames_step_and_drop_without_recursive_ast_work() {
    const DEPTH: usize = 8192;
    let mut unary = DeepUnary(name("body"));
    let mut format = DeepInterpolation(interpolation(None));
    for _ in 0..DEPTH {
        let operand = std::mem::replace(&mut unary.0, Expr::NoneLiteral(Default::default()));
        unary.0 = Expr::UnaryOp(ast::ExprUnaryOp {
            node_index: ast::AtomicNodeIndex::NONE,
            range: Default::default(),
            op: ast::UnaryOp::Not,
            operand: Box::new(operand),
        });
        let element = std::mem::replace(&mut format.0, interpolation(None));
        format.0 = interpolation(Some(Box::new(ast::InterpolatedStringFormatSpec {
            range: Default::default(),
            node_index: ast::AtomicNodeIndex::NONE,
            elements: vec![element].into(),
        })));
    }

    // The fixture guards also dismantle the AST iteratively if an assertion fails.
    for initial in [
        ExpressionSearchFrame::expression(&unary.0),
        ExpressionSearchFrame::interpolated_element(&format.0),
    ] {
        let copied = initial;
        let result = traverse(copied, 0);
        assert_eq!(result.trace.len(), DEPTH + 1);
        assert_eq!(result.trace.last(), Some(&"body"));
        assert!(result.max_parents >= DEPTH);
        assert!(result.steps <= DEPTH * 16 + 32);

        let mut suspended = ExpressionSearchCursor::new(initial);
        while suspended.storage().0 < DEPTH {
            assert!(suspended.commit(suspended.plan(), 0).is_some());
        }
        drop(suspended);
    }
}
