use crate::goto::find_goto_target;
use crate::references::{ReferencesMode, references};
use crate::{Db, ReferenceTarget};
use ruff_db::files::File;
use ruff_text_size::TextSize;
use ty_python_semantic::SemanticModel;

/// Find all document highlights for a symbol at the given position.
/// Document highlights are limited to the current file only.
pub fn document_highlights(
    db: &dyn Db,
    file: File,
    offset: TextSize,
) -> Option<Vec<ReferenceTarget>> {
    let parsed = ruff_db::parsed::parsed_module(db, file);
    let module = parsed.load(db);
    let model = SemanticModel::new(db, file);

    // Get the definitions for the symbol at the cursor position
    let goto_target = find_goto_target(&model, &module, offset)?;

    // Use DocumentHighlights mode which limits search to current file only
    references(db, file, &goto_target, ReferencesMode::DocumentHighlights)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{CursorTest, IntoDiagnostic, cursor_test};
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};
    use ruff_db::files::FileRange;
    use ruff_text_size::Ranged;

    impl CursorTest {
        fn document_highlights(&self) -> String {
            let Some(highlight_results) =
                document_highlights(&self.db, self.cursor.file, self.cursor.offset)
            else {
                return "No highlights found".to_string();
            };

            if highlight_results.is_empty() {
                return "No highlights found".to_string();
            }

            self.render_diagnostics(highlight_results.into_iter().enumerate().map(
                |(i, highlight_item)| -> HighlightResult {
                    HighlightResult {
                        index: i,
                        file_range: FileRange::new(highlight_item.file(), highlight_item.range()),
                        kind: highlight_item.kind(),
                    }
                },
            ))
        }
    }

    struct HighlightResult {
        index: usize,
        file_range: FileRange,
        kind: crate::ReferenceKind,
    }

    impl IntoDiagnostic for HighlightResult {
        fn into_diagnostic(self) -> Diagnostic {
            let kind_str = match self.kind {
                crate::ReferenceKind::Read => "Read",
                crate::ReferenceKind::Write => "Write",
                crate::ReferenceKind::Other => "Other",
            };
            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("document_highlights")),
                Severity::Info,
                format!("Highlight {} ({})", self.index + 1, kind_str),
            );
            main.annotate(Annotation::primary(
                Span::from(self.file_range.file()).with_range(self.file_range.range()),
            ));

            main
        }
    }

    #[test]
    fn test_local_variable_highlights() {
        let test = cursor_test(
            "
def calculate_sum():
    <CURSOR>value = 10
    doubled = value * 2
    result = value + doubled
    return value
",
        );

        assert_snapshot!(test.document_highlights(), @r"
        info[document_highlights]: Highlight 1 (Write)
         --> main.py:3:5
          |
        2 | def calculate_sum():
        3 |     value = 10
          |     ^^^^^
        4 |     doubled = value * 2
        5 |     result = value + doubled
          |

        info[document_highlights]: Highlight 2 (Read)
         --> main.py:4:15
          |
        2 | def calculate_sum():
        3 |     value = 10
        4 |     doubled = value * 2
          |               ^^^^^
        5 |     result = value + doubled
        6 |     return value
          |

        info[document_highlights]: Highlight 3 (Read)
         --> main.py:5:14
          |
        3 |     value = 10
        4 |     doubled = value * 2
        5 |     result = value + doubled
          |              ^^^^^
        6 |     return value
          |

        info[document_highlights]: Highlight 4 (Read)
         --> main.py:6:12
          |
        4 |     doubled = value * 2
        5 |     result = value + doubled
        6 |     return value
          |            ^^^^^
          |
        ");
    }

    #[test]
    fn test_parameter_highlights() {
        let test = cursor_test(
            "
def process_data(<CURSOR>data):
    if data:
        processed = data.upper()
        return processed
    return data
",
        );

        assert_snapshot!(test.document_highlights(), @r"
        info[document_highlights]: Highlight 1 (Other)
         --> main.py:2:18
          |
        2 | def process_data(data):
          |                  ^^^^
        3 |     if data:
        4 |         processed = data.upper()
          |

        info[document_highlights]: Highlight 2 (Read)
         --> main.py:3:8
          |
        2 | def process_data(data):
        3 |     if data:
          |        ^^^^
        4 |         processed = data.upper()
        5 |         return processed
          |

        info[document_highlights]: Highlight 3 (Read)
         --> main.py:4:21
          |
        2 | def process_data(data):
        3 |     if data:
        4 |         processed = data.upper()
          |                     ^^^^
        5 |         return processed
        6 |     return data
          |

        info[document_highlights]: Highlight 4 (Read)
         --> main.py:6:12
          |
        4 |         processed = data.upper()
        5 |         return processed
        6 |     return data
          |            ^^^^
          |
        ");
    }

    #[test]
    fn test_class_name_highlights() {
        let test = cursor_test(
            "
class <CURSOR>Calculator:
    def __init__(self):
        self.name = 'Calculator'

calc = Calculator()
",
        );

        assert_snapshot!(test.document_highlights(), @r"
        info[document_highlights]: Highlight 1 (Other)
         --> main.py:2:7
          |
        2 | class Calculator:
          |       ^^^^^^^^^^
        3 |     def __init__(self):
        4 |         self.name = 'Calculator'
          |

        info[document_highlights]: Highlight 2 (Read)
         --> main.py:6:8
          |
        4 |         self.name = 'Calculator'
        5 |
        6 | calc = Calculator()
          |        ^^^^^^^^^^
          |
        ");
    }

    #[test]
    fn test_no_highlights_for_unknown_symbol() {
        let test = cursor_test(
            "
def test():
    # Cursor on a position with no symbol
    <CURSOR>
",
        );

        assert_snapshot!(test.document_highlights(), @"No highlights found");
    }

    // TODO: Should only highlight the last use and the last declaration
    #[test]
    fn redeclarations() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
                a: str = "test"

                a: int = 10

                print(a<CURSOR>)
                "#,
            )
            .build();

        assert_snapshot!(test.document_highlights(), @r#"
        info[document_highlights]: Highlight 1 (Write)
         --> main.py:2:1
          |
        2 | a: str = "test"
          | ^
        3 |
        4 | a: int = 10
          |

        info[document_highlights]: Highlight 2 (Write)
         --> main.py:4:1
          |
        2 | a: str = "test"
        3 |
        4 | a: int = 10
          | ^
        5 |
        6 | print(a)
          |

        info[document_highlights]: Highlight 3 (Read)
         --> main.py:6:7
          |
        4 | a: int = 10
        5 |
        6 | print(a)
          |       ^
          |
        "#);
    }
}
