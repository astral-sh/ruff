use std::collections::HashMap;

use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    visitor::{self, Visitor},
    Arguments, ElifElseClause, Expr, ExprAttribute, ExprCall, ExprName, ExprSubscript, Operator,
    Stmt, StmtAssign, StmtAugAssign, StmtBreak, StmtDelete, StmtFor, StmtIf,
};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use ruff_diagnostics::Diagnostic;

fn is_mutating_function(function_name: &str) -> bool {
    matches!(
        function_name,
        "append"
            | "sort"
            | "reverse"
            | "remove"
            | "clear"
            | "extend"
            | "insert"
            | "pop"
            | "popitem"
            | "setdefault"
            | "update"
            | "intersection_update"
            | "difference_update"
            | "symmetric_difference_update"
            | "add"
            | "discard"
    )
}
/// ## What it does
/// Checks for mutation of the iterator of a loop in the loop's body
///
/// ## Why is this bad?
/// Changing the structure that is being iterated over will usually lead to
/// unintended behavior as not all elements will be addressed.
///
/// ## Example
/// ```python
/// some_list = [1,2,3]
/// for i in some_list:
///   some_list.remove(i) # this will lead to not all elements being printed
///   print(i)
/// ```
///
///
/// ## References
/// - [Python documentation: Mutable Sequence Types](https://docs.python.org/3/library/stdtypes.html#typesseq-mutable)
#[violation]
pub struct LoopIteratorMutation;

impl Violation for LoopIteratorMutation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("editing a loop's mutable iterable often leads to unexpected results/bugs")
    }
}

fn _to_name_str(node: &Expr) -> String {
    match node {
        Expr::Name(ExprName { id, .. }) => {
            return id.to_string();
        }
        Expr::Attribute(ExprAttribute {
            range: _,
            value,
            attr,
            ..
        }) => {
            let mut inner = _to_name_str(value);
            match inner.as_str() {
                "" => {
                    return "".into();
                }
                _ => {
                    inner.push_str(".");
                    inner.push_str(attr);
                    return inner;
                }
            }
        }
        Expr::Call(ExprCall { range: _, func, .. }) => {
            return _to_name_str(func);
        }
        _ => {
            return "".into();
        }
    }
}
// B909
pub(crate) fn loop_iterator_mutation(checker: &mut Checker, stmt_for: &StmtFor) {
    let StmtFor {
        target: _,
        iter,
        body,
        orelse: _,
        is_async: _,
        range: _,
    } = stmt_for;
    let name;

    match iter.as_ref() {
        Expr::Name(ExprName { .. }) => {
            name = _to_name_str(iter.as_ref());
        }
        Expr::Attribute(ExprAttribute { .. }) => {
            name = _to_name_str(iter.as_ref());
        }
        _ => {
            unreachable!()
        }
    }
    let mut visitor = LoopMutationsVisitor {
        name: &name,
        mutations: HashMap::new(),
        _contidional_block: 0,
    };
    visitor.visit_body(body);
    for mutation in visitor.mutations.values().flatten() {
        checker
            .diagnostics
            .push(Diagnostic::new(LoopIteratorMutation, *mutation));
    }
}

struct LoopMutationsVisitor<'a> {
    name: &'a str,
    mutations: HashMap<u8, Vec<TextRange>>,
    _contidional_block: u8,
}

impl<'a> LoopMutationsVisitor<'a> {
    fn add_mutation(&mut self, range: &TextRange) {
        if !self.mutations.contains_key(&self._contidional_block) {
            self.mutations.insert(self._contidional_block, Vec::new());
        }
        match self.mutations.get_mut(&self._contidional_block) {
            Some(a) => a.push(*range),
            None => {}
        }
    }

    fn handle_delete(&mut self, range: &TextRange, targets: &'a Vec<Expr>) {
        for target in targets {
            let name;
            match target {
                Expr::Subscript(ExprSubscript {
                    range: _,
                    value,
                    slice: _,
                    ctx: _,
                }) => {
                    name = _to_name_str(value);
                }

                Expr::Attribute(_) | Expr::Name(_) => {
                    name = String::new(); // ignore reference deletion
                }
                _ => {
                    name = String::new();
                    visitor::walk_expr(self, target);
                }
            }
            if self.name.eq(&name) {
                self.add_mutation(range);
            }
        }
    }

    fn handle_assign(&mut self, range: &TextRange, targets: &'a Vec<Expr>, _value: &Box<Expr>) {
        for target in targets {
            match target {
                Expr::Subscript(ExprSubscript {
                    range: _,
                    value,
                    slice: _,
                    ctx: _,
                }) => {
                    if self.name.eq(&_to_name_str(value)) {
                        self.add_mutation(range)
                    }
                }
                _ => visitor::walk_expr(self, target),
            }
        }
    }

    fn handle_aug_assign(
        &mut self,
        range: &TextRange,
        target: &Box<Expr>,
        _op: &Operator,
        _value: &Box<Expr>,
    ) {
        if self.name.eq(&_to_name_str(target)) {
            self.add_mutation(range)
        }
    }

    fn handle_call(&mut self, _range: &TextRange, func: &Box<Expr>, _arguments: &Arguments) {
        match func.as_ref() {
            Expr::Attribute(ExprAttribute {
                range,
                value,
                attr,
                ctx: _,
            }) => {
                let name = _to_name_str(value);
                if self.name.eq(&name) && is_mutating_function(&attr.as_str()) {
                    self.add_mutation(range);
                }
            }
            _ => {}
        }
    }

    fn handle_if(
        &mut self,
        _range: &TextRange,
        _test: &Box<Expr>,
        body: &'a Vec<Stmt>,
        _elif_else_clauses: &Vec<ElifElseClause>,
    ) {
        self._contidional_block += 1;
        self.visit_body(body);
        self._contidional_block += 1;
    }
}
/// `Visitor` to collect all used identifiers in a statement.
impl<'a> Visitor<'a> for LoopMutationsVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Delete(StmtDelete { range, targets }) => self.handle_delete(range, targets),
            Stmt::Assign(StmtAssign {
                range,
                targets,
                value,
            }) => self.handle_assign(range, targets, value),
            Stmt::AugAssign(StmtAugAssign {
                range,
                target,
                op,
                value,
            }) => {
                self.handle_aug_assign(range, target, op, value);
            }
            Stmt::If(StmtIf {
                range,
                test,
                body,
                elif_else_clauses,
            }) => self.handle_if(range, test, body, elif_else_clauses),
            Stmt::Break(StmtBreak { range: _ }) => {
                match self.mutations.get_mut(&self._contidional_block) {
                    Some(a) => a.clear(),
                    None => {}
                }
            }
            _ => {
                visitor::walk_stmt(self, stmt);
            }
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Call(ExprCall {
                range,
                func,
                arguments,
            }) => self.handle_call(range, func, arguments),
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }
}
