//! Analysis of a module's `__all__` before semantic indexing.

use ruff_db::parsed::parsed_module;
use ruff_python_ast::flat::{self, fields as ast};
use ruff_python_ast::{
    Expr, Identifier, Stmt,
    flat::visitor::source_order::{
        SourceOrderVisitor, walk_expr as walk_source_order_expr,
        walk_stmt as walk_source_order_stmt,
    },
    name::Name,
};

use crate::{Db, ProgramFile};

/// Returns the elements of a list or tuple directly assigned to `__all__`.
fn literal_dunder_all_assignment(stmt: flat::View<'_, Stmt>) -> Option<flat::NodeList<'_, Expr>> {
    let (target, value) = match stmt.classify() {
        flat::StmtKind::Assign(stmt_assign) => {
            let targets = stmt_assign.targets();
            let value = stmt_assign.value();

            let [target] = targets.as_array()?;
            (target, value)
        }
        flat::StmtKind::AnnAssign(stmt_ann_assign) if let Some(value) = stmt_ann_assign.value() => {
            let target = stmt_ann_assign.target();
            (target, value)
        }
        _ => return None,
    };
    if !matches!(target.classify(), flat::ExprKind::Name(name) if name.id() == "__all__") {
        return None;
    }
    match value.fields() {
        ast::Expr::List(ast::ExprList { elts, .. })
        | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => Some(elts),
        _ => None,
    }
}

/// Returns a literal `__all__` without resolving imports or indexing the file.
///
/// This avoids expanding every name from a module that defines a small `__all__` after importing
/// a large namespace. Other uses of `__all__` fall back to collecting all potentially exported names.
#[salsa::tracked(returns(as_deref), heap_size=ruff_memory_usage::heap_size)]
pub fn static_dunder_all(db: &dyn Db, file: ProgramFile<'_>) -> Option<Box<[Name]>> {
    let module = parsed_module(db, file.python_file(db)).load(db);
    let (position, elements) =
        module
            .root()
            .body()
            .iter()
            .enumerate()
            .find_map(|(position, stmt)| {
                literal_dunder_all_assignment(stmt).map(|elements| (position, elements))
            })?;
    let mut names = elements
        .iter()
        .map(|element| {
            Some(Name::new(
                element.as_string_literal_expr()?.value().to_str(),
            ))
        })
        .collect::<Option<Vec<_>>>()?;

    for (index, stmt) in module.root().body().iter().enumerate() {
        let mut finder = UnknownDunderAllUse {
            initialized: index > position,
            found: false,
        };
        if index == position {
            if let flat::StmtKind::AnnAssign(assignment) = stmt.classify() {
                finder.visit_expr(assignment.annotation());
            }
        } else {
            finder.visit_stmt(stmt);
        }
        if finder.found {
            return None;
        }
    }

    names.sort_unstable();
    names.dedup();
    Some(names.into_boxed_slice())
}

/// Finds operations that cannot be evaluated by `static_dunder_all`.
struct UnknownDunderAllUse {
    initialized: bool,
    found: bool,
}

impl<'a> SourceOrderVisitor<'a> for UnknownDunderAllUse {
    fn visit_expr(&mut self, expr: flat::View<'a, Expr>) {
        if let flat::ExprKind::Name(name) = expr.classify()
            && name.id() == "__all__"
        {
            self.found = true;
        } else if !self.found {
            walk_source_order_expr(self, expr);
        }
    }

    fn visit_identifier(&mut self, identifier: flat::View<'a, Identifier>) {
        if identifier == "__all__" {
            self.found = true;
        }
    }

    fn visit_stmt(&mut self, stmt: flat::View<'a, Stmt>) {
        // A wildcard import can replace an existing `__all__`, but one before the literal
        // assignment is overwritten by that assignment.
        if self.initialized
            && let flat::StmtKind::ImportFrom(import) = stmt.classify()
            && import.names().iter().any(|alias| alias.name() == "*")
        {
            self.found = true;
        } else if !self.found {
            walk_source_order_stmt(self, stmt);
        }
    }
}
