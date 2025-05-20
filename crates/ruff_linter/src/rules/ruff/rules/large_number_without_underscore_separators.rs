use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

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

const HEX_THRESHOLD: usize = 5;
const BIN_THRESHOLD: usize = 9;
const OCT_THRESHOLD: usize = 5;
const DEC_THRESHOLD: usize = 5;

const HEX_GROUPSIZE: usize = 4;
const BIN_GROUPSIZE: usize = 8;
const OCT_GROUPSIZE: usize = 4;
const DEC_GROUPSIZE: usize = 3;

/// RUF061: Large numeric literal without underscore separators
pub(crate) fn large_number_without_underscore_separators(checker: &Checker, expr: &ast::Expr) {
    let value_text = checker.locator().slice(expr.range());
    
    // format number to compare with the source
    let formatted_value: String = format_number_with_underscores(value_text);

    if formatted_value != value_text {
        let diagnostic = Diagnostic::new(
            LargeNumberWithoutUnderscoreSeparators, 
            expr.range()
        ).with_fix(
            Fix::safe_edit(Edit::range_replacement(formatted_value, expr.range()))
        );
        checker.report_diagnostic(diagnostic);
    }
}

/// Format a numeric literal with properly placed underscore separators
fn format_number_with_underscores(value: &str) -> String {
    // Remove existing underscores
    let value = value.replace("_", "");
    if value.starts_with("0x") || value.starts_with("0X") {
        // Hexadecimal
        let prefix = &value[..2];
        let hex_part = &value[2..];

        if hex_part.len() < HEX_THRESHOLD {
            format!("{}{}", prefix, hex_part)
        } else {
            let formatted = format_digits(hex_part, HEX_GROUPSIZE);
            format!("{}{}", prefix, formatted)
        }        
    } else if value.starts_with("0b") || value.starts_with("0B") {
        // Binary
        let prefix = &value[..2];
        let bin_part = &value[2..];
        
        if bin_part.len() < BIN_THRESHOLD {
            format!("{}{}", prefix, bin_part)
        } else {
            let formatted = format_digits(bin_part, BIN_GROUPSIZE);
            format!("{}{}", prefix, formatted)
        }
    } else if value.starts_with("0o") || value.starts_with("0O") {
        // Octal
        let prefix = &value[..2];
        let oct_part = &value[2..];
        
        if oct_part.len() < OCT_THRESHOLD {
            format!("{}{}", prefix, oct_part)
        } else {
            let formatted = format_digits(oct_part, OCT_GROUPSIZE);
            format!("{}{}", prefix, formatted)
        }
    } else {
        if value.contains(['e', 'E']) {
            // Handle scientific notation
            let parts: Vec<&str> = value.split(['e', 'E']).collect();
            let base = format_number_with_underscores(parts[0]);
            let exponent = parts[1];
            
            // Determine which separator was used (e or E)
            let separator = if value.contains('e') { 'e' } else { 'E' };
            
            return format!("{}{}{}", base, separator, exponent);
        }  

        // Decimal (integer or float)
        let parts: Vec<&str> = value.split('.').collect();
        let integer_part = parts[0];
        
        if integer_part.len() < DEC_THRESHOLD {
            if parts.len() > 1 {
                return format!("{}.{}", integer_part, parts[1]);
            } else {
                return format!("{}", integer_part);
            }
        }
        // Format integer part with underscores every 3 digits from the right
        let formatted_integer = format_digits(integer_part, DEC_GROUPSIZE);
        
        if parts.len() > 1 {
            // It's a float, handle the fractional part
            format!("{}.{}", formatted_integer, parts[1])
        } else {
            // It's an integer
            format!("{}", formatted_integer)
        }
    }
}

/// Helper function to format digits with underscores at specified intervals
fn format_digits(digits: &str, group_size: usize) -> String {
    let mut result = String::new();
    let mut count = 0;
    
    // Process digits from right to left
    for c in digits.chars().rev() {
        if count > 0 && count % group_size == 0 {
            result.push('_');
        }
        result.push(c);
        count += 1;
    }
    
    // Reverse the result to get the correct order
    result.chars().rev().collect()
}
