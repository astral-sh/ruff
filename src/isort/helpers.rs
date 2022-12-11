use crate::source_code_locator::SourceCodeLocator;
use rustpython_ast::Stmt;

/// Return a tuple of (number of blank lines, number of commented lines) preceding a `Stmt`.
pub fn match_leading_comments(stmt: &Stmt, locator: &SourceCodeLocator) -> (usize, usize) {
    let mut num_blanks = 0;
    let mut num_comments = 0;
    for line in locator
        .slice_source_code_until(&stmt.location)
        .lines()
        .rev()
    {
        let line = line.trim();
        if line.is_empty() {
            num_blanks += 1;
        } else if line.starts_with('#') {
            num_comments += 1;
        } else {
            break;
        }
    }
    (num_blanks, num_comments)
}
