use rustpython_ast::Stmt;

use crate::source_code_locator::SourceCodeLocator;

/// Return `true` if a `Stmt` is preceded by a "comment break"
pub fn has_comment_break(stmt: &Stmt, locator: &SourceCodeLocator) -> bool {
    // Starting from the `Stmt` (`def f(): pass`), we want to detect patterns like
    // this:
    //
    //   import os
    //
    //   # Detached comment.
    //
    //   def f(): pass

    // This should also be detected:
    //
    //   import os
    //
    //   # Detached comment.
    //
    //   # Direct comment.
    //   def f(): pass

    // But this should not:
    //
    //   import os
    //
    //   # Direct comment.
    //   def f(): pass
    let mut seen_blank = false;
    for line in locator
        .slice_source_code_until(&stmt.location)
        .lines()
        .rev()
    {
        let line = line.trim();
        if seen_blank {
            if line.starts_with('#') {
                return true;
            } else if !line.is_empty() {
                break;
            }
        } else {
            if line.is_empty() {
                seen_blank = true;
            } else if line.starts_with('#') || line.starts_with('@') {
                continue;
            } else {
                break;
            }
        }
    }
    false
}
