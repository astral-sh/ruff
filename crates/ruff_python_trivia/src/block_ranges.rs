use ruff_text_size::TextRange;

/// Stores the ranges of indents and dedents sorted by [`TextRange::start`] in increasing order.
#[derive(Clone, Debug, Default)]
pub struct BlockRanges {
    raw: Vec<BlockRange>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BlockRange {
    pub indent: TextRange,
    pub dedent: TextRange,
}

impl BlockRanges {
    pub fn new(indent_ranges: Vec<TextRange>, dedent_ranges: Vec<TextRange>) -> Self {
        let mut index = 0;
        let mut stack = Vec::new();
        let mut blocks = Vec::new();

        for dedent in &dedent_ranges {
            while index < indent_ranges.len() && indent_ranges[index].end() < dedent.start() {
                stack.push(indent_ranges[index]);
                index += 1;
            }

            if let Some(indent) = stack.pop() {
                blocks.push(BlockRange {
                    indent,
                    dedent: *dedent,
                });
            }
        }

        blocks.sort_by_key(|b| b.indent.start());

        Self { raw: blocks }
    }

    pub fn containing(&self, range: &TextRange) -> Vec<&BlockRange> {
        self.raw
            .iter()
            .filter(|block| {
                block.indent.start() <= range.start() && block.dedent.end() > range.end()
            })
            .collect()
    }
}
