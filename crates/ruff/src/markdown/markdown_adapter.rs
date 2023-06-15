use pulldown_cmark::{CodeBlockKind, CowStr, Event, Parser, Tag};

use crate::code_extraction::{BoundedCodeBlock, CodeBlocks, CodeExtractor};

#[derive(Debug, Default)]
pub struct MarkdownAdapter {}

impl CodeExtractor for MarkdownAdapter {
    fn extract_code<'a, 'input>(&'a self, input: &'input str) -> CodeBlocks {
        let mut in_code_block = false;
        let mut code_block: Option<BoundedCodeBlock> = None;
        let mut code_blocks = CodeBlocks::new();

        Parser::new(input)
            .into_offset_iter()
            .for_each(|(event, range)| match &event {
                Event::Start(tag) => match tag {
                    Tag::CodeBlock(block) => {
                        if is_python_block(&block) {
                            in_code_block = true;
                        }
                    }
                    _ => (),
                },
                Event::Text(text) => {
                    if in_code_block {
                        code_block = Some(BoundedCodeBlock::from_range(text, range));
                    }
                }
                Event::End(_) => {
                    if let Some(bounded_code_block) = &code_block {
                        code_blocks.add(bounded_code_block.clone());
                        in_code_block = false;
                        code_block = None;
                    }
                }
                _ => (),
            });

        code_blocks
    }
}

fn is_python_block(block: &CodeBlockKind) -> bool {
    match block {
        // TODO: detecting Python code from code blocks which aren't labeled as such?
        CodeBlockKind::Fenced(b) => match b {
            CowStr::Borrowed(language) => language.to_owned().to_lowercase() == "python",
            _ => false,
        },
        CodeBlockKind::Indented => false,
    }
}

#[cfg(test)]
mod tests {
    use ruff_text_size::TextSize;

    use crate::code_extraction::BoundedCodeBlock;

    use super::*;

    #[test]
    fn test_get_code_blocks() {
        let input = r#"
# Heading 1
Some more content

```
# fenced block with no language specified
def foo():
    return x
```

```python
# fenced block with language specified
def bar():
    return y
```

and now let's make sure that a second block is picked up
```python
def barTwo():
    return z
```
            "#;
        let adapter = MarkdownAdapter::default();
        let code_blocks = adapter.extract_code(input);
        assert_eq!(
            vec![
                BoundedCodeBlock::new(
                    "# fenced block with language specified\ndef bar():\n    return y\n".to_owned(),
                    TextSize::new(117)
                ),
                BoundedCodeBlock::new(
                    "def barTwo():\n    return z\n".to_owned(),
                    TextSize::new(252)
                )
            ],
            code_blocks.blocks
        );
    }
}
