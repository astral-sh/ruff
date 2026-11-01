use std::fmt::{Debug, Write};

use insta::assert_snapshot;

use ruff_python_ast::helpers::any_over_body;
use ruff_python_ast::visitor::{
    Visitor, walk_alias, walk_bytes_literal, walk_comprehension, walk_except_handler, walk_expr,
    walk_f_string, walk_interpolated_string_element, walk_keyword, walk_match_case, walk_parameter,
    walk_parameters, walk_pattern, walk_stmt, walk_string_literal, walk_t_string, walk_type_param,
    walk_with_item,
};
use ruff_python_ast::{
    Alias, AstNode, BoolOp, BytesLiteral, CmpOp, Comprehension, ExceptHandler, Expr, FString,
    InterpolatedStringElement, Keyword, MatchCase, Operator, Parameter, Parameters, Pattern, Stmt,
    StringLiteral, TString, TypeParam, UnaryOp, WithItem,
};
use ruff_python_parser::parse_module;
use ruff_text_size::Ranged;

#[test]
fn function_arguments() {
    let source = r"def a(b, c,/, d, e = 20, *args, named=5, other=20, **kwargs): pass";

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn function_positional_only_with_default() {
    let source = r"def a(b, c = 34,/, e = 20, *args): pass";

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn compare() {
    let source = r"4 < x < 5";

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn list_comprehension() {
    let source = "[x for x in numbers]";

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn dict_comprehension() {
    let source = "{x: x**2 for x in numbers}";

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn set_comprehension() {
    let source = "{x for x in numbers}";

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn match_class_pattern() {
    let source = r"
match x:
    case Point2D(0, 0):
        ...
    case Point3D(x=0, y=0, z=0):
        ...
";

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn decorators() {
    let source = r"
@decorator
def a():
    pass

@test
class A:
    pass
";

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn type_aliases() {
    let source = r"type X[T: str, U, *Ts, **P] = list[T]";

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn find_expressions_in_type_parameters() {
    for (parameters, stop, expected) in [
        ("T, *Ts, **P", None, &["Alias", "Value"][..]),
        (
            "T: Bound, U = Default",
            None,
            &["Alias", "Bound", "Default", "Value"],
        ),
        ("T: Bound", Some("Bound"), &["Alias", "Bound"]),
        ("T = Default", Some("Default"), &["Alias", "Default"]),
        ("*Ts = Default", Some("Default"), &["Alias", "Default"]),
        ("**P = Default", Some("Default"), &["Alias", "Default"]),
    ] {
        let source = format!("type Alias[{parameters}] = Value");
        let parsed = parse_module(&source).unwrap();
        let mut seen = Vec::new();
        let found = any_over_body(parsed.suite(), |expr| {
            let text = &source[expr.range()];
            seen.push(text);
            Some(text) == stop
        });
        assert_eq!(seen, expected, "{source}");
        assert_eq!(found, stop.is_some(), "{source}");
    }
}

#[test]
fn class_type_parameters() {
    let source = r"class X[T: str, U, *Ts, **P]: ...";

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn function_type_parameters() {
    let source = r"def X[T: str, U, *Ts, **P](): ...";

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn string_literals() {
    let source = r"'a' 'b' 'c'";

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn bytes_literals() {
    let source = r"b'a' b'b' b'c'";

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn f_strings() {
    let source = r"'pre' f'foo {bar:.{x}f} baz'";

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn t_strings() {
    let source = r"t'pre' t'foo {bar:.{x}f} baz'";

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

fn trace_visitation(source: &str) -> String {
    let parsed = parse_module(source).unwrap();
    let mut visitor = RecordVisitor::default();
    visitor.visit_body(parsed.suite());
    visitor.output
}

/// Emits a `tree` with a node for every visited AST node (labelled by the AST node's kind)
/// and leaves for attributes.
#[derive(Default)]
struct RecordVisitor {
    depth: usize,
    output: String,
}

impl RecordVisitor {
    fn enter_node<'ast>(&mut self, node: impl AstNode<'ast>) {
        self.emit(&node.node_kind());
        self.depth += 1;
    }

    fn exit_node(&mut self) {
        self.depth -= 1;
    }

    fn emit(&mut self, text: &dyn Debug) {
        for _ in 0..self.depth {
            self.output.push_str("  ");
        }

        writeln!(self.output, "- {text:?}").unwrap();
    }
}

impl<'a> Visitor<'a> for RecordVisitor {
    fn visit_stmt(&mut self, stmt: Stmt<'a>) {
        self.enter_node(stmt);
        walk_stmt(self, stmt);
        self.exit_node();
    }

    fn visit_annotation(&mut self, expr: Expr<'a>) {
        self.enter_node(expr);
        walk_expr(self, expr);
        self.exit_node();
    }

    fn visit_expr(&mut self, expr: Expr<'a>) {
        self.enter_node(expr);
        walk_expr(self, expr);
        self.exit_node();
    }

    fn visit_bool_op(&mut self, bool_op: BoolOp) {
        self.emit(&bool_op);
    }

    fn visit_operator(&mut self, operator: Operator) {
        self.emit(&operator);
    }

    fn visit_unary_op(&mut self, unary_op: UnaryOp) {
        self.emit(&unary_op);
    }

    fn visit_cmp_op(&mut self, cmp_op: CmpOp) {
        self.emit(&cmp_op);
    }

    fn visit_comprehension(&mut self, comprehension: Comprehension<'a>) {
        self.enter_node(comprehension);
        walk_comprehension(self, comprehension);
        self.exit_node();
    }

    fn visit_except_handler(&mut self, except_handler: ExceptHandler<'a>) {
        self.enter_node(except_handler);
        walk_except_handler(self, except_handler);
        self.exit_node();
    }

    fn visit_parameters(&mut self, parameters: Parameters<'a>) {
        self.enter_node(parameters);
        walk_parameters(self, parameters);
        self.exit_node();
    }

    fn visit_parameter(&mut self, parameter: Parameter<'a>) {
        self.enter_node(parameter);
        walk_parameter(self, parameter);
        self.exit_node();
    }

    fn visit_keyword(&mut self, keyword: Keyword<'a>) {
        self.enter_node(keyword);
        walk_keyword(self, keyword);
        self.exit_node();
    }

    fn visit_alias(&mut self, alias: Alias<'a>) {
        self.enter_node(alias);
        walk_alias(self, alias);
        self.exit_node();
    }

    fn visit_with_item(&mut self, with_item: WithItem<'a>) {
        self.enter_node(with_item);
        walk_with_item(self, with_item);
        self.exit_node();
    }

    fn visit_match_case(&mut self, match_case: MatchCase<'a>) {
        self.enter_node(match_case);
        walk_match_case(self, match_case);
        self.exit_node();
    }

    fn visit_pattern(&mut self, pattern: Pattern<'a>) {
        self.enter_node(pattern);
        walk_pattern(self, pattern);
        self.exit_node();
    }

    fn visit_type_param(&mut self, type_param: TypeParam<'a>) {
        self.enter_node(type_param);
        walk_type_param(self, type_param);
        self.exit_node();
    }

    fn visit_string_literal(&mut self, string_literal: StringLiteral<'a>) {
        self.enter_node(string_literal);
        walk_string_literal(self, string_literal);
        self.exit_node();
    }

    fn visit_bytes_literal(&mut self, bytes_literal: BytesLiteral<'a>) {
        self.enter_node(bytes_literal);
        walk_bytes_literal(self, bytes_literal);
        self.exit_node();
    }

    fn visit_f_string(&mut self, f_string: FString<'a>) {
        self.enter_node(f_string);
        walk_f_string(self, f_string);
        self.exit_node();
    }

    fn visit_interpolated_string_element(
        &mut self,
        f_string_element: InterpolatedStringElement<'a>,
    ) {
        self.enter_node(f_string_element);
        walk_interpolated_string_element(self, f_string_element);
        self.exit_node();
    }

    fn visit_t_string(&mut self, t_string: TString<'a>) {
        self.enter_node(t_string);
        walk_t_string(self, t_string);
        self.exit_node();
    }
}
