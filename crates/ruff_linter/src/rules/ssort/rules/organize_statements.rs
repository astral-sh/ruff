use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::rules::ssort::dependencies::{Dependencies, nodes_from_suite};
use crate::{FixAvailability, Locator, Violation};

/// ## What it does
///
/// Groups and sorts statements based on the order in which they are referenced.
///
/// ## Why is this bad?
///
/// Consistency is good. Use a common convention for statement ordering to make your code more
/// readable and idiomatic.
///
/// ## Example
///
/// ```python
/// def foo():
///     bar()
///
///
/// def baz():
///     pass
///
///
/// def bar():
///     baz()
/// ```
///
/// Use instead:
///
/// ```python
/// def baz():
///     pass
///
///
/// def bar():
///     baz()
///
///
/// def foo():
///     bar()
/// ```
///
/// ## Limitations
///
/// This rule will not sort bodies containing circular dependencies, as they cannot be
/// reordered while preserving the dependency relationships. However, nested bodies within
/// those statements may still be sorted independently.
///
/// For example, the top-level functions `a` and `b` will not be sorted due to their circular
/// dependency, but the class `C` will have its methods sorted:
///
/// ```python
/// def a():
///     return b()
///
///
/// def b():
///     return a()
///
///
/// class C:
///     def method_b(self):
///         return self.method_a()
///
///     def method_a(self):
///         pass
/// ```
///
/// After applying this rule:
///
/// ```python
/// def a():
///     return b()
///
///
/// def b():
///     return a()
///
///
/// class C:
///     def method_a(self):
///         pass
///
///     def method_b(self):
///         return self.method_a()
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.11")]
pub(crate) struct UnsortedStatements;

/// Allows `UnsortedStatements` to be treated as a Violation.
impl Violation for UnsortedStatements {
    /// Fix is sometimes available.
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    /// The message used to describe the violation.
    ///
    /// ## Returns
    /// A string describing the violation.
    #[derive_message_formats]
    fn message(&self) -> String {
        "Statements are unsorted".to_string()
    }

    /// Returns the title for the fix.
    ///
    /// ## Returns
    /// A string describing the fix title.
    fn fix_title(&self) -> Option<String> {
        Some("Organize statements".to_string())
    }
}

/// Get the replacement text for a statement, organizing its body if it is a class definition.
///
/// ## Arguments
/// * `locator` - The locator for the source code.
/// * `stmt` - The statement to get the replacement text for.
/// * `narrative_order` - Whether to use a narrative-oriented order for the result.
///
/// ## Returns
/// A string representing the replacement text for the statement.
fn get_replacement_text(locator: &Locator, stmt: &Stmt, narrative_order: bool) -> String {
    // If the statement is a class, get its definition, otherwise, return the original text
    let original_text = locator.slice(stmt.range()).to_string();
    let Stmt::ClassDef(class_def) = stmt else {
        return original_text;
    };

    // Construct the replacement text by sorting the class body
    let inner_replacement = organize_suite(&class_def.body, locator, narrative_order);

    // Compute the range of the body text and insert the replacement text between them
    let body_range = TextRange::new(
        class_def.body.first().unwrap().range().start(),
        class_def.body.last().unwrap().range().end(),
    );
    format!(
        "{}{}{}",
        locator.slice(TextRange::new(stmt.range().start(), body_range.start())),
        inner_replacement,
        locator.slice(TextRange::new(body_range.end(), stmt.range().end()))
    )
}

/// Get the separator text following a statement in the suite.
///
/// ## Arguments
/// * `suite` - The suite of statements.
/// * `locator` - The locator for the source code.
/// * `stmt_idx` - The index of the statement in the suite.
///
/// ## Returns
/// A string representing the separator text following the statement.
fn separator_for(suite: &[Stmt], locator: &Locator, stmt_idx: usize) -> String {
    if let Some(next) = suite.get(stmt_idx + 1) {
        // Preserve the exact text between this statement and the next
        let range = TextRange::new(suite[stmt_idx].range().end(), next.range().start());
        locator.slice(range).to_string()
    } else if suite.len() > 1 {
        // Reuse the first separator as the trailing text after the last statement may include EOF
        // newlines or comments
        let range = TextRange::new(suite[0].range().end(), suite[1].range().start());
        locator.slice(range).to_string()
    } else {
        // Use a two-newline separator since we have a single statement suite
        "\n\n".to_string()
    }
}

/// Sort a suite of statements based on their reference order.
///
/// ## Arguments
/// * `suite` - The suite of statements to organize.
/// * `locator` - The locator for the source code.
/// * `narrative_order` - Whether to use a narrative-oriented order for the result.
///
/// ## Returns
/// A string representing the organized suite of statements.
fn organize_suite(suite: &[Stmt], locator: &Locator, narrative_order: bool) -> String {
    // Get the dependency order of the nodes
    let nodes = nodes_from_suite(suite);
    let dependencies = Dependencies::from_nodes(&nodes);
    let sorted = dependencies
        .dependency_order(&nodes, narrative_order)
        .unwrap_or_else(|_| (0..nodes.len()).collect());

    // Build a replacement string by reordering statements while preserving formatting
    let mut replacement = String::new();
    for (i, &node_idx) in sorted.iter().enumerate() {
        // Append the statement text
        let suite_text = get_replacement_text(locator, nodes[node_idx].stmt, narrative_order);
        replacement.push_str(&suite_text);

        // Append the separator unless this is the last sorted node
        if i < sorted.len() - 1 {
            replacement.push_str(&separator_for(suite, locator, node_idx));
        }
    }
    replacement
}

/// SS001
pub(crate) fn organize_statements(checker: &Checker, suite: &[Stmt]) {
    // Skip empty suites
    if suite.is_empty() {
        return;
    }

    // Build the replacement string recursively by sorting the statements in the suite
    let replacement = organize_suite(
        suite,
        checker.locator(),
        checker.settings().ssort.narrative_order,
    );

    // Only report a diagnostic if the replacement is different
    let range = TextRange::new(
        suite.first().unwrap().range().start(),
        suite.last().unwrap().range().end(),
    );
    let original_text = checker.locator().slice(range);
    if replacement != original_text {
        checker
            .report_diagnostic(UnsortedStatements, range)
            .set_fix(Fix::safe_edit(Edit::replacement(
                replacement,
                range.start(),
                range.end(),
            )));
    }
}

#[cfg(test)]
mod tests {
    use super::{get_replacement_text, separator_for};
    use crate::Locator;
    use anyhow::Result;
    use ruff_python_parser::parse_module;

    /// Test that `get_replacement_text()` returns the original text for a function.
    #[test]
    fn get_replacement_text_function() -> Result<()> {
        let source = r#"def foo():
    pass
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let stmt = &parsed.suite()[0];

        let replacement = get_replacement_text(&locator, stmt, false);

        assert_eq!(replacement, source.trim_end());
        Ok(())
    }

    /// Test that `get_replacement_text()` returns the original text for a non-sortable class.
    #[test]
    fn get_replacement_text_non_sortable_class() -> Result<()> {
        let source = r#"class Foo:
    x = 1
    y = 2
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let stmt = &parsed.suite()[0];

        let replacement = get_replacement_text(&locator, stmt, false);

        assert_eq!(replacement, source.trim_end());
        Ok(())
    }

    /// Test that `get_replacement_text()` sorts methods with dependencies.
    #[test]
    fn get_replacement_text_sortable_methods() -> Result<()> {
        let source = r#"class Foo:
    def method_b(self):
        return self.method_a()

    def method_a(self):
        pass
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let stmt = &parsed.suite()[0];

        let replacement = get_replacement_text(&locator, stmt, false);

        assert!(replacement.contains("def method_a(self):"));
        assert!(replacement.contains("def method_b(self):"));
        let method_a_pos = replacement.find("def method_a(self):").unwrap();
        let method_b_pos = replacement.find("def method_b(self):").unwrap();
        assert!(method_a_pos < method_b_pos);
        Ok(())
    }

    /// Test that `get_replacement_text()` preserves the class header.
    #[test]
    fn get_replacement_text_class_header() -> Result<()> {
        let source = r#"class Foo(Base):
    def method_b(self):
        return self.method_a()

    def method_a(self):
        pass
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let stmt = &parsed.suite()[0];

        let replacement = get_replacement_text(&locator, stmt, false);

        assert!(replacement.starts_with("class Foo(Base):"));
        Ok(())
    }

    /// Test that `get_replacement_text()` returns the original text for an assignment.
    #[test]
    fn get_replacement_text_assignment() -> Result<()> {
        let source = r#"x = 1
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let stmt = &parsed.suite()[0];

        let replacement = get_replacement_text(&locator, stmt, false);

        assert_eq!(replacement, source.trim_end());
        Ok(())
    }

    /// Test that `get_replacement_text()` returns the original text for circular dependencies.
    #[test]
    fn get_replacement_text_circular_dependency() -> Result<()> {
        let source = r#"class Foo:
    def a(self):
        return self.b()

    def b(self):
        return self.a()
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let stmt = &parsed.suite()[0];

        let replacement = get_replacement_text(&locator, stmt, false);

        assert_eq!(replacement, source.trim_end());
        Ok(())
    }

    /// Test that `get_replacement_text()` returns the original text for an empty class.
    #[test]
    fn get_replacement_text_empty_class() -> Result<()> {
        let source = r#"class Foo:
    pass
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let stmt = &parsed.suite()[0];

        let replacement = get_replacement_text(&locator, stmt, false);

        assert_eq!(replacement, source.trim_end());
        Ok(())
    }

    /// Test that `get_replacement_text()` handles complex a class with multiple methods.
    #[test]
    fn get_replacement_text_complex_class() -> Result<()> {
        let source = r#"class Foo:
    def method_c(self):
        return self.method_b()

    def method_b(self):
        return self.method_a()

    def method_a(self):
        return 1
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let stmt = &parsed.suite()[0];

        let replacement = get_replacement_text(&locator, stmt, false);

        assert!(replacement.contains("def method_a(self):"));
        assert!(replacement.contains("def method_b(self):"));
        assert!(replacement.contains("def method_c(self):"));
        let method_a_pos = replacement.find("def method_a(self):").unwrap();
        let method_b_pos = replacement.find("def method_b(self):").unwrap();
        let method_c_pos = replacement.find("def method_c(self):").unwrap();
        assert!(method_a_pos < method_b_pos);
        assert!(method_b_pos < method_c_pos);
        Ok(())
    }

    /// Test that `get_replacement_text()` preserves decorators on methods.
    #[test]
    fn get_replacement_text_with_decorators() -> Result<()> {
        let source = r#"class Foo:
    @decorator
    def method_b(self):
        return self.method_a()

    def method_a(self):
        return 1
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let stmt = &parsed.suite()[0];

        let replacement = get_replacement_text(&locator, stmt, false);

        assert!(replacement.contains("@decorator"));
        assert!(replacement.contains("def method_a(self):"));
        assert!(replacement.contains("def method_b(self):"));
        Ok(())
    }

    /// Test that `separator_for()` includes trailing inline comments.
    #[test]
    fn separator_for_inline_comment() -> Result<()> {
        let source = r#"def foo():
    pass  # inline comment

def bar():
    pass
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let suite = parsed.suite();

        let separator = separator_for(suite, &locator, 0);

        assert_eq!(separator, "  # inline comment\n\n");
        Ok(())
    }

    /// Test that `separator_for()` handles the last statement correctly.
    #[test]
    fn separator_for_last_statement() -> Result<()> {
        let source = r#"def foo():
    pass

def bar():
    pass
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let suite = parsed.suite();

        let separator = separator_for(suite, &locator, 1);

        assert_eq!(separator, "\n\n");
        Ok(())
    }

    /// Test that `separator_for()` returns a double newline for two statements.
    #[test]
    fn separator_for_two_statements() -> Result<()> {
        let source = r#"def foo():
    pass

def bar():
    pass
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let suite = parsed.suite();

        let separator = separator_for(suite, &locator, 0);
        assert_eq!(separator, "\n\n");

        Ok(())
    }

    /// Test that `separator_for()` handles multiple statements correctly.
    #[test]
    fn separator_for_multiple_statements() -> Result<()> {
        let source = r#"def foo():
    pass

def bar():
    pass

def baz():
    pass
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let suite = parsed.suite();

        let sep0 = separator_for(suite, &locator, 0);
        assert_eq!(sep0, "\n\n");

        let sep1 = separator_for(suite, &locator, 1);
        assert_eq!(sep1, "\n\n");

        Ok(())
    }

    /// Test that `separator_for()` returns the default separator for a single statement.
    #[test]
    fn separator_for_single_statement() -> Result<()> {
        let source = r#"def foo():
    pass
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let suite = parsed.suite();

        let separator = separator_for(suite, &locator, 0);

        assert_eq!(separator, "\n\n");
        Ok(())
    }

    /// Test that `separator_for()` preserves custom spacing between statements.
    #[test]
    fn separator_for_custom_spacing() -> Result<()> {
        let source = r#"def foo():
    pass



def bar():
    pass
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let suite = parsed.suite();

        let separator = separator_for(suite, &locator, 0);

        assert_eq!(separator, "\n\n\n\n");
        Ok(())
    }

    /// Test that `separator_for()` preserves comments between statements.
    #[test]
    fn separator_for_with_comment() -> Result<()> {
        let source = r#"def foo():
    pass
# Comment

def bar():
    pass
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let suite = parsed.suite();

        let separator = separator_for(suite, &locator, 0);

        assert_eq!(separator, "\n# Comment\n\n");
        Ok(())
    }

    /// Test that `separator_for()` handles multiple blank lines.
    #[test]
    fn separator_for_multiple_blank_lines() -> Result<()> {
        let source = r#"def foo():
    pass




def bar():
    pass
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let suite = parsed.suite();

        let separator = separator_for(suite, &locator, 0);

        assert_eq!(separator, "\n\n\n\n\n");
        Ok(())
    }

    /// Test that `separator_for()` handles multiple comments.
    #[test]
    fn separator_for_multiple_comments() -> Result<()> {
        let source = r#"def foo():
    pass
# Comment 1
# Comment 2

def bar():
    pass
"#;
        let locator = Locator::new(source);
        let parsed = parse_module(source)?;
        let suite = parsed.suite();

        let separator = separator_for(suite, &locator, 0);

        assert_eq!(separator, "\n# Comment 1\n# Comment 2\n\n");
        Ok(())
    }
}
