use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal, walk_body};
use ruff_python_ast::{AnyNodeRef, Decorator, Stmt, StmtClassDef, StmtFunctionDef};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_source_file::{Line, UniversalNewlines};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::Db;

/// The kind of a folding range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldingRangeKind {
    /// A comment block.
    Comment,
    /// An import block.
    Imports,
    /// A region (e.g., `# region` / `# endregion`).
    Region,
}

/// A folding range in the source code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldingRange {
    /// The range to fold.
    pub range: TextRange,
    /// The kind of folding range.
    pub kind: Option<FoldingRangeKind>,
}

impl FoldingRange {
    fn with_kind(self, kind: FoldingRangeKind) -> Self {
        Self {
            kind: Some(kind),
            ..self
        }
    }
}

impl From<TextRange> for FoldingRange {
    fn from(range: TextRange) -> FoldingRange {
        FoldingRange { range, kind: None }
    }
}

/// Returns a list of folding ranges for the given file.
pub fn folding_ranges(db: &dyn Db, file: File) -> Vec<FoldingRange> {
    let parsed = parsed_module(db, file).load(db);
    let source = source_text(db, file);

    let mut visitor = FoldingRangeVisitor {
        source: source.as_str(),
        ranges: vec![],
    };
    visitor.visit_body(parsed.suite());

    // Add docstring for module-level (first statement if it's a string literal).
    visitor.add_docstring_range(parsed.suite());

    // Add remaining ranges not covered by the AST visitor.
    visitor.add_comment_ranges();
    visitor.add_custom_region_ranges();

    visitor.ranges
}

struct FoldingRangeVisitor<'a> {
    source: &'a str,
    ranges: Vec<FoldingRange>,
}

impl<'a> FoldingRangeVisitor<'a> {
    /// Add the given folding range if it spans multiple lines.
    fn add_range(&mut self, folding_range: impl Into<FoldingRange>) {
        let folding_range = folding_range.into();
        if !self.is_multiline(folding_range.range) {
            return;
        }
        self.force_add_range(folding_range);
    }

    /// Always adds the given range.
    ///
    /// This is useful when you always want a folding range even if
    /// the range may not span multiple lines. For example, `else`
    /// or `finally` blocks.
    fn force_add_range(&mut self, folding_range: impl Into<FoldingRange>) {
        let folding_range = folding_range.into();
        self.ranges.push(folding_range);
    }

    /// Iterate over lines with their starting byte offsets.
    fn lines(&self) -> impl Iterator<Item = Line<'a>> + use<'a> {
        self.source.universal_newlines()
    }

    fn is_multiline(&self, range: TextRange) -> bool {
        self.source[range].contains('\n') || self.source[range].contains('\r')
    }

    /// Compute folding ranges for consecutive import statements.
    /// Import blocks separated by blank lines are folded separately.
    ///
    /// TODO: It might be better to move this logic into the AST
    /// visitor via `enter_node`. I found it clearer to write it as
    /// a single separate pass over a sequence of statements. But if
    /// this ends up being a perf issue, it should be possible to
    /// do this within the existing AST pass.
    fn add_import_ranges(&mut self, stmts: &[Stmt]) {
        let mut import_range: Option<TextRange> = None;
        let mut prev_import_end: Option<TextSize> = None;

        for stmt in stmts {
            if matches!(stmt, Stmt::Import(_) | Stmt::ImportFrom(_)) {
                // Check if there's a blank line between this import and the previous one.
                let has_blank_line = prev_import_end
                    .is_some_and(|prev_end| self.has_blank_line_between(prev_end, stmt.start()));

                if has_blank_line {
                    // Finalize the current import block and start a new one.
                    if let Some(range) = import_range {
                        self.add_range(
                            FoldingRange::from(range).with_kind(FoldingRangeKind::Imports),
                        );
                    }
                    import_range = Some(stmt.range());
                } else if let Some(ref mut range) = import_range {
                    *range = range.with_end(stmt.end());
                } else {
                    import_range = Some(stmt.range());
                }
                prev_import_end = Some(stmt.end());
            } else {
                if let Some(range) = import_range {
                    self.add_range(FoldingRange::from(range).with_kind(FoldingRangeKind::Imports));
                }
                import_range = None;
                prev_import_end = None;
            }
        }
        if let Some(range) = import_range {
            self.add_range(FoldingRange::from(range).with_kind(FoldingRangeKind::Imports));
        }
    }

    /// Check if there's a blank line appearing anywhere between two positions.
    fn has_blank_line_between(&self, start: TextSize, end: TextSize) -> bool {
        let mut count = 0;
        for line in self.source[TextRange::new(start, end)].universal_newlines() {
            if !line.is_empty() {
                return count >= 2;
            }
            count += 1;
        }
        count >= 2
    }

    /// Compute folding ranges for `# region` / `# endregion` comments.
    fn add_custom_region_ranges(&mut self) {
        let mut region_starts: Vec<TextSize> = Vec::new();

        for line in self.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("# region") || trimmed.starts_with("#region") {
                region_starts.push(line.start());
            } else if trimmed.starts_with("# endregion") || trimmed.starts_with("#endregion") {
                if let Some(start) = region_starts.pop() {
                    let end = line.start() + line.trim_end().text_len();
                    self.add_range(
                        FoldingRange::from(TextRange::new(start, end))
                            .with_kind(FoldingRangeKind::Region),
                    );
                }
            }
        }
    }

    /// Compute folding ranges for consecutive comment lines.
    fn add_comment_ranges(&mut self) {
        let mut comment_range: Option<TextRange> = None;

        for line in self.lines() {
            let trimmed = line.trim_start();

            // Check if this is a comment line (but not a region marker)
            let is_comment = trimmed.starts_with('#')
                && !trimmed.starts_with("# region")
                && !trimmed.starts_with("#region")
                && !trimmed.starts_with("# endregion")
                && !trimmed.starts_with("#endregion");

            if is_comment {
                let end = line.start() + line.trim_end().text_len();
                if let Some(ref mut range) = comment_range {
                    *range = range.with_end(end);
                } else {
                    comment_range = Some(TextRange::new(line.start(), end));
                }
            } else if let Some(range) = comment_range {
                self.add_range(FoldingRange::from(range).with_kind(FoldingRangeKind::Comment));
                comment_range = None;
            }
        }
        if let Some(range) = comment_range {
            self.add_range(FoldingRange::from(range).with_kind(FoldingRangeKind::Comment));
        }
    }

    /// Add a folding range for a docstring if present at the start of a body.
    /// Handles string literals, f-strings, and t-strings (but not bytes literals).
    fn add_docstring_range(&mut self, body: &[Stmt]) {
        let Some(first_stmt) = body.first() else {
            return;
        };
        let Stmt::Expr(ref expr_stmt) = *first_stmt else {
            return;
        };
        let is_string_like = expr_stmt.value.is_string_literal_expr()
            || expr_stmt.value.is_f_string_expr()
            || expr_stmt.value.is_t_string_expr();
        if !is_string_like {
            return;
        }
        self.add_range(FoldingRange::from(first_stmt.range()).with_kind(FoldingRangeKind::Comment));
    }

    /// Add a folding range for decorators applied to a class or function.
    fn add_decorator_range(&mut self, decorator_list: &[Decorator]) {
        if !decorator_list.is_empty() {
            let range = TextRange::new(
                decorator_list.first().unwrap().start(),
                decorator_list.last().unwrap().end(),
            );
            self.add_range(range);
        }
    }

    /// Add a folding range for the function or class definition.
    ///
    /// - target is `async` or `def` for functions, and `class` for classes
    fn add_def_range(
        &mut self,
        range: TextRange,
        decorator_list: &[Decorator],
        target: SimpleTokenKind,
    ) {
        if decorator_list.is_empty() {
            self.add_range(range);
            return;
        }

        let tokenizer_start = decorator_list.last().unwrap().range().end();
        let tokenizer = SimpleTokenizer::starts_at(tokenizer_start, self.source);
        if let Some(token) = tokenizer.skip_trivia().find(|token| token.kind == target) {
            let range = TextRange::new(token.start(), range.end());
            self.add_range(range);
        }
    }

    /// Add a folding range for function definitions, excluding decorators.
    fn add_function_def_range(&mut self, func: &StmtFunctionDef) {
        let target = if func.is_async {
            SimpleTokenKind::Async
        } else {
            SimpleTokenKind::Def
        };
        self.add_def_range(func.range(), &func.decorator_list, target);
    }

    /// Add a folding range for class definitions, excluding decorators.
    fn add_class_def_range(&mut self, class: &StmtClassDef) {
        self.add_def_range(class.range(), &class.decorator_list, SimpleTokenKind::Class);
    }
}

impl SourceOrderVisitor<'_> for FoldingRangeVisitor<'_> {
    fn enter_node(&mut self, node: AnyNodeRef<'_>) -> TraversalSignal {
        match node {
            // Compound statements that create folding regions
            AnyNodeRef::StmtFunctionDef(func) => {
                self.add_decorator_range(&func.decorator_list);
                self.add_function_def_range(func);
                // Note that this may be duplicative with folding
                // ranges added for string literals. But I don't think
                // the LSP protocol specifies that this is a problem.
                // If we do need to de-dupe, then we'll want to keep
                // this one since it attaches a "comment" folding range
                // kind to the range. So we'll need to skip over the
                // corresponding range for the literal.
                self.add_docstring_range(&func.body);
            }
            AnyNodeRef::StmtClassDef(class) => {
                self.add_decorator_range(&class.decorator_list);
                self.add_class_def_range(class);
                // See comment above for class docstrings about this
                // being duplicative with adding folding ranges for
                // string literals.
                self.add_docstring_range(&class.body);
            }
            AnyNodeRef::StmtIf(if_stmt) => {
                // Fold each branch individually rather than the entire if block.
                // The if clause range is from the start of the if to the end of its body.
                if let Some(last_stmt) = if_stmt.body.last() {
                    self.add_range(TextRange::new(if_stmt.start(), last_stmt.end()));
                }
                // Each elif/else clause has its own range.
                for clause in &if_stmt.elif_else_clauses {
                    self.add_range(clause.range());
                }
            }
            AnyNodeRef::StmtFor(for_stmt) => {
                // Fold the for body separately from the else block.
                if let Some(last_stmt) = for_stmt.body.last() {
                    self.add_range(TextRange::new(for_stmt.start(), last_stmt.end()));
                }
                if let (Some(first), Some(last)) = (for_stmt.orelse.first(), for_stmt.orelse.last())
                {
                    self.add_range(TextRange::new(first.start(), last.end()));
                }
            }
            AnyNodeRef::StmtWhile(while_stmt) => {
                // Fold the while body separately from the else block.
                if let Some(last_stmt) = while_stmt.body.last() {
                    self.add_range(TextRange::new(while_stmt.start(), last_stmt.end()));
                }
                if let (Some(first), Some(last)) =
                    (while_stmt.orelse.first(), while_stmt.orelse.last())
                {
                    self.add_range(TextRange::new(first.start(), last.end()));
                }
            }
            AnyNodeRef::StmtWith(with_stmt) => {
                self.add_range(with_stmt.range());
            }
            AnyNodeRef::StmtTry(try_stmt) => {
                // Fold the try body separately from handlers, else, and finally.
                if let Some(last_stmt) = try_stmt.body.last() {
                    self.add_range(TextRange::new(try_stmt.start(), last_stmt.end()));
                }
                // Exception handlers are folded via ExceptHandlerExceptHandler.
                // Fold the else block if present.
                if let (Some(first), Some(last)) = (try_stmt.orelse.first(), try_stmt.orelse.last())
                {
                    self.force_add_range(TextRange::new(first.start(), last.end()));
                }
                // Fold the finally block if present.
                if let (Some(first), Some(last)) =
                    (try_stmt.finalbody.first(), try_stmt.finalbody.last())
                {
                    self.force_add_range(TextRange::new(first.start(), last.end()));
                }
            }
            AnyNodeRef::StmtMatch(match_stmt) => {
                self.add_range(match_stmt.range());
            }

            // Match cases within match statements
            AnyNodeRef::MatchCase(case) => {
                self.add_range(case.range());
            }

            // Exception handlers
            AnyNodeRef::ExceptHandlerExceptHandler(handler) => {
                self.add_range(handler.range());
            }

            // Multiline expressions
            AnyNodeRef::ExprList(list) => {
                self.add_range(list.range());
            }
            AnyNodeRef::ExprTuple(tuple) => {
                // Only fold parenthesized tuples.
                if tuple.parenthesized {
                    self.add_range(tuple.range());
                }
            }
            AnyNodeRef::ExprDict(dict) => {
                self.add_range(dict.range());
            }
            AnyNodeRef::ExprSet(set) => {
                self.add_range(set.range());
            }
            AnyNodeRef::ExprListComp(listcomp) => {
                self.add_range(listcomp.range());
            }
            AnyNodeRef::ExprSetComp(setcomp) => {
                self.add_range(setcomp.range());
            }
            AnyNodeRef::ExprDictComp(dictcomp) => {
                self.add_range(dictcomp.range());
            }
            AnyNodeRef::ExprGenerator(generator) => {
                self.add_range(generator.range());
            }

            // Function calls with arguments spanning multiple lines
            AnyNodeRef::ExprCall(call) => {
                self.add_range(call.range());
            }

            // String and bytes literals
            AnyNodeRef::ExprStringLiteral(string) => {
                self.add_range(string.range());
            }
            AnyNodeRef::ExprBytesLiteral(bytes) => {
                self.add_range(bytes.range());
            }
            AnyNodeRef::ExprFString(fstring) => {
                self.add_range(fstring.range());
            }
            AnyNodeRef::ExprTString(tstring) => {
                self.add_range(tstring.range());
            }

            // Type parameter lists
            AnyNodeRef::TypeParams(params) => {
                self.add_range(params.range());
            }

            _ => {}
        }

        TraversalSignal::Traverse
    }

    fn visit_body(&mut self, body: &'_ [Stmt]) {
        // Handle import blocks in any body (module, function, class, etc.).
        self.add_import_ranges(body);
        walk_body(self, body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::CursorTest;
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};

    #[test]
    fn test_folding_range_class() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
class MyClass:
    def __init__(self):
        self.value = 1

    def method(self):
        return self.value
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / class MyClass:
        3 | |     def __init__(self):
        4 | |         self.value = 1
        5 | |
        6 | |     def method(self):
        7 | |         return self.value
          | |_________________________^
          |

        info[folding-range]: Folding Range
         --> main.py:3:5
          |
        2 |   class MyClass:
        3 | /     def __init__(self):
        4 | |         self.value = 1
          | |______________________^
        5 |
        6 |       def method(self):
          |

        info[folding-range]: Folding Range
         --> main.py:6:5
          |
        4 |           self.value = 1
        5 |
        6 | /     def method(self):
        7 | |         return self.value
          | |_________________________^
          |
        ");
    }

    #[test]
    fn test_folding_range_attribute_comments() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
class MyClass:
    def __init__(self):
        self.value = 1
        """
        This is an
        attribute comment.
        """
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r#"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / class MyClass:
        3 | |     def __init__(self):
        4 | |         self.value = 1
        5 | |         """
        6 | |         This is an
        7 | |         attribute comment.
        8 | |         """
          | |___________^
          |

        info[folding-range]: Folding Range
         --> main.py:3:5
          |
        2 |   class MyClass:
        3 | /     def __init__(self):
        4 | |         self.value = 1
        5 | |         """
        6 | |         This is an
        7 | |         attribute comment.
        8 | |         """
          | |___________^
          |

        info[folding-range]: Folding Range
         --> main.py:5:9
          |
        3 |       def __init__(self):
        4 |           self.value = 1
        5 | /         """
        6 | |         This is an
        7 | |         attribute comment.
        8 | |         """
          | |___________^
          |
        "#);
    }

    #[test]
    fn test_folding_range_imports_basic() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
import os
import sys
from typing import List, Dict
<CURSOR>
def main():
    pass
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range (imports)
         --> main.py:2:1
          |
        2 | / import os
        3 | | import sys
        4 | | from typing import List, Dict
          | |_____________________________^
        5 |
        6 |   def main():
          |

        info[folding-range]: Folding Range
         --> main.py:6:1
          |
        4 |   from typing import List, Dict
        5 |
        6 | / def main():
        7 | |     pass
          | |________^
          |
        ");
    }

    #[test]
    fn test_folding_range_imports_blocks1() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
import os
import sys

import numpy
import pandas
import requests

<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range (imports)
         --> main.py:2:1
          |
        2 | / import os
        3 | | import sys
          | |__________^
        4 |
        5 |   import numpy
          |

        info[folding-range]: Folding Range (imports)
         --> main.py:5:1
          |
        3 |   import sys
        4 |
        5 | / import numpy
        6 | | import pandas
        7 | | import requests
          | |_______________^
          |
        ");
    }

    #[test]
    fn test_folding_range_imports_blocks2() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
import os
from math import prod

try:
    import foo
    import bar
except ImportError:
    first = None
    bar = None

import requests
from fastapi import FastAPI

<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range (imports)
         --> main.py:2:1
          |
        2 | / import os
        3 | | from math import prod
          | |_____________________^
        4 |
        5 |   try:
          |

        info[folding-range]: Folding Range (imports)
          --> main.py:12:1
           |
        10 |       bar = None
        11 |
        12 | / import requests
        13 | | from fastapi import FastAPI
           | |___________________________^
           |

        info[folding-range]: Folding Range
         --> main.py:5:1
          |
        3 |   from math import prod
        4 |
        5 | / try:
        6 | |     import foo
        7 | |     import bar
          | |______________^
        8 |   except ImportError:
        9 |       first = None
          |

        info[folding-range]: Folding Range (imports)
         --> main.py:6:5
          |
        5 |   try:
        6 | /     import foo
        7 | |     import bar
          | |______________^
        8 |   except ImportError:
        9 |       first = None
          |

        info[folding-range]: Folding Range
          --> main.py:8:1
           |
         6 |       import foo
         7 |       import bar
         8 | / except ImportError:
         9 | |     first = None
        10 | |     bar = None
           | |______________^
        11 |
        12 |   import requests
           |
        ");
    }

    #[test]
    fn test_folding_range_imports_nested() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
def my_function():
    import os
    import sys

    import numpy
    import pandas

    do_something()


class MyClass:
    import typing
    import collections
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / def my_function():
        3 | |     import os
        4 | |     import sys
        5 | |
        6 | |     import numpy
        7 | |     import pandas
        8 | |
        9 | |     do_something()
          | |__________________^
          |

        info[folding-range]: Folding Range (imports)
         --> main.py:3:5
          |
        2 |   def my_function():
        3 | /     import os
        4 | |     import sys
          | |______________^
        5 |
        6 |       import numpy
          |

        info[folding-range]: Folding Range (imports)
         --> main.py:6:5
          |
        4 |       import sys
        5 |
        6 | /     import numpy
        7 | |     import pandas
          | |_________________^
        8 |
        9 |       do_something()
          |

        info[folding-range]: Folding Range
          --> main.py:12:1
           |
        12 | / class MyClass:
        13 | |     import typing
        14 | |     import collections
           | |______________________^
           |

        info[folding-range]: Folding Range (imports)
          --> main.py:13:5
           |
        12 |   class MyClass:
        13 | /     import typing
        14 | |     import collections
           | |______________________^
           |
        ");
    }

    #[test]
    fn test_folding_range_control_flow() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
if condition:
    do_something()
elif other:
    do_other()
else:
    default()

for item in items:
    process(item)
else:
    okay()

while running:
    continue_work()
else:
    doit()

<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / if condition:
        3 | |     do_something()
          | |__________________^
        4 |   elif other:
        5 |       do_other()
          |

        info[folding-range]: Folding Range
         --> main.py:4:1
          |
        2 |   if condition:
        3 |       do_something()
        4 | / elif other:
        5 | |     do_other()
          | |______________^
        6 |   else:
        7 |       default()
          |

        info[folding-range]: Folding Range
         --> main.py:6:1
          |
        4 |   elif other:
        5 |       do_other()
        6 | / else:
        7 | |     default()
          | |_____________^
        8 |
        9 |   for item in items:
          |

        info[folding-range]: Folding Range
          --> main.py:9:1
           |
         7 |       default()
         8 |
         9 | / for item in items:
        10 | |     process(item)
           | |_________________^
        11 |   else:
        12 |       okay()
           |

        info[folding-range]: Folding Range
          --> main.py:14:1
           |
        12 |       okay()
        13 |
        14 | / while running:
        15 | |     continue_work()
           | |___________________^
        16 |   else:
        17 |       doit()
           |
        ");
    }

    #[test]
    fn test_folding_range_nested_control_flow() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
if condition:
    while running:
        do_this()
        and_that()
        if maybe:
            and_maybe_this()
            and_maybe_this()
            and_maybe_this()
            and_maybe_this()

<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range
          --> main.py:2:1
           |
         2 | / if condition:
         3 | |     while running:
         4 | |         do_this()
         5 | |         and_that()
         6 | |         if maybe:
         7 | |             and_maybe_this()
         8 | |             and_maybe_this()
         9 | |             and_maybe_this()
        10 | |             and_maybe_this()
           | |____________________________^
           |

        info[folding-range]: Folding Range
          --> main.py:3:5
           |
         2 |   if condition:
         3 | /     while running:
         4 | |         do_this()
         5 | |         and_that()
         6 | |         if maybe:
         7 | |             and_maybe_this()
         8 | |             and_maybe_this()
         9 | |             and_maybe_this()
        10 | |             and_maybe_this()
           | |____________________________^
           |

        info[folding-range]: Folding Range
          --> main.py:6:9
           |
         4 |           do_this()
         5 |           and_that()
         6 | /         if maybe:
         7 | |             and_maybe_this()
         8 | |             and_maybe_this()
         9 | |             and_maybe_this()
        10 | |             and_maybe_this()
           | |____________________________^
           |
        ");
    }

    #[test]
    fn test_folding_range_loop_else() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
for item in items:
    process(item)
    validate(item)
else:
    log_success()
    notify_complete()

while condition:
    do_work()
    check_status()
else:
    handle_done()
    cleanup_resources()
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / for item in items:
        3 | |     process(item)
        4 | |     validate(item)
          | |__________________^
        5 |   else:
        6 |       log_success()
          |

        info[folding-range]: Folding Range
         --> main.py:6:5
          |
        4 |       validate(item)
        5 |   else:
        6 | /     log_success()
        7 | |     notify_complete()
          | |_____________________^
        8 |
        9 |   while condition:
          |

        info[folding-range]: Folding Range
          --> main.py:9:1
           |
         7 |       notify_complete()
         8 |
         9 | / while condition:
        10 | |     do_work()
        11 | |     check_status()
           | |__________________^
        12 |   else:
        13 |       handle_done()
           |

        info[folding-range]: Folding Range
          --> main.py:13:5
           |
        11 |       check_status()
        12 |   else:
        13 | /     handle_done()
        14 | |     cleanup_resources()
           | |_______________________^
           |
        ");
    }

    #[test]
    fn test_folding_range_try_except() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
try:
    risky_operation()
except ValueError:
    handle_value_error()
except TypeError:
    handle_type_error()
else:
    success_action()
finally:
    cleanup()
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / try:
        3 | |     risky_operation()
          | |_____________________^
        4 |   except ValueError:
        5 |       handle_value_error()
          |

        info[folding-range]: Folding Range
          --> main.py:9:5
           |
         7 |     handle_type_error()
         8 | else:
         9 |     success_action()
           |     ^^^^^^^^^^^^^^^^
        10 | finally:
        11 |     cleanup()
           |

        info[folding-range]: Folding Range
          --> main.py:11:5
           |
         9 |     success_action()
        10 | finally:
        11 |     cleanup()
           |     ^^^^^^^^^
           |

        info[folding-range]: Folding Range
         --> main.py:4:1
          |
        2 |   try:
        3 |       risky_operation()
        4 | / except ValueError:
        5 | |     handle_value_error()
          | |________________________^
        6 |   except TypeError:
        7 |       handle_type_error()
          |

        info[folding-range]: Folding Range
         --> main.py:6:1
          |
        4 |   except ValueError:
        5 |       handle_value_error()
        6 | / except TypeError:
        7 | |     handle_type_error()
          | |_______________________^
        8 |   else:
        9 |       success_action()
          |
        ");
    }

    #[test]
    fn test_folding_range_collections() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
my_list = [
    1,
    2,
    3,
]

my_dict = {
    "a": 1,
    "b": 2,
}
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r#"
        info[folding-range]: Folding Range
         --> main.py:2:11
          |
        2 |   my_list = [
          |  ___________^
        3 | |     1,
        4 | |     2,
        5 | |     3,
        6 | | ]
          | |_^
        7 |
        8 |   my_dict = {
          |

        info[folding-range]: Folding Range
          --> main.py:8:11
           |
         6 |   ]
         7 |
         8 |   my_dict = {
           |  ___________^
         9 | |     "a": 1,
        10 | |     "b": 2,
        11 | | }
           | |_^
           |
        "#);
    }

    #[test]
    fn test_folding_range_string_literals() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
multiline_string = """
This is a
multiline string
"""

multiline_bytes = b"""
This is
multiline bytes
"""

multiline_fstring = f"""
This is a
multiline f-string
"""

multiline_tstring = t"""
This is a
multiline t-string
"""
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r#"
        info[folding-range]: Folding Range
         --> main.py:2:20
          |
        2 |   multiline_string = """
          |  ____________________^
        3 | | This is a
        4 | | multiline string
        5 | | """
          | |___^
        6 |
        7 |   multiline_bytes = b"""
          |

        info[folding-range]: Folding Range
          --> main.py:7:19
           |
         5 |   """
         6 |
         7 |   multiline_bytes = b"""
           |  ___________________^
         8 | | This is
         9 | | multiline bytes
        10 | | """
           | |___^
        11 |
        12 |   multiline_fstring = f"""
           |

        info[folding-range]: Folding Range
          --> main.py:12:21
           |
        10 |   """
        11 |
        12 |   multiline_fstring = f"""
           |  _____________________^
        13 | | This is a
        14 | | multiline f-string
        15 | | """
           | |___^
        16 |
        17 |   multiline_tstring = t"""
           |

        info[folding-range]: Folding Range
          --> main.py:17:21
           |
        15 |   """
        16 |
        17 |   multiline_tstring = t"""
           |  _____________________^
        18 | | This is a
        19 | | multiline t-string
        20 | | """
           | |___^
           |
        "#);
    }

    #[test]
    fn test_folding_range_match() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
match value:
    case 1:
        one()
    case 2:
        two()
    case _:
        default()
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / match value:
        3 | |     case 1:
        4 | |         one()
        5 | |     case 2:
        6 | |         two()
        7 | |     case _:
        8 | |         default()
          | |_________________^
          |

        info[folding-range]: Folding Range
         --> main.py:3:5
          |
        2 |   match value:
        3 | /     case 1:
        4 | |         one()
          | |_____________^
        5 |       case 2:
        6 |           two()
          |

        info[folding-range]: Folding Range
         --> main.py:5:5
          |
        3 |       case 1:
        4 |           one()
        5 | /     case 2:
        6 | |         two()
          | |_____________^
        7 |       case _:
        8 |           default()
          |

        info[folding-range]: Folding Range
         --> main.py:7:5
          |
        5 |       case 2:
        6 |           two()
        7 | /     case _:
        8 | |         default()
          | |_________________^
          |
        ");
    }

    #[test]
    fn test_folding_range_regions() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
# region Imports
import os
import sys
# endregion

# region Main
def main():
    pass
# endregion
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range (imports)
         --> main.py:3:1
          |
        2 |   # region Imports
        3 | / import os
        4 | | import sys
          | |__________^
        5 |   # endregion
          |

        info[folding-range]: Folding Range
          --> main.py:8:1
           |
         7 |   # region Main
         8 | / def main():
         9 | |     pass
           | |________^
        10 |   # endregion
           |

        info[folding-range]: Folding Range (region)
         --> main.py:2:1
          |
        2 | / # region Imports
        3 | | import os
        4 | | import sys
        5 | | # endregion
          | |___________^
        6 |
        7 |   # region Main
          |

        info[folding-range]: Folding Range (region)
          --> main.py:7:1
           |
         5 |   # endregion
         6 |
         7 | / # region Main
         8 | | def main():
         9 | |     pass
        10 | | # endregion
           | |___________^
           |
        ");
    }

    #[test]
    fn test_folding_range_docstring() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
def my_function():
    """
    This is a multiline
    docstring.
    """
    pass
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r#"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / def my_function():
        3 | |     """
        4 | |     This is a multiline
        5 | |     docstring.
        6 | |     """
        7 | |     pass
          | |________^
          |

        info[folding-range]: Folding Range (comment)
         --> main.py:3:5
          |
        2 |   def my_function():
        3 | /     """
        4 | |     This is a multiline
        5 | |     docstring.
        6 | |     """
          | |_______^
        7 |       pass
          |

        info[folding-range]: Folding Range
         --> main.py:3:5
          |
        2 |   def my_function():
        3 | /     """
        4 | |     This is a multiline
        5 | |     docstring.
        6 | |     """
          | |_______^
        7 |       pass
          |
        "#);
    }

    #[test]
    fn test_folding_range_docstring_variants() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
def with_fstring_doc():
    f"""
    This is an f-string
    used as a docstring.
    """
    pass


def with_tstring_doc():
    t"""
    This is a t-string
    used as a docstring.
    """
    pass


def with_rawstring_doc():
    r"""
    This is a raw string
    used as a docstring.
    """
    pass

<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r#"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / def with_fstring_doc():
        3 | |     f"""
        4 | |     This is an f-string
        5 | |     used as a docstring.
        6 | |     """
        7 | |     pass
          | |________^
          |

        info[folding-range]: Folding Range (comment)
         --> main.py:3:5
          |
        2 |   def with_fstring_doc():
        3 | /     f"""
        4 | |     This is an f-string
        5 | |     used as a docstring.
        6 | |     """
          | |_______^
        7 |       pass
          |

        info[folding-range]: Folding Range
         --> main.py:3:5
          |
        2 |   def with_fstring_doc():
        3 | /     f"""
        4 | |     This is an f-string
        5 | |     used as a docstring.
        6 | |     """
          | |_______^
        7 |       pass
          |

        info[folding-range]: Folding Range
          --> main.py:10:1
           |
        10 | / def with_tstring_doc():
        11 | |     t"""
        12 | |     This is a t-string
        13 | |     used as a docstring.
        14 | |     """
        15 | |     pass
           | |________^
           |

        info[folding-range]: Folding Range (comment)
          --> main.py:11:5
           |
        10 |   def with_tstring_doc():
        11 | /     t"""
        12 | |     This is a t-string
        13 | |     used as a docstring.
        14 | |     """
           | |_______^
        15 |       pass
           |

        info[folding-range]: Folding Range
          --> main.py:11:5
           |
        10 |   def with_tstring_doc():
        11 | /     t"""
        12 | |     This is a t-string
        13 | |     used as a docstring.
        14 | |     """
           | |_______^
        15 |       pass
           |

        info[folding-range]: Folding Range
          --> main.py:18:1
           |
        18 | / def with_rawstring_doc():
        19 | |     r"""
        20 | |     This is a raw string
        21 | |     used as a docstring.
        22 | |     """
        23 | |     pass
           | |________^
           |

        info[folding-range]: Folding Range (comment)
          --> main.py:19:5
           |
        18 |   def with_rawstring_doc():
        19 | /     r"""
        20 | |     This is a raw string
        21 | |     used as a docstring.
        22 | |     """
           | |_______^
        23 |       pass
           |

        info[folding-range]: Folding Range
          --> main.py:19:5
           |
        18 |   def with_rawstring_doc():
        19 | /     r"""
        20 | |     This is a raw string
        21 | |     used as a docstring.
        22 | |     """
           | |_______^
        23 |       pass
           |
        "#);
    }

    #[test]
    fn test_folding_range_comments() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
# This is a comment block
# that spans multiple lines
# explaining something important

def foo():
    pass

# Another comment block
# with more details
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(
            test.folding_ranges(),
            @r"
        info[folding-range]: Folding Range
         --> main.py:6:1
          |
        4 |   # explaining something important
        5 |
        6 | / def foo():
        7 | |     pass
          | |________^
        8 |
        9 |   # Another comment block
          |

        info[folding-range]: Folding Range (comment)
         --> main.py:2:1
          |
        2 | / # This is a comment block
        3 | | # that spans multiple lines
        4 | | # explaining something important
          | |________________________________^
        5 |
        6 |   def foo():
          |

        info[folding-range]: Folding Range (comment)
          --> main.py:9:1
           |
         7 |       pass
         8 |
         9 | / # Another comment block
        10 | | # with more details
           | |___________________^
           |
        ",
        );
    }

    #[test]
    fn test_folding_range_with() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
with open("file.txt") as f:
    content = f.read()
    process(content)
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r#"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / with open("file.txt") as f:
        3 | |     content = f.read()
        4 | |     process(content)
          | |____________________^
          |
        "#);
    }

    #[test]
    fn test_folding_multiline() {
        // A class definition on a single line shouldn't have
        // any folding ranges.
        let test = CursorTest::builder()
            .source("main.py", "class MyClass: pass\n<CURSOR>")
            .build();
        assert_snapshot!(test.folding_ranges(), @"No folding ranges found");

        // A single LF new-line results in a folding range.
        let test = CursorTest::builder()
            .source("main.py", "class MyClass:\n    pass\n<CURSOR>")
            .build();
        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range
         --> main.py:1:1
          |
        1 | / class MyClass:
        2 | |     pass
          | |________^
          |
        ");

        // So does a single CRLF new-line.
        let test = CursorTest::builder()
            .source("main.py", "class MyClass:\r\n    pass\r\n<CURSOR>")
            .build();
        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range
         --> main.py:1:1
          |
        1 | / class MyClass:
        2 | |     pass
          | |________^
          |
        ");

        // And so to does a single CR new-line.
        let test = CursorTest::builder()
            .source("main.py", "class MyClass:\r    pass\r<CURSOR>")
            .build();
        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range
         --> main.py:1:1
          |
        1 | / class MyClass:
        2 | |     pass
          | |________^
          |
        ");
    }

    impl CursorTest {
        fn folding_ranges(&self) -> String {
            let ranges = folding_ranges(&self.db, self.cursor.file);

            if ranges.is_empty() {
                return "No folding ranges found".to_string();
            }

            let diagnostics: Vec<FoldingRangeDiagnostic> = ranges
                .into_iter()
                .map(|fr| FoldingRangeDiagnostic::new(self.cursor.file, fr))
                .collect();

            self.render_diagnostics(diagnostics)
        }
    }

    struct FoldingRangeDiagnostic {
        file: File,
        folding_range: FoldingRange,
    }

    impl FoldingRangeDiagnostic {
        fn new(file: File, folding_range: FoldingRange) -> Self {
            Self {
                file,
                folding_range,
            }
        }
    }

    impl crate::tests::IntoDiagnostic for FoldingRangeDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let message = match self.folding_range.kind {
                Some(FoldingRangeKind::Comment) => "Folding Range (comment)",
                Some(FoldingRangeKind::Imports) => "Folding Range (imports)",
                Some(FoldingRangeKind::Region) => "Folding Range (region)",
                None => "Folding Range",
            };

            let mut diagnostic = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("folding-range")),
                Severity::Info,
                message.to_string(),
            );

            diagnostic.annotate(Annotation::primary(
                Span::from(self.file).with_range(self.folding_range.range),
            ));

            diagnostic
        }
    }
}
