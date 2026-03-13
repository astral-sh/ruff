//! Tracks the percentage of expressions with a dynamic or `Todo` type,
//! as a measure of ty's type-inference coverage.

use crate::types::Type;
use crate::types::visitor::any_over_type;
use crate::{Db, HasType, SemanticModel};
use ruff_db::{files::File, parsed::parsed_module};
use ruff_python_ast::{
    self as ast, visitor::source_order, visitor::source_order::SourceOrderVisitor,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct CoverageStats {
    pub total: u64,

    pub dynamic: u64,

    pub todo: u64,
}

impl CoverageStats {
    #[must_use]
    pub fn merge(self, other: CoverageStats) -> CoverageStats {
        CoverageStats {
            total: self.total + other.total,
            dynamic: self.dynamic + other.dynamic,
            todo: self.todo + other.todo,
        }
    }

    #[expect(clippy::cast_precision_loss)]
    pub fn dynamic_percentage(&self) -> Option<f64> {
        if self.total == 0 {
            None
        } else {
            Some(self.dynamic as f64 / self.total as f64 * 100.0)
        }
    }

    #[expect(clippy::cast_precision_loss)]
    pub fn todo_percentage(&self) -> Option<f64> {
        if self.total == 0 {
            None
        } else {
            Some(self.todo as f64 / self.total as f64 * 100.0)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeCoverage {
    Known,
    Dynamic,
    Todo,
}

fn classify(db: &dyn Db, ty: Type<'_>) -> TypeCoverage {
    if any_over_type(db, ty, true, |t: Type<'_>| t.is_todo()) {
        return TypeCoverage::Todo;
    }
    if any_over_type(db, ty, true, |t: Type<'_>| t.is_dynamic()) {
        return TypeCoverage::Dynamic;
    }
    TypeCoverage::Known
}

pub fn coverage_types(db: &dyn Db, file: File) -> CoverageStats {
    let mut visitor = CoverageVisitor::new(db, file);
    let ast = parsed_module(db, file).load(db);
    visitor.visit_body(ast.suite());
    visitor.stats
}

struct CoverageVisitor<'db> {
    db: &'db dyn Db,
    model: SemanticModel<'db>,
    stats: CoverageStats,
}

impl<'db> CoverageVisitor<'db> {
    fn new(db: &'db dyn Db, file: File) -> Self {
        Self {
            db,
            model: SemanticModel::new(db, file),
            stats: CoverageStats::default(),
        }
    }

    fn record(&mut self, ty: Option<Type<'db>>) {
        let Some(ty) = ty else {
            return;
        };

        self.stats.total += 1;

        match classify(self.db, ty) {
            TypeCoverage::Todo => {
                self.stats.todo += 1;
            }
            TypeCoverage::Dynamic => {
                self.stats.dynamic += 1;
            }
            TypeCoverage::Known => {}
        }
    }

    fn visit_target(&mut self, target: &ast::Expr) {
        match target {
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                for element in elts {
                    self.visit_target(element);
                }
            }
            _ => self.visit_expr(target),
        }
    }
}

impl SourceOrderVisitor<'_> for CoverageVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        // Record types for statements that introduce named bindings directly,
        // then delegate to walk_stmt for child traversal.
        match stmt {
            ast::Stmt::FunctionDef(function) => {
                self.record(function.inferred_type(&self.model));
            }
            ast::Stmt::ClassDef(class) => {
                self.record(class.inferred_type(&self.model));
            }
            // For assignment targets, use visit_target to handle unpacking correctly.
            ast::Stmt::Assign(assign) => {
                for target in &assign.targets {
                    self.visit_target(target);
                }
                self.visit_expr(&assign.value);
                return;
            }
            ast::Stmt::For(for_stmt) => {
                self.visit_target(&for_stmt.target);
                self.visit_expr(&for_stmt.iter);
                self.visit_body(&for_stmt.body);
                self.visit_body(&for_stmt.orelse);
                return;
            }
            ast::Stmt::With(with_stmt) => {
                for item in &with_stmt.items {
                    if let Some(target) = &item.optional_vars {
                        self.visit_target(target);
                    }
                    self.visit_expr(&item.context_expr);
                }
                self.visit_body(&with_stmt.body);
                return;
            }
            ast::Stmt::AnnAssign(_)
            | ast::Stmt::Return(_)
            | ast::Stmt::Delete(_)
            | ast::Stmt::AugAssign(_)
            | ast::Stmt::TypeAlias(_)
            | ast::Stmt::While(_)
            | ast::Stmt::If(_)
            | ast::Stmt::Match(_)
            | ast::Stmt::Raise(_)
            | ast::Stmt::Try(_)
            | ast::Stmt::Assert(_)
            | ast::Stmt::Import(_)
            | ast::Stmt::ImportFrom(_)
            | ast::Stmt::Global(_)
            | ast::Stmt::Nonlocal(_)
            | ast::Stmt::Expr(_)
            | ast::Stmt::Pass(_)
            | ast::Stmt::Break(_)
            | ast::Stmt::Continue(_)
            | ast::Stmt::IpyEscapeCommand(_) => {}
        }

        source_order::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &ast::Expr) {
        self.record(expr.inferred_type(&self.model));
        source_order::walk_expr(self, expr);
    }

    // Overridden to use visit_target for the comprehension variable, which handles
    // unpacking assignments correctly rather than treating them as plain expressions.
    fn visit_comprehension(&mut self, comprehension: &ast::Comprehension) {
        self.visit_expr(&comprehension.iter);
        self.visit_target(&comprehension.target);
        for if_expr in &comprehension.ifs {
            self.visit_expr(if_expr);
        }
    }

    fn visit_parameter(&mut self, parameter: &ast::Parameter) {
        self.record(parameter.inferred_type(&self.model));
        source_order::walk_parameter(self, parameter);
    }

    fn visit_alias(&mut self, alias: &ast::Alias) {
        self.record(alias.inferred_type(&self.model));
        source_order::walk_alias(self, alias);
    }
}
