use std::fmt::{Debug, Write};

use insta::assert_snapshot;
use ruff_python_ast as ast;
use ruff_python_parser::lexer::lex;
use ruff_python_parser::{parse_tokens, Mode};

use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::visitor::{
    walk_alias, walk_comprehension, walk_except_handler, walk_expr, walk_keyword, walk_match_case,
    walk_parameter, walk_parameters, walk_pattern, walk_stmt, walk_type_param, walk_with_item,
    Visitor,
};
use ruff_python_ast::{
    Alias, BoolOp, CmpOp, Comprehension, ExceptHandler, Expr, Keyword, MatchCase, Operator,
    Parameter, Parameters, Pattern, Stmt, TypeParam, UnaryOp, WithItem,
};

#[test]
fn function_arguments() {
    let source = r#"def a(b, c,/, d, e = 20, *args, named=5, other=20, **kwargs): pass"#;

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn function_positional_only_with_default() {
    let source = r#"def a(b, c = 34,/, e = 20, *args): pass"#;

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn compare() {
    let source = r#"4 < x < 5"#;

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
    let source = r#"
match x:
    case Point2D(0, 0):
        ...
    case Point3D(x=0, y=0, z=0):
        ...
"#;

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn decorators() {
    let source = r#"
@decorator
def a():
    pass

@test
class A:
    pass
"#;

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn type_aliases() {
    let source = r#"type X[T: str, U, *Ts, **P] = list[T]"#;

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn class_type_parameters() {
    let source = r#"class X[T: str, U, *Ts, **P]: ..."#;

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn function_type_parameters() {
    let source = r#"def X[T: str, U, *Ts, **P](): ..."#;

    let trace = trace_visitation(source);

    assert_snapshot!(trace);
}

fn trace_visitation(source: &str) -> String {
    let tokens = lex(source, Mode::Module);
    let parsed = parse_tokens(tokens, Mode::Module, "test.py").unwrap();

    let mut visitor = RecordVisitor::default();
    walk_module(&mut visitor, &parsed);

    visitor.output
}

fn walk_module<'a, V>(visitor: &mut V, module: &'a ast::Mod)
where
    V: Visitor<'a> + ?Sized,
{
    match module {
        ast::Mod::Module(ast::ModModule { body, range: _ }) => {
            visitor.visit_body(body);
        }
        ast::Mod::Expression(ast::ModExpression { body, range: _ }) => visitor.visit_expr(body),
    }
}

/// Emits a `tree` with a node for every visited AST node (labelled by the AST node's kind)
/// and leafs for attributes.
#[derive(Default)]
struct RecordVisitor {
    depth: usize,
    output: String,
}

impl RecordVisitor {
    fn enter_node<'a, T>(&mut self, node: T)
    where
        T: Into<AnyNodeRef<'a>>,
    {
        self.emit(&node.into().kind());
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

impl Visitor<'_> for RecordVisitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        self.enter_node(stmt);
        walk_stmt(self, stmt);
        self.exit_node();
    }

    fn visit_annotation(&mut self, expr: &Expr) {
        self.enter_node(expr);
        walk_expr(self, expr);
        self.exit_node();
    }

    fn visit_expr(&mut self, expr: &Expr) {
        self.enter_node(expr);
        walk_expr(self, expr);
        self.exit_node();
    }

    fn visit_bool_op(&mut self, bool_op: &BoolOp) {
        self.emit(&bool_op);
    }

    fn visit_operator(&mut self, operator: &Operator) {
        self.emit(&operator);
    }

    fn visit_unary_op(&mut self, unary_op: &UnaryOp) {
        self.emit(&unary_op);
    }

    fn visit_cmp_op(&mut self, cmp_op: &CmpOp) {
        self.emit(&cmp_op);
    }

    fn visit_comprehension(&mut self, comprehension: &Comprehension) {
        self.enter_node(comprehension);
        walk_comprehension(self, comprehension);
        self.exit_node();
    }

    fn visit_except_handler(&mut self, except_handler: &ExceptHandler) {
        self.enter_node(except_handler);
        walk_except_handler(self, except_handler);
        self.exit_node();
    }

    fn visit_format_spec(&mut self, format_spec: &Expr) {
        self.enter_node(format_spec);
        walk_expr(self, format_spec);
        self.exit_node();
    }

    fn visit_parameters(&mut self, parameters: &Parameters) {
        self.enter_node(parameters);
        walk_parameters(self, parameters);
        self.exit_node();
    }

    fn visit_parameter(&mut self, parameter: &Parameter) {
        self.enter_node(parameter);
        walk_parameter(self, parameter);
        self.exit_node();
    }

    fn visit_keyword(&mut self, keyword: &Keyword) {
        self.enter_node(keyword);
        walk_keyword(self, keyword);
        self.exit_node();
    }

    fn visit_alias(&mut self, alias: &Alias) {
        self.enter_node(alias);
        walk_alias(self, alias);
        self.exit_node();
    }

    fn visit_with_item(&mut self, with_item: &WithItem) {
        self.enter_node(with_item);
        walk_with_item(self, with_item);
        self.exit_node();
    }

    fn visit_match_case(&mut self, match_case: &MatchCase) {
        self.enter_node(match_case);
        walk_match_case(self, match_case);
        self.exit_node();
    }

    fn visit_pattern(&mut self, pattern: &Pattern) {
        self.enter_node(pattern);
        walk_pattern(self, pattern);
        self.exit_node();
    }

    fn visit_type_param(&mut self, type_param: &TypeParam) {
        self.enter_node(type_param);
        walk_type_param(self, type_param);
        self.exit_node();
    }
}
