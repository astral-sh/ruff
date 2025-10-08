//! Hover type inference for mdtest assertions.
//!
//! This module provides functionality to extract hover assertions from comments,
//! infer types at specified positions, and generate hover check outputs for matching.

use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::{line_index, source_text};
use ruff_source_file::{PositionEncoding, SourceLocation};
use ruff_text_size::TextSize;
use ty_ide::find_goto_target;
use ty_python_semantic::SemanticModel;

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

/// Get the inferred type at a given position in a file using ty_ide's goto logic.
/// Returns None if no node is found at that position or if the node has no type.
///
/// Unlike ty_ide::hover, this function includes types for literals, which is useful
/// for testing type inference in mdtest assertions.
fn infer_type_at_position(db: &Db, file: File, offset: TextSize) -> Option<String> {
    let parsed = parsed_module(db, file).load(db);
    let goto_target = find_goto_target(&parsed, offset)?;

    let model = SemanticModel::new(db, file);
    let ty = goto_target.inferred_type(&model)?;

    Some(ty.display(db).to_string())
}

/// Generate hover `CheckOutputs` for all hover assertions in a file.
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
                character_offset: hover.column,
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
