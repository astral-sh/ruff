use ruff_python_ast::source_code::Locator;
use ruff_text_size::{TextLen, TextRange, TextSize};

/// Return `true` if the given string is a radix literal (e.g., `0b101`).
pub fn is_radix_literal(content: &str) -> bool {
    content.starts_with("0b")
        || content.starts_with("0o")
        || content.starts_with("0x")
        || content.starts_with("0B")
        || content.starts_with("0O")
        || content.starts_with("0X")
}

/// Find the first token in the given range that satisfies the given predicate.
pub fn find_tok(
    range: TextRange,
    locator: &Locator,
    f: impl Fn(rustpython_parser::Tok) -> bool,
) -> TextRange {
    for (tok, tok_range) in rustpython_parser::lexer::lex_located(
        &locator.contents()[range],
        rustpython_parser::Mode::Module,
        range.start(),
    )
    .flatten()
    {
        if f(tok) {
            return tok_range;
        }
    }
    unreachable!("Failed to find token in range {:?}", range)
}

/// Expand the range of a compound statement.
///
/// `location` is the start of the compound statement (e.g., the `if` in `if x:`).
/// `end_location` is the end of the last statement in the body.
pub fn expand_indented_block(
    location: TextSize,
    end_location: TextSize,
    locator: &Locator,
) -> TextRange {
    let contents = locator.contents();

    // Find the colon, which indicates the end of the header.
    let mut nesting = 0;
    let mut colon = None;
    for (tok, tok_range) in rustpython_parser::lexer::lex_located(
        &contents[TextRange::new(location, end_location)],
        rustpython_parser::Mode::Module,
        location,
    )
    .flatten()
    {
        match tok {
            rustpython_parser::Tok::Colon if nesting == 0 => {
                colon = Some(tok_range.start());
                break;
            }
            rustpython_parser::Tok::Lpar
            | rustpython_parser::Tok::Lsqb
            | rustpython_parser::Tok::Lbrace => nesting += 1,
            rustpython_parser::Tok::Rpar
            | rustpython_parser::Tok::Rsqb
            | rustpython_parser::Tok::Rbrace => nesting -= 1,
            _ => {}
        }
    }
    let colon_location = colon.unwrap();

    // From here, we have two options: simple statement or compound statement.
    let indent = rustpython_parser::lexer::lex_located(
        &contents[TextRange::new(colon_location, end_location)],
        rustpython_parser::Mode::Module,
        colon_location,
    )
    .flatten()
    .find_map(|(tok, range)| match tok {
        rustpython_parser::Tok::Indent => Some(range.end()),
        _ => None,
    });

    let line_end = locator.line_end(end_location);
    let Some(indent_end) = indent else {

        // Simple statement: from the colon to the end of the line.
        return TextRange::new(colon_location, line_end);
    };

    let indent_width = indent_end - locator.line_start(indent_end);

    // Compound statement: from the colon to the end of the block.
    // For each line that follows, check that there's no content up to the expected indent.
    let mut offset = TextSize::default();
    let mut line_offset = TextSize::default();
    // Issue, body goes to far..  it includes the whole try including the catch

    let rest = &contents[usize::from(line_end)..];
    for (relative_offset, c) in rest.char_indices() {
        if line_offset < indent_width && !c.is_whitespace() {
            break; // Found end of block
        }

        match c {
            '\n' | '\r' => {
                // Ignore empty lines
                if line_offset > TextSize::from(0) {
                    offset = TextSize::try_from(relative_offset).unwrap() + TextSize::from(1);
                }
                line_offset = TextSize::from(0);
            }
            _ => {
                line_offset += c.text_len();
            }
        }
    }

    // Reached end of file
    let end = if line_offset >= indent_width {
        contents.text_len()
    } else {
        line_end + offset
    };

    TextRange::new(colon_location, end)
}

/// Return true if the `orelse` block of an `if` statement is an `elif` statement.
pub fn is_elif(orelse: &[rustpython_parser::ast::Stmt], locator: &Locator) -> bool {
    if orelse.len() == 1 && matches!(orelse[0].node, rustpython_parser::ast::StmtKind::If { .. }) {
        let contents = locator.after(orelse[0].start());
        if contents.starts_with("elif") {
            return true;
        }
    }
    false
}
