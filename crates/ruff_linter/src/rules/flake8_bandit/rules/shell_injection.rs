//! Checks relating to shell injection.

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::Truthiness;
use ruff_python_ast::{self as ast, Arguments, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::{
    checkers::ast::Checker, registry::Rule, rules::flake8_bandit::helpers::string_literal,
};

/// ## What it does
/// Check for method calls that initiate a subprocess with a shell.
///
/// ## Why is this bad?
/// Starting a subprocess with a shell can allow attackers to execute arbitrary
/// shell commands. Consider starting the process without a shell call and
/// sanitize the input to mitigate the risk of shell injection.
///
/// ## Example
/// ```python
/// import subprocess
///
/// subprocess.run("ls -l", shell=True)
/// ```
///
/// Use instead:
/// ```python
/// import subprocess
///
/// subprocess.run(["ls", "-l"])
/// ```
///
/// ## References
/// - [Python documentation: `subprocess` — Subprocess management](https://docs.python.org/3/library/subprocess.html)
/// - [Common Weakness Enumeration: CWE-78](https://cwe.mitre.org/data/definitions/78.html)
#[violation]
pub struct SubprocessPopenWithShellEqualsTrue {
    safety: Safety,
    is_exact: bool,
}

impl Violation for SubprocessPopenWithShellEqualsTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        match (self.safety, self.is_exact) {
            (Safety::SeemsSafe, true) => format!(
                "`subprocess` call with `shell=True` seems safe, but may be changed in the future; consider rewriting without `shell`"
            ),
            (Safety::Unknown, true) => format!("`subprocess` call with `shell=True` identified, security issue"),
            (Safety::SeemsSafe, false) => format!(
                "`subprocess` call with truthy `shell` seems safe, but may be changed in the future; consider rewriting without `shell`"
            ),
            (Safety::Unknown, false) => format!("`subprocess` call with truthy `shell` identified, security issue"),
        }
    }
}

/// ## What it does
/// Check for method calls that initiate a subprocess without a shell.
///
/// ## Why is this bad?
/// Starting a subprocess without a shell can prevent attackers from executing
/// arbitrary shell commands; however, it is still error-prone. Consider
/// validating the input.
///
/// ## Known problems
/// Prone to false positives as it is difficult to determine whether the
/// passed arguments have been validated ([#4045]).
///
/// ## Example
/// ```python
/// import subprocess
///
/// cmd = input("Enter a command: ").split()
/// subprocess.run(cmd)
/// ```
///
/// ## References
/// - [Python documentation: `subprocess` — Subprocess management](https://docs.python.org/3/library/subprocess.html)
///
/// [#4045]: https://github.com/astral-sh/ruff/issues/4045
#[violation]
pub struct SubprocessWithoutShellEqualsTrue;

impl Violation for SubprocessWithoutShellEqualsTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`subprocess` call: check for execution of untrusted input")
    }
}

/// ## What it does
/// Checks for method calls that set the `shell` parameter to `true` or another
/// truthy value when invoking a subprocess.
///
/// ## Why is this bad?
/// Setting the `shell` parameter to `true` or another truthy value when
/// invoking a subprocess can introduce security vulnerabilities, as it allows
/// shell metacharacters and whitespace to be passed to child processes,
/// potentially leading to shell injection attacks.
///
/// It is recommended to avoid using `shell=True` unless absolutely necessary
/// and, when used, to ensure that all inputs are properly sanitized and quoted
/// to prevent such vulnerabilities.
///
/// ## Known problems
/// Prone to false positives as it is triggered on any function call with a
/// `shell=True` parameter.
///
/// ## Example
/// ```python
/// import subprocess
///
/// user_input = input("Enter a command: ")
/// subprocess.run(user_input, shell=True)
/// ```
///
/// ## References
/// - [Python documentation: Security Considerations](https://docs.python.org/3/library/subprocess.html#security-considerations)
#[violation]
pub struct CallWithShellEqualsTrue {
    is_exact: bool,
}

impl Violation for CallWithShellEqualsTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.is_exact {
            format!("Function call with `shell=True` parameter identified, security issue")
        } else {
            format!("Function call with truthy `shell` parameter identified, security issue")
        }
    }
}

/// ## What it does
/// Checks for calls that start a process with a shell, providing guidance on
/// whether the usage is safe or not.
///
/// ## Why is this bad?
/// Starting a process with a shell can introduce security risks, such as
/// code injection vulnerabilities. It's important to be aware of whether the
/// usage of the shell is safe or not.
///
/// This rule triggers on functions like `os.system`, `popen`, etc., which
/// start processes with a shell. It evaluates whether the provided command
/// is a literal string or an expression. If the command is a literal string,
/// it's considered safe. If the command is an expression, it's considered
/// (potentially) unsafe.
///
/// ## Example
/// ```python
/// import os
///
/// # Safe usage (literal string)
/// command = "ls -l"
/// os.system(command)
///
/// # Potentially unsafe usage (expression)
/// cmd = get_user_input()
/// os.system(cmd)
/// ```
///
/// ## Note
/// The `subprocess` module provides more powerful facilities for spawning new
/// processes and retrieving their results, and using that module is preferable
/// to using `os.system` or similar functions. Consider replacing such usages
/// with `subprocess.call` or related functions.
///
/// ## References
/// - [Python documentation: `subprocess`](https://docs.python.org/3/library/subprocess.html)
#[violation]
pub struct StartProcessWithAShell {
    safety: Safety,
}

impl Violation for StartProcessWithAShell {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.safety {
            Safety::SeemsSafe => format!("Starting a process with a shell: seems safe, but may be changed in the future; consider rewriting without `shell`"),
            Safety::Unknown => format!("Starting a process with a shell, possible injection detected"),
        }
    }
}

/// ## What it does
/// Checks for functions that start a process without a shell.
///
/// ## Why is this bad?
/// The `subprocess` module provides more powerful facilities for spawning new
/// processes and retrieving their results; using that module is preferable to
/// using these functions.
///
/// ## Example
/// ```python
/// os.spawnlp(os.P_NOWAIT, "/bin/mycmd", "mycmd", "myarg")
/// ```
///
/// Use instead:
/// ```python
/// subprocess.Popen(["/bin/mycmd", "myarg"])
/// ```
///
/// ## References
/// - [Python documentation: Replacing the `os.spawn` family](https://docs.python.org/3/library/subprocess.html#replacing-the-os-spawn-family)
#[violation]
pub struct StartProcessWithNoShell;

impl Violation for StartProcessWithNoShell {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Starting a process without a shell")
    }
}

/// ## What it does
/// Checks for the starting of a process with a partial executable path.
///
/// ## Why is this bad?
/// Starting a process with a partial executable path can allow attackers to
/// execute an arbitrary executable by adjusting the `PATH` environment variable.
/// Consider using a full path to the executable instead.
///
/// ## Example
/// ```python
/// import subprocess
///
/// subprocess.Popen(["ruff", "check", "file.py"])
/// ```
///
/// Use instead:
/// ```python
/// import subprocess
///
/// subprocess.Popen(["/usr/bin/ruff", "check", "file.py"])
/// ```
///
/// ## References
/// - [Python documentation: `subprocess.Popen()`](https://docs.python.org/3/library/subprocess.html#subprocess.Popen)
/// - [Common Weakness Enumeration: CWE-426](https://cwe.mitre.org/data/definitions/426.html)
#[violation]
pub struct StartProcessWithPartialPath;

impl Violation for StartProcessWithPartialPath {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Starting a process with a partial executable path")
    }
}

/// ## What it does
/// Checks for possible wildcard injections in calls to `subprocess.Popen()`.
///
/// ## Why is this bad?
/// Wildcard injections can lead to unexpected behavior if unintended files are
/// matched by the wildcard. Consider using a more specific path instead.
///
/// ## Example
/// ```python
/// import subprocess
///
/// subprocess.Popen(["chmod", "777", "*.py"])
/// ```
///
/// Use instead:
/// ```python
/// import subprocess
///
/// subprocess.Popen(["chmod", "777", "main.py"])
/// ```
///
/// ## References
/// - [Common Weakness Enumeration: CWE-78](https://cwe.mitre.org/data/definitions/78.html)
#[violation]
pub struct UnixCommandWildcardInjection;

impl Violation for UnixCommandWildcardInjection {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Possible wildcard injection in call due to `*` usage")
    }
}

/// S602, S603, S604, S605, S606, S607, S609
pub(crate) fn shell_injection(checker: &mut Checker, call: &ast::ExprCall) {
    let call_kind = get_call_kind(&call.func, checker.semantic());
    let shell_keyword = find_shell_keyword(&call.arguments, checker.semantic());

    if matches!(call_kind, Some(CallKind::Subprocess)) {
        if let Some(arg) = call.arguments.args.first() {
            match shell_keyword {
                // S602
                Some(ShellKeyword {
                    truthiness: truthiness @ (Truthiness::True | Truthiness::Truthy),
                }) => {
                    if checker.enabled(Rule::SubprocessPopenWithShellEqualsTrue) {
                        checker.diagnostics.push(Diagnostic::new(
                            SubprocessPopenWithShellEqualsTrue {
                                safety: Safety::from(arg),
                                is_exact: matches!(truthiness, Truthiness::True),
                            },
                            call.func.range(),
                        ));
                    }
                }
                // S603
                Some(ShellKeyword {
                    truthiness: Truthiness::False | Truthiness::Falsey | Truthiness::Unknown,
                }) => {
                    if checker.enabled(Rule::SubprocessWithoutShellEqualsTrue) {
                        checker.diagnostics.push(Diagnostic::new(
                            SubprocessWithoutShellEqualsTrue,
                            call.func.range(),
                        ));
                    }
                }
                // S603
                None => {
                    if checker.enabled(Rule::SubprocessWithoutShellEqualsTrue) {
                        checker.diagnostics.push(Diagnostic::new(
                            SubprocessWithoutShellEqualsTrue,
                            call.func.range(),
                        ));
                    }
                }
            }
        }
    } else if let Some(ShellKeyword {
        truthiness: truthiness @ (Truthiness::True | Truthiness::Truthy),
    }) = shell_keyword
    {
        // S604
        if checker.enabled(Rule::CallWithShellEqualsTrue) {
            checker.diagnostics.push(Diagnostic::new(
                CallWithShellEqualsTrue {
                    is_exact: matches!(truthiness, Truthiness::True),
                },
                call.func.range(),
            ));
        }
    }

    // S605
    if checker.enabled(Rule::StartProcessWithAShell) {
        if matches!(call_kind, Some(CallKind::Shell)) {
            if let Some(arg) = call.arguments.args.first() {
                checker.diagnostics.push(Diagnostic::new(
                    StartProcessWithAShell {
                        safety: Safety::from(arg),
                    },
                    call.func.range(),
                ));
            }
        }
    }

    // S606
    if checker.enabled(Rule::StartProcessWithNoShell) {
        if matches!(call_kind, Some(CallKind::NoShell)) {
            checker
                .diagnostics
                .push(Diagnostic::new(StartProcessWithNoShell, call.func.range()));
        }
    }

    // S607
    if checker.enabled(Rule::StartProcessWithPartialPath) {
        if call_kind.is_some() {
            if let Some(arg) = call.arguments.args.first() {
                if is_partial_path(arg) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(StartProcessWithPartialPath, arg.range()));
                }
            }
        }
    }

    // S609
    if checker.enabled(Rule::UnixCommandWildcardInjection) {
        if matches!(call_kind, Some(CallKind::Shell))
            || matches!(
                (call_kind, shell_keyword),
                (
                    Some(CallKind::Subprocess),
                    Some(ShellKeyword {
                        truthiness: Truthiness::True | Truthiness::Truthy,
                    })
                )
            )
        {
            if let Some(arg) = call.arguments.args.first() {
                if is_wildcard_command(arg) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(UnixCommandWildcardInjection, arg.range()));
                }
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum CallKind {
    Subprocess,
    Shell,
    NoShell,
}

/// Return the [`CallKind`] of the given function call.
fn get_call_kind(func: &Expr, semantic: &SemanticModel) -> Option<CallKind> {
    semantic
        .resolve_qualified_name(func)
        .and_then(|qualified_name| match qualified_name.segments() {
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
                    "getoutput" | "getstatusoutput" => Some(CallKind::Shell),
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
struct ShellKeyword {
    /// Whether the `shell` keyword argument is set and evaluates to `True`.
    truthiness: Truthiness,
}

/// Return the `shell` keyword argument to the given function call, if any.
fn find_shell_keyword(arguments: &Arguments, semantic: &SemanticModel) -> Option<ShellKeyword> {
    arguments.find_keyword("shell").map(|keyword| ShellKeyword {
        truthiness: Truthiness::from_expr(&keyword.value, |id| semantic.has_builtin_binding(id)),
    })
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Safety {
    SeemsSafe,
    Unknown,
}

impl From<&Expr> for Safety {
    /// Return the [`Safety`] level for the [`Expr`]. This is based on Bandit's definition: string
    /// literals are considered okay, but dynamically-computed values are not.
    fn from(expr: &Expr) -> Self {
        if expr.is_string_literal_expr() {
            Self::SeemsSafe
        } else {
            Self::Unknown
        }
    }
}

/// Return `true` if the string appears to be a full file path.
///
/// ## Examples
/// ```python
/// import subprocess
///
/// os.system("/bin/ls")
/// os.system("./bin/ls")
/// os.system(["/bin/ls"])
/// os.system(["/bin/ls", "/tmp"])
/// os.system(r"C:\\bin\ls")
fn is_full_path(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first_char) = chars.next() else {
        return false;
    };

    // Ex) `/bin/ls`
    if first_char == '\\' || first_char == '/' || first_char == '.' {
        return true;
    }

    // Ex) `C:`
    if first_char.is_alphabetic() {
        if let Some(second_char) = chars.next() {
            if second_char == ':' {
                return true;
            }
        }
    }

    false
}

/// Return `true` if the [`Expr`] is a string literal or list of string literals that starts with a
/// partial path.
fn is_partial_path(expr: &Expr) -> bool {
    let string_literal = match expr {
        Expr::List(ast::ExprList { elts, .. }) => elts.first().and_then(string_literal),
        _ => string_literal(expr),
    };
    string_literal.is_some_and(|text| !is_full_path(text))
}

/// Return `true` if the [`Expr`] is a wildcard command.
///
/// ## Examples
/// ```python
/// import subprocess
///
/// subprocess.Popen("/bin/chown root: *", shell=True)
/// subprocess.Popen(["/usr/local/bin/rsync", "*", "some_where:"], shell=True)
/// ```
fn is_wildcard_command(expr: &Expr) -> bool {
    if let Expr::List(list) = expr {
        let mut has_star = false;
        let mut has_command = false;
        for item in list {
            if let Some(text) = string_literal(item) {
                has_star |= text.contains('*');
                has_command |= text.contains("chown")
                    || text.contains("chmod")
                    || text.contains("tar")
                    || text.contains("rsync");
            }
            if has_star && has_command {
                break;
            }
        }
        has_star && has_command
    } else {
        let string_literal = string_literal(expr);
        string_literal.is_some_and(|text| {
            text.contains('*')
                && (text.contains("chown")
                    || text.contains("chmod")
                    || text.contains("tar")
                    || text.contains("rsync"))
        })
    }
}
