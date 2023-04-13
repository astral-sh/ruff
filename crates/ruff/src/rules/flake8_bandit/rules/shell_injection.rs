//! Checks relating to shell injection.

use num_bigint::BigInt;
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_semantic::context::Context;

use crate::{
    checkers::ast::Checker, registry::Rule, rules::flake8_bandit::helpers::string_literal,
};

static FULL_PATH_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^([A-Za-z]:|[\\/.])").unwrap());

#[violation]
pub struct SubprocessPopenWithShellEqualsTrue {
    seems_safe: bool,
}

impl Violation for SubprocessPopenWithShellEqualsTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.seems_safe {
            format!(
                "`subprocess` call with `shell=True` seems safe, but may be changed in the future; consider rewriting without `shell`"
            )
        } else {
            format!("`subprocess` call with `shell=True` identified, security issue")
        }
    }
}

#[violation]
pub struct SubprocessWithoutShellEqualsTrue;

impl Violation for SubprocessWithoutShellEqualsTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`subprocess` call: check for execution of untrusted input")
    }
}

#[violation]
pub struct CallWithShellEqualsTrue;

impl Violation for CallWithShellEqualsTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function call with `shell=True` parameter identified, security issue")
    }
}

#[violation]
pub struct StartProcessWithAShell {
    seems_safe: bool,
}

impl Violation for StartProcessWithAShell {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.seems_safe {
            format!("Starting a process with a shell: seems safe, but may be changed in the future; consider rewriting without `shell`")
        } else {
            format!("Starting a process with a shell, possible injection detected")
        }
    }
}

#[violation]
pub struct StartProcessWithNoShell;

impl Violation for StartProcessWithNoShell {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Starting a process without a shell")
    }
}

#[violation]
pub struct StartProcessWithPartialPath;

impl Violation for StartProcessWithPartialPath {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Starting a process with a partial executable path")
    }
}

#[derive(Copy, Clone, Debug)]
enum CallKind {
    Subprocess,
    Shell,
    NoShell,
}

/// Return the [`CallKind`] of the given function call.
fn get_call_kind(func: &Expr, context: &Context) -> Option<CallKind> {
    context
        .resolve_call_path(func)
        .and_then(|call_path| match call_path.as_slice() {
            &[module, submodule] => match module {
                "os" => match submodule {
                    "execl" | "execle" | "execlp" | "execlpe" | "execv" | "execve" | "execvp"
                    | "execvpe" | "spawnl" | "spawnle" | "spawnlp" | "spawnlpe" | "spawnv"
                    | "spawnve" | "spawnvp" | "spawnvpe" | "startfile" => Some(CallKind::NoShell),
                    "system" | "popen" | "popen2" | "popen3" | "popen4" => Some(CallKind::Shell),
                    _ => None,
                },
                "subprocess" => match submodule {
                    "Popen" | "call" | "check_call" | "check_output" | "run" => {
                        Some(CallKind::Subprocess)
                    }
                    _ => None,
                },
                "popen2" => match submodule {
                    "popen2" | "popen3" | "popen4" | "Popen3" | "Popen4" => Some(CallKind::Shell),
                    _ => None,
                },
                "commands" => match submodule {
                    "getoutput" | "getstatusoutput" => Some(CallKind::Shell),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
}

#[derive(Copy, Clone, Debug)]
enum Truthiness {
    // The `shell` keyword argument is set and evaluates to `False`.
    Falsey,
    // The `shell` keyword argument is set and evaluates to `True`.
    Truthy,
    // The `shell` keyword argument is set, but its value is unknown.
    Unknown,
}

impl From<&Keyword> for Truthiness {
    fn from(value: &Keyword) -> Self {
        match &value.node.value.node {
            ExprKind::Constant {
                value: Constant::Bool(b),
                ..
            } => {
                if *b {
                    Truthiness::Truthy
                } else {
                    Truthiness::Falsey
                }
            }
            ExprKind::Constant {
                value: Constant::Int(int),
                ..
            } => {
                if int == &BigInt::from(0u8) {
                    Truthiness::Falsey
                } else {
                    Truthiness::Truthy
                }
            }
            ExprKind::Constant {
                value: Constant::Float(float),
                ..
            } => {
                if (float - 0.0).abs() < f64::EPSILON {
                    Truthiness::Falsey
                } else {
                    Truthiness::Truthy
                }
            }
            ExprKind::Constant {
                value: Constant::None,
                ..
            } => Truthiness::Falsey,
            ExprKind::List { elts, .. }
            | ExprKind::Set { elts, .. }
            | ExprKind::Tuple { elts, .. } => {
                if elts.is_empty() {
                    Truthiness::Falsey
                } else {
                    Truthiness::Truthy
                }
            }
            ExprKind::Dict { keys, .. } => {
                if keys.is_empty() {
                    Truthiness::Falsey
                } else {
                    Truthiness::Truthy
                }
            }
            _ => Truthiness::Unknown,
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct ShellKeyword<'a> {
    /// Whether the `shell` keyword argument is set and evaluates to `True`.
    truthiness: Truthiness,
    /// The `shell` keyword argument.
    keyword: &'a Keyword,
}

/// Return the `shell` keyword argument to the given function call, if any.
fn find_shell_keyword(keywords: &[Keyword]) -> Option<ShellKeyword> {
    keywords
        .iter()
        .find(|keyword| {
            keyword
                .node
                .arg
                .as_ref()
                .map_or(false, |arg| arg == "shell")
        })
        .map(|keyword| ShellKeyword {
            truthiness: keyword.into(),
            keyword,
        })
}

/// Return `true` if the value provided to the `shell` call seems safe. This is based on Bandit's
/// definition: string literals are considered okay, but dynamically-computed values are not.
fn shell_call_seems_safe(arg: &Expr) -> bool {
    matches!(
        arg.node,
        ExprKind::Constant {
            value: Constant::Str(_),
            ..
        }
    )
}

/// Return the [`Expr`] as a string literal, if it's a string or a list of strings.
fn try_string_literal(expr: &Expr) -> Option<&str> {
    match &expr.node {
        ExprKind::List { elts, .. } => {
            if elts.is_empty() {
                None
            } else {
                string_literal(&elts[0])
            }
        }
        _ => string_literal(expr),
    }
}

/// S602, S603, S604, S605, S606, S607
pub fn shell_injection(checker: &mut Checker, func: &Expr, args: &[Expr], keywords: &[Keyword]) {
    let call_kind = get_call_kind(func, &checker.ctx);

    if matches!(call_kind, Some(CallKind::Subprocess)) {
        if let Some(arg) = args.first() {
            match find_shell_keyword(keywords) {
                // S602
                Some(ShellKeyword {
                    truthiness: Truthiness::Truthy,
                    keyword,
                }) => {
                    if checker
                        .settings
                        .rules
                        .enabled(Rule::SubprocessPopenWithShellEqualsTrue)
                    {
                        checker.diagnostics.push(Diagnostic::new(
                            SubprocessPopenWithShellEqualsTrue {
                                seems_safe: shell_call_seems_safe(arg),
                            },
                            Range::from(keyword),
                        ));
                    }
                }
                // S603
                Some(ShellKeyword {
                    truthiness: Truthiness::Falsey | Truthiness::Unknown,
                    keyword,
                }) => {
                    if checker
                        .settings
                        .rules
                        .enabled(Rule::SubprocessWithoutShellEqualsTrue)
                    {
                        checker.diagnostics.push(Diagnostic::new(
                            SubprocessWithoutShellEqualsTrue,
                            Range::from(keyword),
                        ));
                    }
                }
                // S603
                None => {
                    if checker
                        .settings
                        .rules
                        .enabled(Rule::SubprocessWithoutShellEqualsTrue)
                    {
                        checker.diagnostics.push(Diagnostic::new(
                            SubprocessWithoutShellEqualsTrue,
                            Range::from(arg),
                        ));
                    }
                }
            }
        }
    } else if let Some(ShellKeyword {
        truthiness: Truthiness::Truthy,
        keyword,
    }) = find_shell_keyword(keywords)
    {
        // S604
        if checker
            .settings
            .rules
            .enabled(Rule::CallWithShellEqualsTrue)
        {
            checker.diagnostics.push(Diagnostic::new(
                CallWithShellEqualsTrue,
                Range::from(keyword),
            ));
        }
    }

    // S605
    if matches!(call_kind, Some(CallKind::Shell)) {
        if let Some(arg) = args.first() {
            if checker.settings.rules.enabled(Rule::StartProcessWithAShell) {
                checker.diagnostics.push(Diagnostic::new(
                    StartProcessWithAShell {
                        seems_safe: shell_call_seems_safe(arg),
                    },
                    Range::from(arg),
                ));
            }
        }
    }

    // S606
    if matches!(call_kind, Some(CallKind::NoShell)) {
        if checker
            .settings
            .rules
            .enabled(Rule::StartProcessWithNoShell)
        {
            checker
                .diagnostics
                .push(Diagnostic::new(StartProcessWithNoShell, Range::from(func)));
        }
    }

    // S607
    if call_kind.is_some() {
        if let Some(arg) = args.first() {
            if checker
                .settings
                .rules
                .enabled(Rule::StartProcessWithPartialPath)
            {
                if let Some(value) = try_string_literal(arg) {
                    if FULL_PATH_REGEX.find(value).is_none() {
                        checker.diagnostics.push(Diagnostic::new(
                            StartProcessWithPartialPath,
                            Range::from(arg),
                        ));
                    }
                }
            }
        }
    }
}
