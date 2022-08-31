use libcst_native::{CompoundStatement, Expression, SimpleStatementLine, Statement};
use libcst_native::{Expr, SmallStatement};

pub trait CSTVisitor {
    fn visit_statement(&mut self, statement: &Statement) {
        walk_statement(self, statement);
    }
    fn visit_simple_statement_line(&mut self, simple_statement_line: &SimpleStatementLine) {
        walk_simple_statement_line(self, simple_statement_line);
    }
    fn visit_compound_statement(&mut self, compound_statement: &CompoundStatement) {
        walk_compound_statement(self, compound_statement);
    }
    fn visit_small_statement(&mut self, small_statement: &SmallStatement) {
        walk_small_statement(self, small_statement);
    }
    fn visit_expression(&mut self, expression: &Expression) {
        walk_expression(self, expression);
    }
}

pub fn walk_statement<V: CSTVisitor + ?Sized>(visitor: &mut V, statement: &Statement) {
    match statement {
        Statement::Simple(simple_statement_line) => {
            visitor.visit_simple_statement_line(simple_statement_line)
        }
        Statement::Compound(compound_statement) => {
            visitor.visit_compound_statement(compound_statement)
        }
    }
}

pub fn walk_simple_statement_line<V: CSTVisitor + ?Sized>(
    visitor: &mut V,
    simple_statement_line: &SimpleStatementLine,
) {
    for small_statement in &simple_statement_line.body {
        visitor.visit_small_statement(small_statement);
    }
}

pub fn walk_compound_statement<V: CSTVisitor + ?Sized>(
    visitor: &mut V,
    compound_statement: &CompoundStatement,
) {
    match compound_statement {
        CompoundStatement::FunctionDef(_) => {}
        CompoundStatement::If(_) => {}
        CompoundStatement::For(_) => {}
        CompoundStatement::While(_) => {}
        CompoundStatement::ClassDef(_) => {}
        CompoundStatement::Try(_) => {}
        CompoundStatement::TryStar(_) => {}
        CompoundStatement::With(_) => {}
        CompoundStatement::Match(_) => {}
    }
}

pub fn walk_small_statement<V: CSTVisitor + ?Sized>(
    visitor: &mut V,
    small_statement: &SmallStatement,
) {
    match small_statement {
        SmallStatement::Pass(_) => {}
        SmallStatement::Break(_) => {}
        SmallStatement::Continue(_) => {}
        SmallStatement::Return(inner) => {
            if let Some(expression) = &inner.value {
                visitor.visit_expression(expression);
            }
        }
        SmallStatement::Expr(inner) => {
            visitor.visit_expression(&inner.value);
        }
        SmallStatement::Assert(inner) => {
            visitor.visit_expression(&inner.test);
            if let Some(expression) = &inner.msg {
                visitor.visit_expression(expression);
            }
        }
        SmallStatement::Import(_) => {}
        SmallStatement::ImportFrom(_) => {}
        SmallStatement::Assign(_) => {}
        SmallStatement::AnnAssign(_) => {}
        SmallStatement::Raise(_) => {}
        SmallStatement::Global(_) => {}
        SmallStatement::Nonlocal(_) => {}
        SmallStatement::AugAssign(_) => {}
        SmallStatement::Del(_) => {}
    }
}

pub fn walk_expression<V: CSTVisitor + ?Sized>(visitor: &mut V, expression: &Expression) {
    match expression {
        Expression::Name(_) => {}
        Expression::Ellipsis(_) => {}
        Expression::Integer(_) => {}
        Expression::Float(_) => {}
        Expression::Imaginary(_) => {}
        Expression::Comparison(_) => {}
        Expression::UnaryOperation(_) => {}
        Expression::BinaryOperation(_) => {}
        Expression::BooleanOperation(_) => {}
        Expression::Attribute(_) => {}
        Expression::Tuple(_) => {}
        Expression::Call(_) => {}
        Expression::GeneratorExp(_) => {}
        Expression::ListComp(_) => {}
        Expression::SetComp(_) => {}
        Expression::DictComp(_) => {}
        Expression::List(_) => {}
        Expression::Set(_) => {}
        Expression::Dict(_) => {}
        Expression::Subscript(_) => {}
        Expression::StarredElement(_) => {}
        Expression::IfExp(_) => {}
        Expression::Lambda(_) => {}
        Expression::Yield(_) => {}
        Expression::Await(_) => {}
        Expression::SimpleString(_) => {}
        Expression::ConcatenatedString(_) => {}
        Expression::FormattedString(_) => {}
        Expression::NamedExpr(_) => {}
    }
}
