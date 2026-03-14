//! Tracks the percentage of expressions with a dynamic or `Todo` type,
//! as a measure of ty's type-inference coverage.

use std::collections::HashMap;

use crate::types::Type;
use crate::types::visitor::any_over_type;
use crate::{Db, HasType, SemanticModel};
use ruff_db::source::{line_index, source_text};
use ruff_db::{files::File, parsed::parsed_module};
use ruff_python_ast::{
    self as ast, visitor::source_order, visitor::source_order::SourceOrderVisitor,
};
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextRange};

#[derive(Debug, Default, Clone, Copy)]
pub struct LineCoverageStats {
    pub precise: u64,
    pub imprecise: u64,
    pub dynamic: u64,
    pub todo: u64,
    pub empty: u64,
}

impl LineCoverageStats {
    pub fn typed_total(&self) -> u64 {
        self.precise + self.imprecise + self.dynamic + self.todo
    }

    #[must_use]
    pub fn merge(self, other: LineCoverageStats) -> LineCoverageStats {
        LineCoverageStats {
            precise: self.precise + other.precise,
            imprecise: self.imprecise + other.imprecise,
            dynamic: self.dynamic + other.dynamic,
            todo: self.todo + other.todo,
            empty: self.empty + other.empty,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CoverageStats {
    pub total: u64,
    pub dynamic: u64,
    pub imprecise: u64,
    pub todo: u64,
    pub lines: LineCoverageStats,
}

impl CoverageStats {
    #[must_use]
    pub fn merge(self, other: CoverageStats) -> CoverageStats {
        CoverageStats {
            total: self.total + other.total,
            dynamic: self.dynamic + other.dynamic,
            imprecise: self.imprecise + other.imprecise,
            todo: self.todo + other.todo,
            lines: self.lines.merge(other.lines),
        }
    }

    /// Returns the percentage of expressions that are directly or containedly dynamic
    /// (combining `dynamic` + `imprecise` to match mypy's headline imprecision figure).
    #[expect(clippy::cast_precision_loss)]
    pub fn dynamic_percentage(&self) -> Option<f64> {
        if self.total == 0 {
            None
        } else {
            Some((self.dynamic + self.imprecise) as f64 / self.total as f64 * 100.0)
        }
    }

    /// Returns the percentage of expressions whose type is *directly* dynamic
    /// (top-level `Unknown`/`Any` only, not contained in a type argument).
    #[expect(clippy::cast_precision_loss)]
    pub fn pure_dynamic_percentage(&self) -> Option<f64> {
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

/// Per-line coverage classification returned by [`coverage_details`].
///
/// The ordering `Known < Imprecise < Dynamic < Todo` lets the line map use `.max()`
/// to pick the worst classification per line without a manual comparator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TypeCoverage {
    Known,
    Imprecise, // dynamic appears in a type argument, e.g. list[Unknown]
    Dynamic,   // expression type is directly Unknown/Any at the top level
    Todo,      // type contains a @todo marker
}

/// Aggregate stats plus per-line classification for a single file.
pub struct FileCoverageDetails {
    pub stats: CoverageStats,
    /// Maps 1-based line numbers to their worst-case classification.
    /// Lines absent from the map contain no typed expressions (empty).
    pub line_map: HashMap<usize, TypeCoverage>,
}

fn classify(db: &dyn Db, ty: Type<'_>) -> TypeCoverage {
    if any_over_type(db, ty, true, |t: Type<'_>| t.is_todo()) {
        return TypeCoverage::Todo;
    }
    // Top-level dynamic (the expression type itself is directly Unknown/Any).
    if ty.is_dynamic() {
        return TypeCoverage::Dynamic;
    }
    // Contained dynamic (Unknown/Any appears in a type argument).
    if any_over_type(db, ty, true, |t: Type<'_>| t.is_dynamic()) {
        return TypeCoverage::Imprecise;
    }
    TypeCoverage::Known
}

/// Returns both aggregate stats and a per-line classification map for `file`.
pub fn coverage_details(db: &dyn Db, file: File) -> FileCoverageDetails {
    let mut visitor = CoverageVisitor::new(db, file);
    let ast = parsed_module(db, file).load(db);
    visitor.visit_body(ast.suite());

    let mut lines = LineCoverageStats::default();
    for line in 1..=visitor.line_index.line_count() {
        match visitor.line_map.get(&line) {
            None => lines.empty += 1,
            Some(TypeCoverage::Known) => lines.precise += 1,
            Some(TypeCoverage::Imprecise) => lines.imprecise += 1,
            Some(TypeCoverage::Dynamic) => lines.dynamic += 1,
            Some(TypeCoverage::Todo) => lines.todo += 1,
        }
    }
    visitor.stats.lines = lines;

    FileCoverageDetails {
        stats: visitor.stats,
        line_map: visitor.line_map,
    }
}

/// Returns only aggregate stats for `file`. Prefer [`coverage_details`] when
/// per-line data is also needed (e.g. for HTML output).
pub fn coverage_types(db: &dyn Db, file: File) -> CoverageStats {
    coverage_details(db, file).stats
}

struct CoverageVisitor<'db> {
    db: &'db dyn Db,
    line_index: LineIndex,
    source: ruff_db::source::SourceText,
    model: SemanticModel<'db>,
    stats: CoverageStats,
    line_map: HashMap<usize, TypeCoverage>,
}

impl<'db> CoverageVisitor<'db> {
    fn new(db: &'db dyn Db, file: File) -> Self {
        Self {
            db,
            line_index: line_index(db, file),
            source: source_text(db, file),
            model: SemanticModel::new(db, file),
            stats: CoverageStats::default(),
            line_map: HashMap::new(),
        }
    }

    fn record(&mut self, ty: Option<Type<'db>>, range: TextRange) {
        let Some(ty) = ty else {
            return;
        };

        self.stats.total += 1;
        let coverage = classify(self.db, ty);

        let line = self
            .line_index
            .line_column(range.start(), &self.source)
            .line
            .get();
        self.line_map
            .entry(line)
            .and_modify(|prev| *prev = (*prev).max(coverage))
            .or_insert(coverage);

        match coverage {
            TypeCoverage::Todo => self.stats.todo += 1,
            TypeCoverage::Dynamic => self.stats.dynamic += 1,
            TypeCoverage::Imprecise => self.stats.imprecise += 1,
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
                self.record(function.inferred_type(&self.model), function.name.range());
            }
            ast::Stmt::ClassDef(class) => {
                self.record(class.inferred_type(&self.model), class.name.range());
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
        self.record(expr.inferred_type(&self.model), expr.range());
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
        self.record(parameter.inferred_type(&self.model), parameter.name.range());
        source_order::walk_parameter(self, parameter);
    }

    fn visit_alias(&mut self, alias: &ast::Alias) {
        self.record(alias.inferred_type(&self.model), alias.range());
        source_order::walk_alias(self, alias);
    }
}
