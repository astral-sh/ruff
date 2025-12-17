use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::find_node::covering_node;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::Db;

/// Returns a list of nested selection ranges, where each range contains the next one.
/// The first range in the list is the largest range containing the cursor position.
pub fn selection_range(db: &dyn Db, file: File, offset: TextSize) -> Vec<TextRange> {
    let parsed = parsed_module(db, file).load(db);
    let range = TextRange::new(offset, offset);

    let covering = covering_node(parsed.syntax().into(), range);

    let mut ranges = Vec::new();
    // Start with the largest range (the root), so iterate ancestors backwards
    for node in covering.ancestors().rev() {
        if should_include_in_selection(node) {
            let range = node.range();
            // Eliminate duplicates when parent and child nodes have the same range
            if ranges.last() != Some(&range) {
                ranges.push(range);
            }
        }
    }

    ranges
}

/// Determines if a node should be included in the selection range hierarchy.
/// This filters out intermediate nodes that don't provide meaningful selections.
fn should_include_in_selection(node: ruff_python_ast::AnyNodeRef) -> bool {
    use ruff_python_ast::AnyNodeRef;

    // We will likely need to tune this based on user feedback. Some users may
    // prefer finer-grained selections while others may prefer coarser-grained.
    match node {
        // Exclude nodes that don't represent meaningful semantic units for selection
        AnyNodeRef::StmtExpr(_) => false,

        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::CursorTest;
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};
    use ruff_db::files::FileRange;
    use ruff_text_size::Ranged;

    /// Test selection range on a simple expression
    #[test]
    fn test_selection_range_simple_expression() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
x = 1 + <CURSOR>2
",
            )
            .build();

        assert_snapshot!(test.selection_range(), @r"
        info[selection-range]: Selection Range 0
         --> main.py:1:1
          |
        1 | /
        2 | | x = 1 + 2
          | |__________^
          |

        info[selection-range]: Selection Range 1
         --> main.py:2:1
          |
        2 | x = 1 + 2
          | ^^^^^^^^^
          |

        info[selection-range]: Selection Range 2
         --> main.py:2:5
          |
        2 | x = 1 + 2
          |     ^^^^^
          |

        info[selection-range]: Selection Range 3
         --> main.py:2:9
          |
        2 | x = 1 + 2
          |         ^
          |
        ");
    }

    /// Test selection range on a function call
    #[test]
    fn test_selection_range_function_call() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
print(\"he<CURSOR>llo\")
",
            )
            .build();

        assert_snapshot!(test.selection_range(), @r#"
        info[selection-range]: Selection Range 0
         --> main.py:1:1
          |
        1 | /
        2 | | print("hello")
          | |_______________^
          |

        info[selection-range]: Selection Range 1
         --> main.py:2:1
          |
        2 | print("hello")
          | ^^^^^^^^^^^^^^
          |

        info[selection-range]: Selection Range 2
         --> main.py:2:6
          |
        2 | print("hello")
          |      ^^^^^^^^^
          |

        info[selection-range]: Selection Range 3
         --> main.py:2:7
          |
        2 | print("hello")
          |       ^^^^^^^
          |
        "#);
    }

    /// Test selection range on a function definition
    #[test]
    fn test_selection_range_function_definition() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
def my_<CURSOR>function():
    return 42
",
            )
            .build();

        assert_snapshot!(test.selection_range(), @r"
        info[selection-range]: Selection Range 0
         --> main.py:1:1
          |
        1 | /
        2 | | def my_function():
        3 | |     return 42
          | |______________^
          |

        info[selection-range]: Selection Range 1
         --> main.py:2:1
          |
        2 | / def my_function():
        3 | |     return 42
          | |_____________^
          |

        info[selection-range]: Selection Range 2
         --> main.py:2:5
          |
        2 | def my_function():
          |     ^^^^^^^^^^^
        3 |     return 42
          |
        ");
    }

    /// Test selection range on a class definition
    #[test]
    fn test_selection_range_class_definition() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
class My<CURSOR>Class:
    def __init__(self):
        self.value = 1
",
            )
            .build();

        assert_snapshot!(test.selection_range(), @r"
        info[selection-range]: Selection Range 0
         --> main.py:1:1
          |
        1 | /
        2 | | class MyClass:
        3 | |     def __init__(self):
        4 | |         self.value = 1
          | |_______________________^
          |

        info[selection-range]: Selection Range 1
         --> main.py:2:1
          |
        2 | / class MyClass:
        3 | |     def __init__(self):
        4 | |         self.value = 1
          | |______________________^
          |

        info[selection-range]: Selection Range 2
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
        3 |     def __init__(self):
        4 |         self.value = 1
          |
        ");
    }

    /// Test selection range on a deeply nested expression with comprehension, lambda, and subscript
    #[test]
    fn test_selection_range_deeply_nested_expression() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
result = [(lambda x: x[key.<CURSOR>attr])(item) for item in data if item is not None]
",
            )
            .build();

        assert_snapshot!(test.selection_range(), @r"
        info[selection-range]: Selection Range 0
         --> main.py:1:1
          |
        1 | /
        2 | | result = [(lambda x: x[key.attr])(item) for item in data if item is not None]
          | |______________________________________________________________________________^
          |

        info[selection-range]: Selection Range 1
         --> main.py:2:1
          |
        2 | result = [(lambda x: x[key.attr])(item) for item in data if item is not None]
          | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
          |

        info[selection-range]: Selection Range 2
         --> main.py:2:10
          |
        2 | result = [(lambda x: x[key.attr])(item) for item in data if item is not None]
          |          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
          |

        info[selection-range]: Selection Range 3
         --> main.py:2:11
          |
        2 | result = [(lambda x: x[key.attr])(item) for item in data if item is not None]
          |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
          |

        info[selection-range]: Selection Range 4
         --> main.py:2:12
          |
        2 | result = [(lambda x: x[key.attr])(item) for item in data if item is not None]
          |            ^^^^^^^^^^^^^^^^^^^^^
          |

        info[selection-range]: Selection Range 5
         --> main.py:2:22
          |
        2 | result = [(lambda x: x[key.attr])(item) for item in data if item is not None]
          |                      ^^^^^^^^^^^
          |

        info[selection-range]: Selection Range 6
         --> main.py:2:24
          |
        2 | result = [(lambda x: x[key.attr])(item) for item in data if item is not None]
          |                        ^^^^^^^^
          |

        info[selection-range]: Selection Range 7
         --> main.py:2:28
          |
        2 | result = [(lambda x: x[key.attr])(item) for item in data if item is not None]
          |                            ^^^^
          |
        ");
    }

    impl CursorTest {
        fn selection_range(&self) -> String {
            let ranges = selection_range(&self.db, self.cursor.file, self.cursor.offset);

            if ranges.is_empty() {
                return "No selection range found".to_string();
            }

            // Create one diagnostic per range for clearer visualization
            let diagnostics: Vec<SelectionRangeDiagnostic> = ranges
                .iter()
                .enumerate()
                .map(|(index, &range)| {
                    SelectionRangeDiagnostic::new(FileRange::new(self.cursor.file, range), index)
                })
                .collect();

            self.render_diagnostics(diagnostics)
        }
    }

    struct SelectionRangeDiagnostic {
        range: FileRange,
        index: usize,
    }

    impl SelectionRangeDiagnostic {
        fn new(range: FileRange, index: usize) -> Self {
            Self { range, index }
        }
    }

    impl crate::tests::IntoDiagnostic for SelectionRangeDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let mut diagnostic = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("selection-range")),
                Severity::Info,
                format!("Selection Range {}", self.index),
            );

            diagnostic.annotate(Annotation::primary(
                Span::from(self.range.file()).with_range(self.range.range()),
            ));

            diagnostic
        }
    }
}
