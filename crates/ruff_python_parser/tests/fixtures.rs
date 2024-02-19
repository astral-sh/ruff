use std::cmp::Ordering;
use std::fmt::{Formatter, Write};
use std::fs;
use std::path::Path;

use annotate_snippets::display_list::{DisplayList, FormatOptions};
use annotate_snippets::snippet::{AnnotationType, Slice, Snippet, SourceAnnotation};
use ruff_python_ast::visitor::preorder::{walk_module, PreorderVisitor, TraversalSignal};
use ruff_python_ast::{AnyNodeRef, Mod};

use ruff_python_parser::{Mode, ParseErrorType, Program};
use ruff_source_file::{LineIndex, OneIndexed, SourceCode};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

#[test]
fn invalid_syntax() {
    let test_file = |input_path: &Path| {
        let source = fs::read_to_string(input_path).expect("Expected test file to exist");

        let program = Program::parse_str(&source, Mode::Module);

        assert_ne!(
            &program.parse_errors,
            &[],
            "{input_path:?}: Expected parser to generate at least one syntax error for a program containing syntax errors."
        );

        validate_ast(&program.ast, source.text_len(), input_path);

        let mut output = String::new();
        writeln!(&mut output, "## AST").unwrap();
        writeln!(&mut output, "\n ```\n{:#?}\n```", &program.ast).unwrap();

        writeln!(&mut output, "## Errors\n").unwrap();

        let line_index = LineIndex::from_source_text(&source);
        let source_code = SourceCode::new(&source, &line_index);

        for error in &program.parse_errors {
            writeln!(
                &mut output,
                "{}\n",
                CodeFrame {
                    range: error.location,
                    error,
                    source_code: &source_code,
                }
            )
            .unwrap();
        }

        insta::with_settings!({
            omit_expression => true,
            input_file => input_path,
            prepend_module_to_snapshot => false,
        }, {
            insta::assert_snapshot!(output);
        });
    };

    insta::glob!("../resources", "invalid/**/*.{py,pyi}", test_file);
}

#[test]
fn valid_syntax() {
    let test_file = |input_path: &Path| {
        let source = fs::read_to_string(input_path).expect("Expected test file to exist");

        let program = Program::parse_str(&source, Mode::Module);

        if !program.parse_errors.is_empty() {
            let line_index = LineIndex::from_source_text(&source);
            let source_code = SourceCode::new(&source, &line_index);

            let mut message = "Expected no syntax errors for a valid program but the parser generated the following errors:\n".to_string();

            for error in &program.parse_errors {
                writeln!(
                    &mut message,
                    "{}\n",
                    CodeFrame {
                        range: error.location,
                        error,
                        source_code: &source_code,
                    }
                )
                .unwrap();
            }

            panic!("{input_path:?}: {message}");
        }

        validate_ast(&program.ast, source.text_len(), input_path);

        let mut output = String::new();
        writeln!(&mut output, "## AST").unwrap();
        writeln!(&mut output, "\n ```\n{:#?}\n```", &program.ast).unwrap();

        insta::with_settings!({
            omit_expression => true,
            input_file => input_path,
            prepend_module_to_snapshot => false,
        }, {
            insta::assert_snapshot!(output);
        });
    };

    insta::glob!("../resources", "valid/**/*.{py,pyi}", test_file);
}

struct CodeFrame<'a> {
    range: TextRange,
    error: &'a ParseErrorType,
    source_code: &'a SourceCode<'a, 'a>,
}

impl std::fmt::Display for CodeFrame<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Copied and modified from ruff_linter/src/message/text.rs
        let content_start_index = self.source_code.line_index(self.range.start());
        let mut start_index = content_start_index.saturating_sub(2);

        // Trim leading empty lines.
        while start_index < content_start_index {
            if !self.source_code.line_text(start_index).trim().is_empty() {
                break;
            }
            start_index = start_index.saturating_add(1);
        }

        let content_end_index = self.source_code.line_index(self.range.end());
        let mut end_index = content_end_index
            .saturating_add(2)
            .min(OneIndexed::from_zero_indexed(self.source_code.line_count()));

        // Trim trailing empty lines.
        while end_index > content_end_index {
            if !self.source_code.line_text(end_index).trim().is_empty() {
                break;
            }

            end_index = end_index.saturating_sub(1);
        }

        let start_offset = self.source_code.line_start(start_index);
        let end_offset = self.source_code.line_end(end_index);

        let annotation_range = self.range - start_offset;
        let source = self
            .source_code
            .slice(TextRange::new(start_offset, end_offset));

        let start_char = source[TextRange::up_to(annotation_range.start())]
            .chars()
            .count();

        let char_length = source[annotation_range].chars().count();
        let label = format!("Syntax Error: {error}", error = self.error);

        let snippet = Snippet {
            title: None,
            slices: vec![Slice {
                source,
                line_start: start_index.get(),
                annotations: vec![SourceAnnotation {
                    label: &label,
                    annotation_type: AnnotationType::Error,
                    range: (start_char, start_char + char_length),
                }],
                // The origin (file name, line number, and column number) is already encoded
                // in the `label`.
                origin: None,
                fold: false,
            }],
            footer: Vec::new(),
            opt: FormatOptions::default(),
        };

        writeln!(f, "{message}", message = DisplayList::from(snippet))
    }
}

/// Verifies that:
/// * the range of the parent node fully encloses all its child nodes
/// * the ranges are strictly increasing when traversing the nodes in pre-order.
/// * all ranges are within the length of the source code.
fn validate_ast(root: &Mod, source_len: TextSize, test_path: &Path) {
    walk_module(&mut ValidateAstVisitor::new(source_len, test_path), root);
}

#[derive(Debug)]
struct ValidateAstVisitor<'a> {
    parents: Vec<AnyNodeRef<'a>>,
    previous: Option<AnyNodeRef<'a>>,
    source_length: TextSize,
    test_path: &'a Path,
}

impl<'a> ValidateAstVisitor<'a> {
    fn new(source_length: TextSize, test_path: &'a Path) -> Self {
        Self {
            parents: Vec::new(),
            previous: None,
            source_length,
            test_path,
        }
    }
}

impl<'ast> PreorderVisitor<'ast> for ValidateAstVisitor<'ast> {
    fn enter_node(&mut self, node: AnyNodeRef<'ast>) -> TraversalSignal {
        assert!(
            node.end() <= self.source_length,
            "{path}: The range of the node exceeds the length of the source code. Node: {node:#?}",
            path = self.test_path.display()
        );

        if let Some(previous) = self.previous {
            assert_ne!(previous.range().ordering(node.range()),  Ordering::Greater,
                    "{path}: The ranges of the nodes are not strictly increasing when traversing the AST in pre-order.\nPrevious node: {previous:#?}\n\nCurrent node: {node:#?}\n\nRoot: {root:#?}",
                    path = self.test_path.display(),
                    root = self.parents.first()
                );
        }

        if let Some(parent) = self.parents.last() {
            assert!(parent.range().contains_range(node.range()),
                    "{path}: The range of the parent node does not fully enclose the range of the child node.\nParent node: {parent:#?}\n\nChild node: {node:#?}\n\nRoot: {root:#?}",
                    path = self.test_path.display(),
                    root = self.parents.first()
                );
        }

        self.parents.push(node);

        TraversalSignal::Traverse
    }

    fn leave_node(&mut self, node: AnyNodeRef<'ast>) {
        self.parents.pop().expect("Expected tree to be balanced");

        self.previous = Some(node);
    }
}
