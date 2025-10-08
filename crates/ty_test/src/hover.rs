//! Hover type inference for mdtest assertions.
//!
//! This module provides functionality to extract hover assertions from comments,
//! infer types at specified positions, and generate hover check outputs for matching.

use crate::matcher;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::{line_index, source_text};
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::AnyNodeRef;
use ruff_python_trivia::CommentRanges;
use ruff_text_size::{Ranged, TextSize};
use ty_python_semantic::{HasType, SemanticModel};

use crate::db::Db;

/// Find the AST node with minimal range that fully contains the given offset.
/// This is a simplified version of ty_ide's covering_node logic.
fn find_covering_node<'a>(root: AnyNodeRef<'a>, offset: TextSize) -> Option<AnyNodeRef<'a>> {
    struct Visitor<'a> {
        offset: TextSize,
        found: Option<AnyNodeRef<'a>>,
    }

    impl<'a> SourceOrderVisitor<'a> for Visitor<'a> {
        fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
            if node.range().contains(self.offset) {
                self.found = Some(node);
                TraversalSignal::Traverse
            } else {
                TraversalSignal::Skip
            }
        }
    }

    let mut visitor = Visitor {
        offset,
        found: None,
    };

    root.visit_source_order(&mut visitor);
    visitor.found
}

/// Get the inferred type at a given position in a file.
/// Returns None if no node is found at that position or if the node has no type.
fn infer_type_at_position(db: &Db, file: File, offset: TextSize) -> Option<String> {
    let parsed = parsed_module(db, file).load(db);
    let ast = parsed.syntax();
    let root: AnyNodeRef = ast.into();

    let node = find_covering_node(root, offset)?;

    let model = SemanticModel::new(db, file);

    // Try to get the type from the node - HasType is mainly implemented for ast types
    let ty = match node {
        AnyNodeRef::StmtFunctionDef(s) => s.inferred_type(&model),
        AnyNodeRef::StmtClassDef(s) => s.inferred_type(&model),
        AnyNodeRef::StmtExpr(s) => s.value.as_ref().inferred_type(&model),
        AnyNodeRef::ExprBoolOp(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprNamed(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprBinOp(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprUnaryOp(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprLambda(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprIf(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprDict(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprSet(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprListComp(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprSetComp(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprDictComp(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprGenerator(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprAwait(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprYield(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprYieldFrom(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprCompare(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprCall(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprFString(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprStringLiteral(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprBytesLiteral(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprNumberLiteral(e) => {
            ruff_python_ast::ExprRef::from(e).inferred_type(&model)
        }
        AnyNodeRef::ExprBooleanLiteral(e) => {
            ruff_python_ast::ExprRef::from(e).inferred_type(&model)
        }
        AnyNodeRef::ExprNoneLiteral(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprEllipsisLiteral(e) => {
            ruff_python_ast::ExprRef::from(e).inferred_type(&model)
        }
        AnyNodeRef::ExprAttribute(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprSubscript(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprStarred(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprName(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprList(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprTuple(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprSlice(e) => ruff_python_ast::ExprRef::from(e).inferred_type(&model),
        AnyNodeRef::ExprIpyEscapeCommand(e) => {
            ruff_python_ast::ExprRef::from(e).inferred_type(&model)
        }
        _ => return None,
    };

    Some(ty.display(db).to_string())
}

/// Generate hover CheckOutputs for all hover assertions in a file.
///
/// This scans the file for hover assertions (comments with `# ↓ hover:`),
/// computes the hover position from the down arrow location, calls the type
/// inference, and returns CheckOutput::Hover entries.
pub(crate) fn generate_hover_outputs(db: &Db, file: File) -> Vec<matcher::CheckOutput> {
    let source = source_text(db, file);
    let lines = line_index(db, file);
    let parsed = parsed_module(db, file).load(db);
    let comment_ranges = CommentRanges::from(parsed.tokens());

    let mut hover_outputs = Vec::new();

    for comment_range in &comment_ranges {
        let comment_text = &source[comment_range];

        // Check if this is a hover assertion (contains "# ↓ hover:" or "# hover:")
        if !comment_text.trim().starts_with('#') {
            continue;
        }

        let trimmed = comment_text.trim().strip_prefix('#').unwrap().trim();
        if !trimmed.starts_with("↓ hover:") && !trimmed.starts_with("hover:") {
            continue;
        }

        // Find the down arrow position in the comment
        let arrow_offset = comment_text.find('↓');
        if arrow_offset.is_none() {
            // No down arrow means we can't determine the column
            continue;
        }
        let arrow_column = arrow_offset.unwrap();

        // Get the line number of the comment
        let comment_line = lines.line_index(comment_range.start());

        // The hover target is the next non-comment, non-empty line
        let target_line = comment_line.saturating_add(1);

        // Get the start offset of the target line
        let target_line_start = lines.line_start(target_line, &source);

        // Calculate the hover position: start of target line + arrow column
        let hover_offset = target_line_start + TextSize::try_from(arrow_column).unwrap();

        // Get the inferred type at that position
        if let Some(inferred_type) = infer_type_at_position(db, file, hover_offset) {
            hover_outputs.push(matcher::CheckOutput::Hover {
                offset: hover_offset,
                inferred_type,
            });
        }
    }

    hover_outputs
}
