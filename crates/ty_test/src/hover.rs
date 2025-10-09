//! Hover type inference for mdtest assertions.
//!
//! This module provides functionality to extract hover assertions from comments, infer types at
//! specified positions, and generate hover check outputs for matching.

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

/// A hover result for testing `hover` assertions.
#[derive(Debug, Clone)]
pub(crate) struct HoverOutput {
    /// The offset (within the entire file) where hover was requested
    pub(crate) offset: TextSize,
    /// The inferred type at that position
    pub(crate) inferred_type: String,
}

/// Get the inferred type at a given position in a file. Returns None if no node is found at that
/// position or if the node has no inferred type.
///
/// This reuses much of the logic from [`ty_ide::hover`]. Unlike that function, we return types for
/// literals, which is useful for testing type inference in mdtest assertions.
fn infer_type_at_position(db: &Db, file: File, offset: TextSize) -> Option<String> {
    let parsed = parsed_module(db, file).load(db);
    let goto_target = find_goto_target(&parsed, offset)?;

    let model = SemanticModel::new(db, file);
    let ty = goto_target.inferred_type(&model)?;

    Some(ty.display(db).to_string())
}

/// Generate hover outputs for all of the `hover` assertions in a file.
pub(crate) fn generate_hover_outputs_into(
    db: &Db,
    hover_outputs: &mut Vec<CheckOutput>,
    file: File,
) {
    let assertions = InlineFileAssertions::from_file(db, file);
    let source = source_text(db, file);
    let lines = line_index(db, file);

    // Iterate through all assertion groups, which are already associated with their target line
    for line_assertions in &assertions {
        let target_line = line_assertions.line_number;

        // Look for hover assertions in this line's assertions
        for assertion in line_assertions.iter() {
            if !matches!(assertion, UnparsedAssertion::Hover { .. }) {
                continue;
            }

            let Ok(ParsedAssertion::Hover(hover)) = assertion.parse(&lines, &source) else {
                // The matcher will catch and report incorrectly formatted `hover` assertions, so
                // we can just skip them.
                continue;
            };

            // Convert the column offset within the assertion's line into a byte offset within the
            // entire file.
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
}
