//! Checks relating to shell injection

use num_bigint::BigInt;
use once_cell::sync::Lazy;
use regex::Regex;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword};

use crate::{
    checkers::ast::Checker, registry::Rule, rules::flake8_bandit::helpers::string_literal,
};

static FULL_PATH_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^([A-Za-z]:|[\\/\.])").unwrap());

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
            format!("`subprocess` call with `shell=True` identified")
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
        format!("Function call with `shell=True` parameter identified")
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

struct Config<'a> {
    subprocess: Vec<Vec<&'a str>>,
    shell: Vec<Vec<&'a str>>,
    no_shell: Vec<Vec<&'a str>>,
}

static CONFIG: Lazy<Config> = Lazy::new(|| Config {
    subprocess: vec![
        vec!["subprocess", "Popen"],
        vec!["subprocess", "call"],
        vec!["subprocess", "check_call"],
        vec!["subprocess", "check_output"],
        vec!["subprocess", "run"],
    ],
    shell: vec![
        vec!["os", "system"],
        vec!["os", "popen"],
        vec!["os", "popen2"],
        vec!["os", "popen3"],
        vec!["os", "popen4"],
        vec!["popen2", "popen2"],
        vec!["popen2", "popen3"],
        vec!["popen2", "popen4"],
        vec!["popen2", "Popen3"],
        vec!["popen2", "Popen4"],
        vec!["commands", "getoutput"],
        vec!["commands", "getstatusoutput"],
    ],
    no_shell: vec![
        vec!["os", "execl"],
        vec!["os", "execle"],
        vec!["os", "execlp"],
        vec!["os", "execlpe"],
        vec!["os", "execv"],
        vec!["os", "execve"],
        vec!["os", "execvp"],
        vec!["os", "execvpe"],
        vec!["os", "spawnl"],
        vec!["os", "spawnle"],
        vec!["os", "spawnlp"],
        vec!["os", "spawnlpe"],
        vec!["os", "spawnv"],
        vec!["os", "spawnve"],
        vec!["os", "spawnvp"],
        vec!["os", "spawnvpe"],
        vec!["os", "startfile"],
    ],
});

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
            ExprKind::List { elts, .. } => {
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
            ExprKind::Tuple { elts, .. } => {
                if elts.is_empty() {
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
    has_shell: Truthiness,
    keyword: &'a Keyword,
}

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
            has_shell: keyword.into(),
            keyword,
        })
}

fn shell_call_seems_safe(arg: &Expr) -> bool {
    matches!(
        arg.node,
        ExprKind::Constant {
            value: Constant::Str(_),
            ..
        }
    )
}

fn get_call_kind(checker: &mut Checker, func: &Expr) -> Option<CallKind> {
    checker.ctx.resolve_call_path(func).and_then(|call_path| {
        if CONFIG
            .subprocess
            .iter()
            .any(|subprocess| call_path.as_slice() == subprocess.as_slice())
        {
            Some(CallKind::Subprocess)
        } else if CONFIG
            .shell
            .iter()
            .any(|shell| call_path.as_slice() == shell.as_slice())
        {
            Some(CallKind::Shell)
        } else if CONFIG
            .no_shell
            .iter()
            .any(|no_shell| call_path.as_slice() == no_shell.as_slice())
        {
            Some(CallKind::NoShell)
        } else {
            None
        }
    })
}

fn string_literal_including_list(expr: &Expr) -> Option<&str> {
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
    let call_kind = get_call_kind(checker, func);

    if let Some(CallKind::Subprocess) = call_kind {
        if !args.is_empty() {
            match find_shell_keyword(keywords) {
                // S602
                Some(ShellKeyword {
                    has_shell: Truthiness::Truthy,
                    keyword,
                }) => {
                    if checker
                        .settings
                        .rules
                        .enabled(Rule::SubprocessPopenWithShellEqualsTrue)
                    {
                        checker.diagnostics.push(Diagnostic::new(
                            SubprocessPopenWithShellEqualsTrue {
                                seems_safe: shell_call_seems_safe(&args[0]),
                            },
                            Range::from(keyword),
                        ));
                    }
                }
                // S603
                Some(ShellKeyword {
                    has_shell: Truthiness::Falsey | Truthiness::Unknown,
                    keyword,
                }) => {
                    if checker
                        .settings
                        .rules
                        .enabled(Rule::SubprocessWithoutShellEqualsTrue)
                    {
                        checker.diagnostics.push(Diagnostic::new(
                            SubprocessWithoutShellEqualsTrue {},
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
                            SubprocessWithoutShellEqualsTrue {},
                            Range::from(&args[0]),
                        ));
                    }
                }
            }
        }
    } else if let Some(ShellKeyword {
        has_shell: Truthiness::Truthy,
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
                CallWithShellEqualsTrue {},
                Range::from(keyword),
            ));
        }
    }

    // S605
    if let Some(CallKind::Shell) = call_kind {
        if !args.is_empty() && checker.settings.rules.enabled(Rule::StartProcessWithAShell) {
            checker.diagnostics.push(Diagnostic::new(
                StartProcessWithAShell {
                    seems_safe: shell_call_seems_safe(&args[0]),
                },
                Range::from(&args[0]),
            ));
        }
    }

    // S606
    if let Some(CallKind::NoShell) = call_kind {
        if checker
            .settings
            .rules
            .enabled(Rule::StartProcessWithNoShell)
        {
            checker.diagnostics.push(Diagnostic::new(
                StartProcessWithNoShell {},
                Range::from(func),
            ));
        }
    }

    // S607
    if call_kind.is_some() && !args.is_empty() {
        if let Some(value) = string_literal_including_list(&args[0]) {
            if FULL_PATH_REGEX.find(value).is_none()
                && checker
                    .settings
                    .rules
                    .enabled(Rule::StartProcessWithPartialPath)
            {
                checker.diagnostics.push(Diagnostic::new(
                    StartProcessWithPartialPath {},
                    Range::from(&args[0]),
                ));
            }
        }
    }
}
