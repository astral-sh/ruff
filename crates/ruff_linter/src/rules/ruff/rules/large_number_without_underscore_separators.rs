use crate::AlwaysFixableViolation;
use crate::checkers::ast::Checker;
use crate::rules::ruff::settings::Settings;
use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for numeric literals that could be more readable with underscore separators
/// between groups of digits.
///
/// ## Why is this bad?
/// Large numeric literals can be difficult to read. Using underscore separators
/// improves readability by visually separating groups of digits.
///
/// ## Example
///
/// ```python
/// # Before
/// x = 1000000
/// y = 1234567.89
/// ```
///
/// Use instead:
/// ```python
/// # After
/// x = 1_000_000
/// y = 1_234_567.89
/// ```
///
/// ## References
/// - [PEP 515 - Underscores in Numeric Literals](https://peps.python.org/pep-0515/)
/// - [Number Localization Formatting Guide](https://randombits.dev/articles/number-localization/formatting)
#[derive(ViolationMetadata)]
pub(crate) struct LargeNumberWithoutUnderscoreSeparators;

impl AlwaysFixableViolation for LargeNumberWithoutUnderscoreSeparators {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Large numeric literal without underscore separators".to_string()
    }

    fn fix_title(&self) -> String {
        "Add underscore separators to numeric literal".to_string()
    }
}

/// RUF062: Large numeric literal without underscore separators
pub(crate) fn large_number_without_underscore_separators(checker: &Checker, expr: &ast::Expr) {
    let value_text = checker.locator().slice(expr.range());

    // format number to compare with the source
    let formatted_value: String =
        format_number_with_underscores(value_text, &checker.settings().ruff);

    if formatted_value != value_text {
        checker
            .report_diagnostic(LargeNumberWithoutUnderscoreSeparators, expr.range())
            .set_fix(Fix::safe_edit(Edit::range_replacement(
                formatted_value,
                expr.range(),
            )));
    }
}

/// Format a numeric literal with properly placed underscore separators
fn format_number_with_underscores(value: &str, settings: &Settings) -> String {
    // Remove existing underscores
    let value = value.replace("_", "");
    if value.starts_with("0x") || value.starts_with("0X") {
        // Hexadecimal
        let prefix = &value[..2];
        let hex_part = &value[2..];

        let formatted = format_digits(
            hex_part,
            settings.hex_digit_group_size,
            settings.hex_digit_group_size,
            settings.hex_digit_grouping_threshold,
        );
        format!("{}{}", prefix, formatted)
    } else if value.starts_with("0b") || value.starts_with("0B") {
        // Binary
        let prefix = &value[..2];
        let bin_part = &value[2..];

        let formatted = format_digits(
            bin_part,
            settings.bin_digit_group_size,
            settings.bin_digit_group_size,
            settings.bin_digit_grouping_threshold,
        );
        format!("{}{}", prefix, formatted)
    } else if value.starts_with("0o") || value.starts_with("0O") {
        // Octal
        let prefix = &value[..2];
        let oct_part = &value[2..];

        let formatted = format_digits(
            oct_part,
            settings.oct_digit_group_size,
            settings.oct_digit_group_size,
            settings.oct_digit_grouping_threshold,
        );
        format!("{}{}", prefix, formatted)
    } else {
        if value.contains(['e', 'E']) {
            // Handle scientific notation
            let parts: Vec<&str> = value.split(['e', 'E']).collect();
            let base = format_number_with_underscores(parts[0], settings);
            let exponent = parts[1];

            // Determine which separator was used (e or E)
            let separator = if value.contains('e') { 'e' } else { 'E' };

            return format!("{}{}{}", base, separator, exponent);
        }

        // Decimal (integer or float)
        let parts: Vec<&str> = value.split('.').collect();
        let group_size = if settings.use_indian_decimal_format {
            2
        } else {
            3
        };
        let integer_part = format_digits(
            &parts[0],
            group_size,
            3,
            settings.dec_digit_grouping_threshold,
        );

        if parts.len() > 1 {
            // It's a float, handle the fractional part
            let float_part = format_float(
                parts[1],
                group_size,
                3,
                settings.dec_digit_grouping_threshold,
            );
            format!("{}.{}", integer_part, float_part)
        } else {
            // It's an integer
            format!("{}", integer_part)
        }
    }
}

/// Helper function to format digits with underscores at specified intervals
fn format_digits(
    digits: &str,
    group_size: usize,
    first_group_size: usize,
    threshold: usize,
) -> String {
    if digits.len() < threshold || group_size == 0 || first_group_size == 0 {
        return digits.to_string();
    }

    let mut result = String::with_capacity(digits.len() * 2);
    let mut count = 0;

    // Process digits from right to left
    for c in digits.chars().rev() {
        if count == first_group_size
            || (count > first_group_size + 1 && (count - first_group_size) % group_size == 0)
        {
            result.push('_');
        }
        result.push(c);
        count += 1;
    }

    // Reverse the result to get the correct order
    result.chars().rev().collect()
}

// Helper function to format float parts with underscores at specified intervals
fn format_float(
    digits: &str,
    group_size: usize,
    first_group_size: usize,
    threshold: usize,
) -> String {
    if digits.len() < threshold || group_size == 0 || first_group_size == 0 {
        return digits.to_string();
    }

    let mut result = String::with_capacity(digits.len() * 2);
    let mut count = 0;

    // Process digits from right to left
    for c in digits.chars() {
        if count == first_group_size
            || (count > first_group_size && (count - first_group_size) % group_size == 0)
        {
            result.push('_');
        }
        result.push(c);
        count += 1;
    }

    result
}
