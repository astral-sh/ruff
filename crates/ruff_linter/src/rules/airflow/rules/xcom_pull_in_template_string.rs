use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_semantic::Modules;
use ruff_python_trivia::Cursor;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::rules::airflow::helpers::is_airflow_builtin_or_provider;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for Airflow operator keyword arguments that use a Jinja template
/// string containing an `xcom_pull` call to retrieve another task's output,
/// including both pure-template strings (where the entire value is an
/// `xcom_pull` expression) and mixed-content strings (where `xcom_pull`
/// appears alongside other text). Template values built with f-strings are
/// not processed by this rule, since they are harder to analyze statically.
///
/// ## Why is this bad?
/// Using `{{ ti.xcom_pull(task_ids='some_task') }}` as a string template to
/// access the output of an upstream task is less readable and more
/// error-prone than using the `.output` attribute on the task object
/// directly. The `.output` attribute provides better IDE support and makes
/// task dependencies more explicit.
///
/// ## Example
/// ```python
/// from airflow.operators.python import PythonOperator
///
///
/// task_1 = PythonOperator(task_id="task_1", python_callable=my_func)
/// task_2 = PythonOperator(
///     task_id="task_2",
///     op_args="{{ ti.xcom_pull(task_ids='task_1') }}",
/// )
/// task_3 = PythonOperator(
///     task_id="task_3",
///     op_args="echo {{ ti.xcom_pull(task_ids='task_1') }}",
/// )
/// ```
///
/// Use instead:
/// ```python
/// from airflow.operators.python import PythonOperator
///
///
/// task_1 = PythonOperator(task_id="task_1", python_callable=my_func)
/// task_2 = PythonOperator(
///     task_id="task_2",
///     op_args=task_1.output,
/// )
/// task_3 = PythonOperator(
///     task_id="task_3",
///     op_args="echo {{ task_1.output }}",
/// )
/// ```
///
/// ## Fix safety
/// The fix is always unsafe because the variable in scope that matches the
/// task ID may not be the Airflow task object that produced the `XCom` value.
/// For pure-template strings the entire argument is replaced with a Python
/// expression; for mixed-content strings only the `xcom_pull` call within
/// the Jinja block is replaced in-place.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.11")]
pub(crate) struct AirflowXcomPullInTemplateString {
    task_id: String,
    fix_title_style: FixTitleStyle,
}

impl Violation for AirflowXcomPullInTemplateString {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let AirflowXcomPullInTemplateString { task_id, .. } = self;
        format!(
            "Use the `.output` attribute on the task object for \"{task_id}\" instead of `xcom_pull` in a template string"
        )
    }

    fn fix_title(&self) -> Option<String> {
        let AirflowXcomPullInTemplateString {
            task_id,
            fix_title_style,
        } = self;
        match fix_title_style {
            FixTitleStyle::MixedContent => {
                Some(format!("Replace with `{{{{ {task_id}.output }}}}`"))
            }
            FixTitleStyle::PureTemplate => Some(format!("Replace with `{task_id}.output`")),
        }
    }
}

/// Controls the wording of the fix title emitted by the AIR201 violation.
///
/// Pure-template fixes replace the entire Python argument with a bare
/// expression (`task_id.output`). Mixed-content fixes replace only the
/// `xcom_pull` call within the Jinja block, so the title shows the Jinja
/// form (`{{ task_id.output }}`).
#[derive(Debug, PartialEq, Eq, Hash)]
enum FixTitleStyle {
    PureTemplate,
    MixedContent,
}

/// Within-source byte range and task ID for a single `xcom_pull` occurrence
/// found by the mixed-content scanner. Offsets are relative to the start of
/// the raw source slice passed to `scan_xcom_pull_patterns`; the caller is
/// responsible for translating them to absolute file positions by adding the
/// literal's `TextSize` start.
///
/// `u32` matches the `TextSize` representation used throughout Ruff and
/// encodes the 4 GiB file-size invariant at the type level rather than
/// deferring to a late `u32::try_from` conversion.
struct XcomPullMatch {
    /// Byte offset within the raw source slice where `ti` or `task_instance` starts.
    source_start: u32,
    /// Byte offset within the raw source slice just past the closing `)`.
    source_end: u32,
    task_id: String,
}

impl XcomPullMatch {
    /// Construct a match, asserting the range invariant in debug builds.
    fn new(source_start: u32, source_end: u32, task_id: String) -> Self {
        debug_assert!(
            source_start <= source_end,
            "source_start ({source_start}) must not exceed source_end ({source_end})"
        );
        Self {
            source_start,
            source_end,
            task_id,
        }
    }

    /// Translate the within-source offsets to an absolute `TextRange` in the
    /// file by adding the string literal's start position.
    fn absolute_range(&self, literal_start: TextSize) -> TextRange {
        TextRange::new(
            literal_start + TextSize::new(self.source_start),
            literal_start + TextSize::new(self.source_end),
        )
    }
}

/// AIR201
pub(crate) fn xcom_pull_in_template_string(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    // Check if this is a call to an Airflow operator or sensor.
    if !checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| {
            is_airflow_builtin_or_provider(qualified_name.segments(), "operators", "Operator")
                || is_airflow_builtin_or_provider(qualified_name.segments(), "sensors", "Sensor")
        })
    {
        return;
    }

    // Check all arguments for xcom_pull template strings. Any operator
    // argument can be a Jinja template field (determined by the operator's
    // `template_fields` attribute), so we check both positional and keyword
    // arguments.
    for arg_value in call
        .arguments
        .iter_source_order()
        .map(ruff_python_ast::ArgOrKeyword::value)
    {
        let Some(string_literal) = arg_value.as_string_literal_expr() else {
            continue;
        };

        // `to_str()` decodes escape sequences; the pure-template parser needs
        // the logical character sequence. The mixed-content scanner uses
        // `locator().slice()` (raw source bytes) instead, so that its byte
        // offsets align with the physical file positions used for fixes.
        let string_value = string_literal.value.to_str();

        // Pure-template path: the entire string value is a single xcom_pull
        // Jinja expression. The fix replaces the whole argument with a Python
        // attribute access expression.
        if let Some(task_id) = parse_xcom_pull_template(string_value) {
            let mut diagnostic = checker.report_diagnostic(
                AirflowXcomPullInTemplateString {
                    task_id: task_id.clone(),
                    fix_title_style: FixTitleStyle::PureTemplate,
                },
                arg_value.range(),
            );

            // If the task_id matches a variable in scope, provide an unsafe fix
            // replacing the template string with `<variable>.output`.
            if checker.semantic().lookup_symbol(&task_id).is_some() {
                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                    format!("{task_id}.output"),
                    arg_value.range(),
                )));
            }
            continue;
        }

        // Mixed-content path: xcom_pull appears alongside other content within
        // the string. Each occurrence gets its own diagnostic; the fix replaces
        // only the `ti.xcom_pull(...)` sub-range in-place, preserving the
        // surrounding text (including Jinja delimiters and trailing subscripts).
        //
        // Skip implicitly concatenated strings: the scanner computes fix ranges
        // as byte offsets into the raw source slice of the full expression range,
        // which works correctly only when all parts form a single contiguous span
        // with no intermediate quote boundaries separating them.
        if string_literal.value.is_implicit_concatenated() {
            continue;
        }
        let raw_source = checker.locator().slice(string_literal.range());
        let literal_start = string_literal.start();
        for scan_match in scan_xcom_pull_patterns(raw_source) {
            let mut diagnostic = checker.report_diagnostic(
                AirflowXcomPullInTemplateString {
                    task_id: scan_match.task_id.clone(),
                    fix_title_style: FixTitleStyle::MixedContent,
                },
                arg_value.range(),
            );

            if checker
                .semantic()
                .lookup_symbol(&scan_match.task_id)
                .is_some()
            {
                // Translate within-source byte offsets to absolute file positions.
                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                    format!("{}.output", scan_match.task_id),
                    scan_match.absolute_range(literal_start),
                )));
            }
        }
    }
}

/// Return all `(ti|task_instance).xcom_pull(...)` matches found in `source`.
///
/// `source` is the raw file content of a string literal (including its
/// surrounding quote characters). The returned offsets are byte positions
/// within `source`, covering exactly `ti.xcom_pull(args)` — not the
/// surrounding `{{ }}` Jinja delimiters. The caller translates them to
/// absolute file positions by adding the literal's start offset.
///
/// The scanner is quote-agnostic: the `ti.xcom_pull(` pattern cannot appear
/// in the quote characters themselves, so no quote-style handling is needed.
fn scan_xcom_pull_patterns(source: &str) -> Vec<XcomPullMatch> {
    let mut matches = Vec::new();
    let mut pos = 0;

    while pos < source.len() {
        let remaining = &source[pos..];
        let mut cursor = Cursor::new(remaining);

        // Try to parse an identifier at the current position. If there is
        // none (e.g. we are on punctuation, whitespace, or a non-ASCII
        // Unicode character), advance past the full codepoint so we never
        // split a multi-byte UTF-8 sequence and trigger a string-slice panic.
        let Some(receiver) = parse_identifier(&mut cursor) else {
            let ch_len = remaining.chars().next().map_or(1, char::len_utf8);
            pos += ch_len;
            continue;
        };

        if !is_xcom_pull_receiver(receiver) {
            // Skip the entire identifier so we don't re-examine its suffix
            // (e.g. `multi_ti` must not match as `ti` at offset 6).
            pos += receiver.len();
            continue;
        }

        // Attempt to parse `.xcom_pull(args)`. If this fails, the cursor may
        // have consumed characters beyond the receiver (e.g. whitespace and a
        // different method name), so advance by the full amount consumed rather
        // than just the receiver length to avoid redundant re-scanning.
        let Some(task_id_str) = parse_xcom_pull_method_and_args(&mut cursor) else {
            let consumed = remaining.len() - cursor.as_str().len();
            pos += consumed.max(receiver.len());
            continue;
        };

        // `parse_xcom_pull_args` guarantees the task ID is a non-empty valid
        // Python identifier, so we can push the match directly.
        // `consumed` covers exactly `receiver.method(args)` — the cursor is
        // positioned past the closing `)` and no further.
        //
        // Convert to u32 at the point of collection; silently skip the match
        // on overflow (source files are expected to be well under 4 GiB).
        let consumed = remaining.len() - cursor.as_str().len();
        if let (Ok(start_u32), Ok(end_u32)) = (u32::try_from(pos), u32::try_from(pos + consumed)) {
            matches.push(XcomPullMatch::new(
                start_u32,
                end_u32,
                task_id_str.to_string(),
            ));
        }
        // Advance past the entire match to avoid re-examining it.
        pos += consumed;
    }

    matches
}

/// Returns `true` if `s` is a valid Python identifier.
///
/// Uses the same `parse_identifier` cursor helper as the template parser to
/// ensure consistent behavior: `s` must start with an ASCII letter or
/// underscore, consist only of ASCII alphanumerics and underscores, and be
/// non-empty.
///
/// # TODO(airflow)
/// Task IDs that fail this check (e.g. `kebab-case` or `group.task`) are
/// silently skipped. A future AIR rule could enforce that task IDs are valid
/// Python identifiers. Dotted IDs like `group.task` represent task group
/// members; once Airflow adds support for the `group['task'].output` syntax
/// they could be replaced automatically.
fn is_valid_python_identifier(s: &str) -> bool {
    let mut cursor = Cursor::new(s);
    parse_identifier(&mut cursor).is_some() && cursor.is_eof()
}

/// Returns `true` if `name` is a recognised `xcom_pull` receiver (`ti` or
/// `task_instance`). Used consistently by both detection paths so that adding
/// a new alias requires a single change.
fn is_xcom_pull_receiver(name: &str) -> bool {
    name == "ti" || name == "task_instance"
}

/// Advance the cursor past `.xcom_pull(args)`, where the receiver has already
/// been parsed and validated. Returns the task ID on success, or `None` if the
/// method name is not `xcom_pull`, the opening `(` is absent, or the argument
/// list is invalid.
///
/// Shared by both the pure-template parser and the mixed-content scanner to
/// avoid duplicating the method + argument parsing sequence.
fn parse_xcom_pull_method_and_args<'a>(cursor: &mut Cursor<'a>) -> Option<&'a str> {
    parse_xcom_pull_method(cursor)?;
    parse_xcom_pull_args(cursor)
}

/// Advance the cursor past `.xcom_pull(`, consuming optional whitespace around
/// the dot and before the opening parenthesis. Returns `None` — without
/// advancing the cursor past the receiver — if the sequence is not present or
/// the method name is not `xcom_pull`.
///
/// Called after the receiver identifier has already been parsed and validated.
fn parse_xcom_pull_method(cursor: &mut Cursor) -> Option<()> {
    eat_whitespace(cursor);
    if !cursor.eat_char('.') {
        return None;
    }
    eat_whitespace(cursor);
    if parse_identifier(cursor)? != "xcom_pull" {
        return None;
    }
    eat_whitespace(cursor);
    if !cursor.eat_char('(') {
        return None;
    }
    Some(())
}

/// Parse the argument list of an `xcom_pull(...)` call, with the cursor
/// positioned immediately after the opening `(`. On success the cursor is
/// positioned after the closing `)`.
///
/// Accepts the same argument forms used by both the pure-template and
/// mixed-content detection paths:
/// - positional: `'task_id'`, `['task_id']`, `('task_id',)`
/// - keyword: `task_ids='task_id'`, `task_id='task_id'`
/// - optional `key='return_value'` in either order
///
/// Returns a non-empty reference into the source for the task ID string, or
/// `None` if the argument list is invalid, contains unsupported forms, or
/// yields an empty task ID.
fn parse_xcom_pull_args<'a>(cursor: &mut Cursor<'a>) -> Option<&'a str> {
    let mut task_id: Option<&'a str> = None;
    let mut saw_keyword = false;
    let mut saw_key = false;

    loop {
        eat_whitespace(cursor);
        if cursor.eat_char(')') {
            break;
        }

        if let Some(identifier) = parse_identifier(cursor) {
            // Keyword argument: must be followed by `=`.
            eat_whitespace(cursor);
            if !cursor.eat_char('=') {
                return None;
            }
            saw_keyword = true;
            eat_whitespace(cursor);

            match identifier {
                "task_ids" | "task_id" => {
                    if task_id.is_some() {
                        return None;
                    }
                    task_id = Some(parse_task_id_value(cursor)?);
                }
                "key" => {
                    if saw_key || parse_quoted_string(cursor)? != "return_value" {
                        return None;
                    }
                    saw_key = true;
                }
                _ => return None,
            }
        } else {
            // Positional arguments must come before any keyword arguments.
            if saw_keyword || task_id.is_some() {
                return None;
            }
            task_id = Some(parse_task_id_value(cursor)?);
        }

        eat_whitespace(cursor);
        if cursor.eat_char(',') {
            continue;
        }
        if cursor.eat_char(')') {
            break;
        }
        return None;
    }

    // Enforce postconditions here so every call site receives a guaranteed
    // non-empty, valid-Python-identifier task ID without additional checks.
    let id = task_id?;
    if id.is_empty() || !is_valid_python_identifier(id) {
        None
    } else {
        Some(id)
    }
}

/// Parse a Jinja template string to extract a single `xcom_pull` task ID.
///
/// Returns the task ID if the entire string is a single `{{ ti.xcom_pull(task_ids='...') }}`
/// or `{{ task_instance.xcom_pull(task_ids='...') }}` template. The `task_ids` value may also
/// be wrapped in a list or tuple (e.g. `task_ids=['...']`), and an optional
/// `key='return_value'` argument is allowed in either order (since it is the
/// default). Returns `None` if the string contains other content, multiple
/// task IDs, non-default keyword arguments, or a task ID that is not a valid
/// Python identifier.
fn parse_xcom_pull_template(s: &str) -> Option<String> {
    let mut cursor = Cursor::new(s);
    eat_whitespace(&mut cursor);

    if !cursor.eat_char2('{', '{') {
        return None;
    }
    eat_whitespace(&mut cursor);

    let receiver = parse_identifier(&mut cursor)?;
    if !is_xcom_pull_receiver(receiver) {
        return None;
    }
    let task_id = parse_xcom_pull_method_and_args(&mut cursor)?;

    eat_whitespace(&mut cursor);
    if !cursor.eat_char2('}', '}') {
        return None;
    }
    eat_whitespace(&mut cursor);

    if !cursor.is_eof() {
        return None;
    }

    // `parse_xcom_pull_args` already validates that the task ID is a
    // non-empty Python identifier, so no additional check is needed here.
    Some(task_id.to_string())
}

fn eat_whitespace(cursor: &mut Cursor) {
    cursor.eat_while(char::is_whitespace);
}

fn parse_identifier<'a>(cursor: &mut Cursor<'a>) -> Option<&'a str> {
    let source = cursor.as_str();
    if !cursor.eat_if(|c| c.is_ascii_alphabetic() || c == '_') {
        return None;
    }
    cursor.eat_while(|c| c.is_ascii_alphanumeric() || c == '_');
    let len = source.len() - cursor.as_str().len();
    Some(&source[..len])
}

/// Parse a quoted string delimited by single or double quotes.
///
/// Returns `None` if the string contains escape sequences (e.g. `\'`),
/// since Jinja supports them and correctly handling escaped content is
/// out of scope for this rule.
fn parse_quoted_string<'a>(cursor: &mut Cursor<'a>) -> Option<&'a str> {
    let quote_char = if cursor.eat_char('\'') {
        '\''
    } else if cursor.eat_char('"') {
        '"'
    } else {
        return None;
    };

    let remaining = cursor.as_str();
    let end = remaining.find(quote_char)?;
    let value = &remaining[..end];

    // Bail if the string contains escape sequences.
    if value.contains('\\') {
        return None;
    }

    cursor.skip_bytes(end + quote_char.len_utf8());
    Some(value)
}

/// Parse a task ID value, which may be a plain quoted string or wrapped in
/// a single-element list or tuple: `'task'`, `['task']`, `('task',)`.
fn parse_task_id_value<'a>(cursor: &mut Cursor<'a>) -> Option<&'a str> {
    let closing_bracket = if cursor.eat_char('[') {
        eat_whitespace(cursor);
        Some(']')
    } else if cursor.eat_char('(') {
        eat_whitespace(cursor);
        Some(')')
    } else {
        None
    };

    let task_id = parse_quoted_string(cursor)?;
    eat_whitespace(cursor);

    if let Some(bracket) = closing_bracket {
        // Allow optional trailing comma for tuples: `('task',)`.
        if cursor.eat_char(',') {
            eat_whitespace(cursor);
        }
        if !cursor.eat_char(bracket) {
            return None;
        }
        eat_whitespace(cursor);
    }

    Some(task_id)
}

#[cfg(test)]
mod tests {
    use super::{is_valid_python_identifier, parse_xcom_pull_template, scan_xcom_pull_patterns};

    // --- is_valid_python_identifier ---

    #[test]
    fn test_valid_identifier_simple() {
        assert!(is_valid_python_identifier("task_1"));
        assert!(is_valid_python_identifier("my_task"));
        assert!(is_valid_python_identifier("_private"));
        assert!(is_valid_python_identifier("task"));
        assert!(is_valid_python_identifier("_"));
    }

    #[test]
    fn test_rejects_kebab_case() {
        assert!(!is_valid_python_identifier("kebab-case"));
        assert!(!is_valid_python_identifier("my-task"));
    }

    #[test]
    fn test_rejects_dotted_id() {
        assert!(!is_valid_python_identifier("group.task"));
        assert!(!is_valid_python_identifier("group_1.task_1"));
    }

    #[test]
    fn test_rejects_digit_only_start() {
        assert!(!is_valid_python_identifier("1task"));
        assert!(!is_valid_python_identifier("123"));
    }

    #[test]
    fn test_rejects_empty() {
        assert!(!is_valid_python_identifier(""));
    }

    // --- scan_xcom_pull_patterns ---

    #[test]
    fn test_scan_mixed_content_positional() {
        let raw = r#""echo {{ ti.xcom_pull('task_1') }}""#;
        let matches = scan_xcom_pull_patterns(raw);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].task_id, "task_1");
    }

    #[test]
    fn test_scan_mixed_content_keyword() {
        let raw = r#""result: {{ ti.xcom_pull(task_ids='task_1') }}""#;
        let matches = scan_xcom_pull_patterns(raw);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].task_id, "task_1");
    }

    #[test]
    fn test_scan_multiple_occurrences() {
        let raw = r#""{{ ti.xcom_pull('task_1') }} and {{ ti.xcom_pull('task_2') }}""#;
        let matches = scan_xcom_pull_patterns(raw);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].task_id, "task_1");
        assert_eq!(matches[1].task_id, "task_2");
    }

    #[test]
    fn test_scan_skips_kebab_case() {
        let raw = r#""echo {{ ti.xcom_pull('kebab-task') }}""#;
        let matches = scan_xcom_pull_patterns(raw);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_scan_skips_dotted_id() {
        let raw = r#""echo {{ ti.xcom_pull('group.task') }}""#;
        let matches = scan_xcom_pull_patterns(raw);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_scan_no_false_positive_on_longer_receiver() {
        // `multi_ti` must not match as `ti`.
        let raw = r#""{{ multi_ti.xcom_pull('task_1') }}""#;
        let matches = scan_xcom_pull_patterns(raw);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_scan_offsets_cover_only_call() {
        // The match range must cover exactly `ti.xcom_pull('task_1')`,
        // not the surrounding `{{ }}` or Python quotes.
        let raw = r#""{{ ti.xcom_pull('task_1') }}""#;
        let matches = scan_xcom_pull_patterns(raw);
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        let start = m.source_start as usize;
        let end = m.source_end as usize;
        assert_eq!(&raw[start..end], "ti.xcom_pull('task_1')");
    }

    // --- parse_xcom_pull_template (existing + new identifier-gate tests) ---

    #[test]
    fn test_rejects_kebab_case_in_pure_template() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids='kebab-case') }}"),
            None
        );
    }

    #[test]
    fn test_rejects_dotted_id_in_pure_template() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids='group.task') }}"),
            None
        );
    }

    #[test]
    fn test_basic_ti_xcom_pull() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids='my_task') }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_task_instance_xcom_pull() {
        assert_eq!(
            parse_xcom_pull_template("{{ task_instance.xcom_pull(task_ids='my_task') }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_positional_argument() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull('my_task') }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_double_quotes() {
        assert_eq!(
            parse_xcom_pull_template(r#"{{ ti.xcom_pull(task_ids="my_task") }}"#),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_no_spaces_in_braces() {
        assert_eq!(
            parse_xcom_pull_template("{{ti.xcom_pull(task_ids='my_task')}}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_extra_whitespace() {
        assert_eq!(
            parse_xcom_pull_template("{{  ti.xcom_pull( task_ids = 'my_task' )  }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_whitespace_around_dot() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti . xcom_pull(task_ids='my_task') }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_whitespace_before_open_paren() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull (task_ids='my_task') }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_task_id_singular_keyword() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_id='my_task') }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_rejects_mixed_content() {
        assert_eq!(
            parse_xcom_pull_template("echo {{ ti.xcom_pull(task_ids='my_task') }}"),
            None
        );
    }

    #[test]
    fn test_rejects_additional_arguments() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids='my_task', key='my_key') }}"),
            None
        );
    }

    #[test]
    fn test_rejects_empty_task_id() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids='') }}"),
            None
        );
    }

    #[test]
    fn test_rejects_no_template() {
        assert_eq!(parse_xcom_pull_template("just a string"), None);
    }

    #[test]
    fn test_rejects_multi_element_list() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids=['a', 'b']) }}"),
            None
        );
    }

    #[test]
    fn test_single_element_list() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids=['my_task']) }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_single_element_tuple() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids=('my_task',)) }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_single_element_tuple_no_trailing_comma() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids=('my_task')) }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_key_return_value() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids='my_task', key='return_value') }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_key_return_value_double_quotes() {
        assert_eq!(
            parse_xcom_pull_template(
                r#"{{ ti.xcom_pull(task_ids='my_task', key="return_value") }}"#
            ),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_list_with_key_return_value() {
        assert_eq!(
            parse_xcom_pull_template(
                "{{ ti.xcom_pull(task_ids=['my_task'], key='return_value') }}"
            ),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_rejects_non_default_key() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids='my_task', key='custom_key') }}"),
            None
        );
    }

    #[test]
    fn test_rejects_list_with_non_default_key() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids=['my_task'], key='custom_key') }}"),
            None
        );
    }

    #[test]
    fn test_rejects_escaped_quotes() {
        assert_eq!(
            parse_xcom_pull_template(r"{{ ti.xcom_pull(task_ids='my\_task') }}"),
            None
        );
    }

    #[test]
    fn test_rejects_unknown_keyword() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(dag_id='my_task') }}"),
            None
        );
    }

    #[test]
    fn test_key_return_value_before_task_ids() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(key='return_value', task_ids='my_task') }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_key_return_value_before_task_ids_list() {
        assert_eq!(
            parse_xcom_pull_template(
                "{{ ti.xcom_pull(key='return_value', task_ids=['my_task']) }}"
            ),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_rejects_non_default_key_before_task_ids() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(key='custom_key', task_ids='my_task') }}"),
            None
        );
    }
}
