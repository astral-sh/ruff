//! Tracks per-line type-coverage classifications (precise, imprecise, dynamic, todo, empty)
//! as a measure of ty's type-inference coverage.

use std::collections::HashMap;

use crate::types::Type;
use crate::types::visitor::any_over_type;
use crate::{Db, HasType, SemanticModel};
use ruff_db::source::line_index;
use ruff_db::{files::File, parsed::parsed_module};
use ruff_python_ast::{
    self as ast, helpers::is_docstring_stmt, visitor::source_order,
    visitor::source_order::SourceOrderVisitor,
};
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextRange};

/// Per-line coverage counts for a file or a collection of files.
#[derive(Debug, Default, Clone, Copy)]
pub struct CoverageStats {
    pub precise: u64,
    pub imprecise: u64,
    pub dynamic: u64,
    pub todo: u64,
    pub empty: u64,
}

impl CoverageStats {
    pub fn total(&self) -> u64 {
        self.precise + self.imprecise + self.dynamic + self.todo + self.empty
    }

    /// Combined (imprecise + dynamic) line percentage, used for sort order and quality badges.
    #[expect(clippy::cast_precision_loss)]
    pub fn combined_imprecision_pct(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        (self.dynamic + self.imprecise) as f64 / total as f64 * 100.0
    }

    #[must_use]
    pub fn merge(self, other: CoverageStats) -> CoverageStats {
        CoverageStats {
            precise: self.precise + other.precise,
            imprecise: self.imprecise + other.imprecise,
            dynamic: self.dynamic + other.dynamic,
            todo: self.todo + other.todo,
            empty: self.empty + other.empty,
        }
    }
}

/// Per-line coverage classification returned by [`coverage_details`].
///
/// The ordering `Precise < Imprecise < Dynamic < Todo` lets the line map use `.max()`
/// to pick the most dynamic classification per line without a manual comparator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TypeCoverage {
    Precise,
    Imprecise, // dynamic appears in a type argument, e.g. list[Unknown]
    Dynamic,   // expression type is directly Unknown/Any at the top level
    Todo,      // type contains a @todo marker
}

/// Aggregate stats plus per-line classification for a single file.
#[derive(Debug)]
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
    TypeCoverage::Precise
}

/// Returns both aggregate stats and a per-line classification map for `file`.
pub fn coverage_details(db: &dyn Db, file: File) -> FileCoverageDetails {
    let mut visitor = CoverageVisitor::new(db, file);
    let ast = parsed_module(db, file).load(db);
    visitor.visit_body(ast.suite());

    let mut stats = CoverageStats::default();
    for line in 1..=visitor.line_index.line_count() {
        match visitor.line_map.get(&line) {
            None => stats.empty += 1,
            Some(TypeCoverage::Precise) => stats.precise += 1,
            Some(TypeCoverage::Imprecise) => stats.imprecise += 1,
            Some(TypeCoverage::Dynamic) => stats.dynamic += 1,
            Some(TypeCoverage::Todo) => stats.todo += 1,
        }
    }

    FileCoverageDetails {
        stats,
        line_map: visitor.line_map,
    }
}

struct CoverageVisitor<'db> {
    db: &'db dyn Db,
    line_index: LineIndex,
    model: SemanticModel<'db>,
    line_map: HashMap<usize, TypeCoverage>,
}

impl<'db> CoverageVisitor<'db> {
    fn new(db: &'db dyn Db, file: File) -> Self {
        Self {
            db,
            line_index: line_index(db, file),
            model: SemanticModel::new(db, file),
            line_map: HashMap::new(),
        }
    }

    fn record(&mut self, ty: Option<Type<'db>>, range: TextRange) {
        let Some(ty) = ty else {
            return;
        };

        let coverage = classify(self.db, ty);

        let start_line = self.line_index.line_index(range.start()).get();
        let end_line = self.line_index.line_index(range.end()).get();

        // On the start line, take the most dynamic classification: multiple expressions
        // may start on the same line and the most dynamic one should win.
        self.line_map
            .entry(start_line)
            .and_modify(|prev| *prev = (*prev).max(coverage))
            .or_insert(coverage);

        // For subsequent lines covered by this expression's range, only fill in
        // lines that have no expression starting on them (e.g. closing parens,
        // continuation lines, triple-quoted string content). Because visit_expr
        // walks children before recording the parent, children have already
        // claimed their lines, so a precise child won't be overwritten here.
        for line in (start_line + 1)..=end_line {
            self.line_map.entry(line).or_insert(coverage);
        }
    }
}

impl SourceOrderVisitor<'_> for CoverageVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        match stmt {
            ast::Stmt::FunctionDef(function) => {
                self.record(function.inferred_type(&self.model), function.name.range());
            }
            ast::Stmt::ClassDef(class) => {
                self.record(class.inferred_type(&self.model), class.name.range());
            }
            // Docstrings carry no type information worth measuring; leave their
            // lines empty rather than counting them as precise.
            stmt if is_docstring_stmt(stmt) => return,
            _ => {}
        }
        source_order::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &ast::Expr) {
        // Walk children before recording the parent so that children claim their
        // lines first; the parent's range expansion then only fills uncovered gaps.
        source_order::walk_expr(self, expr);
        self.record(expr.inferred_type(&self.model), expr.range());
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
