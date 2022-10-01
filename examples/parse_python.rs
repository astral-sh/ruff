use anyhow::Result;
use tree_sitter::{Parser, Query, QueryCursor};

enum Action {
    Up,
    Down,
    Right,
}

fn main() -> Result<()> {
    let src = r#"
def double(x):
    # Return a double.
    return x * 2

x = double(1)
y = (f"{x}" "b")
"#;
    let mut parser = Parser::new();
    parser
        .set_language(tree_sitter_python::language())
        .expect("Error loading Python grammar");
    let parse_tree = parser.parse(src, None);

    if let Some(parse_tree) = &parse_tree {
        // Check for comments.
        let query = Query::new(tree_sitter_python::language(), "(comment) @capture")?;
        let mut query_cursor = QueryCursor::new();
        let all_matches = query_cursor.matches(&query, parse_tree.root_node(), src.as_bytes());

        for each_match in all_matches {
            for capture in each_match.captures.iter() {
                let range = capture.node.range();
                let text = &src[range.start_byte..range.end_byte];
                let line = range.start_point.row;
                let col = range.start_point.column;
                println!(
                    "[Line: {}, Col: {}] Offending source code: `{}`",
                    line, col, text
                );
            }
        }

        // Check for string concatenations.
        let query = Query::new(
            tree_sitter_python::language(),
            "(concatenated_string) @capture",
        )?;
        let mut query_cursor = QueryCursor::new();
        let all_matches = query_cursor.matches(&query, parse_tree.root_node(), src.as_bytes());

        for each_match in all_matches {
            for capture in each_match.captures.iter() {
                let range = capture.node.range();
                let text = &src[range.start_byte..range.end_byte];
                let line = range.start_point.row;
                let col = range.start_point.column;
                println!(
                    "[Line: {}, Col: {}] Offending source code: `{}`",
                    line, col, text
                );
            }
        }

        // Walk the tree.
        let mut cursor = parse_tree.walk();
        let mut action = Action::Down;
        loop {
            match action {
                Action::Up => {
                    if cursor.goto_next_sibling() {
                        action = Action::Right;
                    } else if cursor.goto_parent() {
                        action = Action::Up;
                    } else {
                        break;
                    }
                }
                Action::Down => {
                    let range = cursor.node().range();
                    let text = &src[range.start_byte..range.end_byte];
                    let line = range.start_point.row;
                    let col = range.start_point.column;
                    println!(
                        "[Line: {}, Col: {}] {}: `{}`",
                        line,
                        col,
                        cursor.node().kind(),
                        text
                    );

                    if cursor.goto_first_child() {
                        action = Action::Down;
                    } else if cursor.goto_next_sibling() {
                        action = Action::Right;
                    } else if cursor.goto_parent() {
                        action = Action::Up;
                    } else {
                        break;
                    }
                }
                Action::Right => {
                    let range = cursor.node().range();
                    let text = &src[range.start_byte..range.end_byte];
                    let line = range.start_point.row;
                    let col = range.start_point.column;
                    println!(
                        "[Line: {}, Col: {}] {}: `{}`",
                        line,
                        col,
                        cursor.node().kind(),
                        text
                    );

                    if cursor.goto_first_child() {
                        action = Action::Down;
                    } else if cursor.goto_next_sibling() {
                        action = Action::Right;
                    } else if cursor.goto_parent() {
                        action = Action::Up;
                    } else {
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
