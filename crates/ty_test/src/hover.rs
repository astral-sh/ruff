//! Hover type inference for mdtest assertions.
//!
//! This module provides functionality to extract hover assertions from comments,
//! infer types at specified positions, and generate hover check outputs for matching.

use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::{line_index, source_text};
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::AnyNodeRef;
use ruff_source_file::{OneIndexed, PositionEncoding, SourceLocation};
use ruff_text_size::{Ranged, TextSize};
use ty_python_semantic::{HasType, SemanticModel};

use crate::assertion::{InlineFileAssertions, ParsedAssertion, UnparsedAssertion};
use crate::check_output::CheckOutput;
use crate::db::Db;

/// A hover result for testing hover assertions.
#[derive(Debug, Clone)]
pub(crate) struct HoverOutput {
    /// The position where hover was requested
    pub(crate) offset: TextSize,
    /// The inferred type at that position
    pub(crate) inferred_type: String,
}

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
/// Uses the parsed assertions from the assertion module, which correctly handles
/// multiple stacked assertion comments and determines the target line number.
pub(crate) fn generate_hover_outputs(
    db: &Db,
    file: File,
    assertions: &InlineFileAssertions,
) -> Vec<CheckOutput> {
    let source = source_text(db, file);
    let lines = line_index(db, file);

    let mut hover_outputs = Vec::new();

    // Iterate through all assertion groups, which are already associated with their target line
    for line_assertions in assertions {
        let target_line = line_assertions.line_number;

        // Look for hover assertions in this line's assertions
        for assertion in line_assertions.iter() {
            let UnparsedAssertion::Hover { .. } = assertion else {
                continue;
            };

            // Parse the assertion to get the hover information
            let Ok(ParsedAssertion::Hover(hover)) = assertion.parse(&lines, &source) else {
                // Invalid hover assertion - will be caught as error by matcher
                continue;
            };

            // Convert the character column to a byte offset using LineIndex::offset
            let hover_location = SourceLocation {
                line: target_line,
                character_offset: OneIndexed::from_zero_indexed(hover.column),
            };
            let hover_offset = lines.offset(hover_location, &source, PositionEncoding::Utf32);

            // Get the inferred type at that position
            let Some(inferred_type) = infer_type_at_position(db, file, hover_offset) else {
                continue;
            };

            hover_outputs.push(CheckOutput::Hover(HoverOutput {
                offset: hover_offset,
                inferred_type,
            }));
        }
    }

    hover_outputs
}
