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
fn find_covering_node<'a>(root: AnyNodeRef<'a>, offset: TextSize) -> Option<AnyNodeRef<'a>> {
    struct Visitor<'a> {
        offset: TextSize,
        minimal_node: Option<AnyNodeRef<'a>>,
    }

    impl<'a> SourceOrderVisitor<'a> for Visitor<'a> {
        fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
            if node.range().contains(self.offset) {
                // Update minimal_node if this node's range is smaller than the current one
                if let Some(current) = self.minimal_node {
                    if node.range().len() < current.range().len() {
                        self.minimal_node = Some(node);
                    }
                } else {
                    self.minimal_node = Some(node);
                }
                TraversalSignal::Traverse
            } else {
                TraversalSignal::Skip
            }
        }
    }

    let mut visitor = Visitor {
        offset,
        minimal_node: None,
    };

    root.visit_source_order(&mut visitor);
    visitor.minimal_node
}

/// Get the inferred type at a given position in a file.
/// Returns None if no node is found at that position or if the node has no type.
fn infer_type_at_position(db: &Db, file: File, offset: TextSize) -> Option<String> {
    let parsed = parsed_module(db, file).load(db);
    let ast = parsed.syntax();
    let root: AnyNodeRef = ast.into();

    let node = find_covering_node(root, offset)?;

    let model = SemanticModel::new(db, file);

    // Get the expression at this position and infer its type
    let expr = node.as_expr_ref()?;
    let ty = expr.inferred_type(&model);

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
