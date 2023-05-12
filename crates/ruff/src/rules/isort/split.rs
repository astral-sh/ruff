use crate::rules::isort::types::{ImportBlock, Importable};

/// Find the index of the block that the import should be placed in.
/// The index is the position of the pattern in `forced_separate` plus one.
/// If the import is not matched by any of the patterns, return 0 (the first
/// block).
fn find_block_index(forced_separate: &[String], imp: &dyn Importable) -> usize {
    forced_separate
        .iter()
        .position(|pattern| imp.module_base().starts_with(pattern))
        .map_or(0, |position| position + 1)
}

/// Split the import block into multiple blocks, where the first block is the
/// imports that are not matched by any of the patterns in `forced_separate`,
/// and the rest of the blocks are the imports that _are_ matched by the
/// patterns in `forced_separate`, in the order they appear in the
/// `forced_separate` set. Empty blocks are retained for patterns that do not
/// match any imports.
pub(crate) fn split_by_forced_separate<'a>(
    block: ImportBlock<'a>,
    forced_separate: &[String],
) -> Vec<ImportBlock<'a>> {
    if forced_separate.is_empty() {
        // Nothing to do here.
        return vec![block];
    }
    let mut blocks = vec![ImportBlock::default()]; // The zeroth block is for non-forced-separate imports.
    for _ in forced_separate {
        // Populate the blocks with empty blocks for each forced_separate pattern.
        blocks.push(ImportBlock::default());
    }
    let ImportBlock {
        import,
        import_from,
        import_from_as,
        import_from_star,
    } = block;
    for (imp, comment_set) in import {
        blocks[find_block_index(forced_separate, &imp)]
            .import
            .insert(imp, comment_set);
    }
    for (imp, val) in import_from {
        blocks[find_block_index(forced_separate, &imp)]
            .import_from
            .insert(imp, val);
    }
    for ((imp, alias), val) in import_from_as {
        blocks[find_block_index(forced_separate, &imp)]
            .import_from_as
            .insert((imp, alias), val);
    }
    for (imp, comment_set) in import_from_star {
        blocks[find_block_index(forced_separate, &imp)]
            .import_from_star
            .insert(imp, comment_set);
    }
    blocks
}
