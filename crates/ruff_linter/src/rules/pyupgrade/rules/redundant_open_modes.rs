use std::fmt::Write;
use std::str::FromStr;

use anyhow::Result;
use bitflags::bitflags;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_codegen::Stylist;
use ruff_python_parser::{TokenKind, Tokens};
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for redundant `open` mode parameters.
///
/// ## Why is this bad?
/// Redundant `open` mode parameters are unnecessary and should be removed to
/// avoid confusion.
///
/// ## Example
/// ```python
/// with open("foo.txt", "r") as f:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// with open("foo.txt") as f:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `open`](https://docs.python.org/3/library/functions.html#open)
#[violation]
pub struct RedundantOpenModes {
    replacement: String,
}

impl AlwaysFixableViolation for RedundantOpenModes {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedundantOpenModes { replacement } = self;
        if replacement.is_empty() {
            "Unnecessary open mode parameters".to_string()
        } else {
            format!("Unnecessary open mode parameters, use \"{replacement}\"")
        }
    }

    fn fix_title(&self) -> String {
        let RedundantOpenModes { replacement } = self;
        if replacement.is_empty() {
            "Remove open mode parameters".to_string()
        } else {
            format!("Replace with \"{replacement}\"")
        }
    }
}

/// UP015
pub(crate) fn redundant_open_modes(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["" | "builtins" | "aiofiles", "open"]
            )
        })
    {
        return;
    }

    match call.arguments.find_argument("mode", 1) {
        None => {
            if !call.arguments.is_empty() {
                if let Some(keyword) = call.arguments.find_keyword("mode") {
                    if let Expr::StringLiteral(ast::ExprStringLiteral {
                        value: mode_param_value,
                        ..
                    }) = &keyword.value
                    {
                        if let Ok(mode) = OpenMode::from_str(mode_param_value.to_str()) {
                            if mode.redundant() {
                                checker.diagnostics.push(create_diagnostic(
                                    call,
                                    &keyword.value,
                                    &mode.to_string(),
                                    checker.tokens(),
                                    checker.stylist(),
                                ));
                            }
                        }
                    }
                }
            }
        }
        Some(mode_param) => {
            if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = &mode_param {
                if let Ok(mode) = OpenMode::from_str(value.to_str()) {
                    if mode.redundant() {
                        checker.diagnostics.push(create_diagnostic(
                            call,
                            mode_param,
                            &mode.to_string(),
                            checker.tokens(),
                            checker.stylist(),
                        ));
                    }
                }
            }
        }
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub(super) struct OpenMode: u8 {
        /// `r`
        const READ = 0b0001;
        /// `w`
        const WRITE = 0b0010;
        /// `a`
        const APPEND = 0b0100;
        /// `x`
        const CREATE = 0b1000;
        /// `b`
        const BINARY = 0b10000;
        /// `t`
        const TEXT = 0b10_0000;
        /// `+`
        const PLUS = 0b100_0000;
        /// `U`
        const UNIVERSAL_NEWLINES = 0b1000_0000;
    }
}

impl TryFrom<char> for OpenMode {
    type Error = ();

    fn try_from(value: char) -> std::result::Result<Self, Self::Error> {
        match value {
            'r' => Ok(Self::READ),
            'w' => Ok(Self::WRITE),
            'a' => Ok(Self::APPEND),
            'x' => Ok(Self::CREATE),
            'b' => Ok(Self::BINARY),
            't' => Ok(Self::TEXT),
            '+' => Ok(Self::PLUS),
            'U' => Ok(Self::UNIVERSAL_NEWLINES),
            _ => Err(()),
        }
    }
}

impl FromStr for OpenMode {
    type Err = ();

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        let mut open_mode = OpenMode::empty();
        for char in string.chars() {
            open_mode |= OpenMode::try_from(char)?;
        }
        Ok(open_mode)
    }
}

impl OpenMode {
    fn redundant(self) -> bool {
        // `t` is always redundant.
        if self.contains(Self::TEXT) {
            return true;
        }

        // `U` is always redundant.
        if self.contains(Self::UNIVERSAL_NEWLINES) {
            return true;
        }

        // `r` is redundant, unless `b` or `+` is also set.
        if self.contains(Self::READ) && !self.intersects(Self::BINARY | Self::PLUS) {
            return true;
        }

        false
    }
}

/// Write the [`OpenMode`] as a canonical string (i.e., ignoring redundant flags).
impl std::fmt::Display for OpenMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.contains(Self::WRITE) {
            f.write_char('w')?;
        } else if self.contains(Self::APPEND) {
            f.write_char('a')?;
        } else if self.contains(Self::CREATE) {
            f.write_char('x')?;
        } else if self.intersects(Self::BINARY | Self::PLUS) {
            f.write_char('r')?;
        }
        if self.contains(Self::BINARY) {
            f.write_char('b')?;
        }
        if self.contains(Self::PLUS) {
            f.write_char('+')?;
        }
        Ok(())
    }
}

fn create_diagnostic(
    call: &ast::ExprCall,
    mode_param: &Expr,
    replacement: &str,
    tokens: &Tokens,
    stylist: &Stylist,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        RedundantOpenModes {
            replacement: replacement.to_string(),
        },
        call.range(),
    );

    if replacement.is_empty() {
        diagnostic
            .try_set_fix(|| create_remove_param_fix(call, mode_param, tokens).map(Fix::safe_edit));
    } else {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            format!("{}{replacement}{}", stylist.quote(), stylist.quote()),
            mode_param.range(),
        )));
    }

    diagnostic
}

fn create_remove_param_fix(
    call: &ast::ExprCall,
    mode_param: &Expr,
    tokens: &Tokens,
) -> Result<Edit> {
    // Find the last comma before mode_param and create a deletion fix
    // starting from the comma and ending after mode_param.
    let mut fix_start: Option<TextSize> = None;
    let mut fix_end: Option<TextSize> = None;
    let mut is_first_arg: bool = false;
    let mut delete_first_arg: bool = false;

    for token in tokens.in_range(call.range()) {
        if token.start() == mode_param.start() {
            if is_first_arg {
                delete_first_arg = true;
                continue;
            }
            fix_end = Some(token.end());
            break;
        }
        match token.kind() {
            TokenKind::Name if delete_first_arg => {
                fix_end = Some(token.start());
                break;
            }
            TokenKind::Lpar => {
                is_first_arg = true;
                fix_start = Some(token.end());
            }
            TokenKind::Comma => {
                is_first_arg = false;
                if !delete_first_arg {
                    fix_start = Some(token.start());
                }
            }
            _ => {}
        }
    }

    match (fix_start, fix_end) {
        (Some(start), Some(end)) => Ok(Edit::deletion(start, end)),
        _ => Err(anyhow::anyhow!(
            "Failed to locate start and end parentheses"
        )),
    }
}
