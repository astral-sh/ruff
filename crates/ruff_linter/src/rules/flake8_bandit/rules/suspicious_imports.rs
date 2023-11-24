//! Check for imports of or from suspicious modules.
//!
//! See: <https://bandit.readthedocs.io/en/latest/blacklists/blacklist_imports.html>
use ruff_diagnostics::{Diagnostic, DiagnosticKind, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Stmt};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

// TODO: Docs
// ref: https://github.com/PyCQA/bandit/blob/6b2e24722bdcc40ea37c3bc155b6856961763814/bandit/blacklists/imports.py#L17
#[violation]
pub struct SuspiciousTelnetlibImport;

impl Violation for SuspiciousTelnetlibImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`telnetlib` and related modules are considered insecure. Use SSH or some other encrypted protocol")
    }
}

#[violation]
pub struct SuspiciousFtplibImport;

impl Violation for SuspiciousFtplibImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`ftplib` and related modules are considered insecure. Use SSH/SFTP/SCP or some other encrypted protocol")
    }
}

#[violation]
pub struct SuspiciousPickleImport;

impl Violation for SuspiciousPickleImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`pickle`, `cPickle`, `dill` and `shelve` modules are possibly insecure")
    }
}

#[violation]
pub struct SuspiciousSubprocessImport;

impl Violation for SuspiciousSubprocessImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`subprocess` module is possibly insecure")
    }
}

#[violation]
pub struct SuspiciousXmlEtreeImport;

impl Violation for SuspiciousXmlEtreeImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`xml.etree` methods are vulnerable to XML attacks")
    }
}

#[violation]
pub struct SuspiciousXmlSaxImport;

impl Violation for SuspiciousXmlSaxImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`xml.sax` methods are vulnerable to XML attacks")
    }
}

#[violation]
pub struct SuspiciousXmlExpatImport;

impl Violation for SuspiciousXmlExpatImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`xml.dom.expatbuilder` is vulnerable to XML attacks")
    }
}

#[violation]
pub struct SuspiciousXmlMinidomImport;

impl Violation for SuspiciousXmlMinidomImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`xml.dom.minidom` is vulnerable to XML attacks")
    }
}

#[violation]
pub struct SuspiciousXmlPulldomImport;

impl Violation for SuspiciousXmlPulldomImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`xml.dom.pulldom` is vulnerable to XML attacks")
    }
}

#[violation]
pub struct SuspiciousLxmlImport;

impl Violation for SuspiciousLxmlImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`lxml` is vulnerable to XML attacks")
    }
}

#[violation]
pub struct SuspiciousXmlrpclibImport;

impl Violation for SuspiciousXmlrpclibImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("XMLRPC is particularly dangerous as it is also concerned with communicating data over a network")
    }
}

#[violation]
pub struct SuspiciousHttpoxyImport;

impl Violation for SuspiciousHttpoxyImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`httpoxy` is a set of vulnerabilities that affect application code running inCGI, or CGI-like environments. The use of CGI for web applications should be avoided")
    }
}

#[violation]
pub struct SuspiciousPycryptoImport;

impl Violation for SuspiciousPycryptoImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`pycrypto` library is known to have publicly disclosed buffer overflow vulnerability"
        )
    }
}

#[violation]
pub struct SuspiciousPyghmiImport;

impl Violation for SuspiciousPyghmiImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("An IPMI-related module is being imported. IPMI is considered insecure. Use an encrypted protocol")
    }
}

/// S401, S402, S403, S404, S405, S406, S407, S408, S409, S410, S411, S412, S413
pub(crate) fn suspicious_imports(checker: &mut Checker, stmt: &Stmt) {
    match stmt {
        Stmt::Import(ast::StmtImport { names, .. }) => {
            for name in names {
                match name.name.as_str() {
                    "telnetlib" => check_and_push_diagnostic(
                        checker,
                        DiagnosticKind::from(SuspiciousTelnetlibImport),
                        name.range,
                    ),
                    "ftplib" => check_and_push_diagnostic(
                        checker,
                        DiagnosticKind::from(SuspiciousFtplibImport),
                        name.range,
                    ),
                    "pickle" | "cPickle" | "dill" | "shelve" => check_and_push_diagnostic(
                        checker,
                        DiagnosticKind::from(SuspiciousPickleImport),
                        name.range,
                    ),
                    "subprocess" => check_and_push_diagnostic(
                        checker,
                        DiagnosticKind::from(SuspiciousSubprocessImport),
                        name.range,
                    ),
                    "lxml" => check_and_push_diagnostic(
                        checker,
                        DiagnosticKind::from(SuspiciousLxmlImport),
                        name.range,
                    ),
                    "xmlrpc" => check_and_push_diagnostic(
                        checker,
                        DiagnosticKind::from(SuspiciousXmlrpclibImport),
                        name.range,
                    ),
                    "pyghmi" => check_and_push_diagnostic(
                        checker,
                        DiagnosticKind::from(SuspiciousPyghmiImport),
                        name.range,
                    ),
                    _ => {}
                }
            }
        }
        Stmt::ImportFrom(ast::StmtImportFrom { module, .. }) => {
            let Some(identifier) = module else { return };
            match identifier.as_str() {
                "telnetlib" => check_and_push_diagnostic(
                    checker,
                    DiagnosticKind::from(SuspiciousTelnetlibImport),
                    identifier.range(),
                ),
                "ftplib" => check_and_push_diagnostic(
                    checker,
                    DiagnosticKind::from(SuspiciousFtplibImport),
                    identifier.range(),
                ),
                "pickle" | "cPickle" | "dill" | "shelve" => check_and_push_diagnostic(
                    checker,
                    DiagnosticKind::from(SuspiciousPickleImport),
                    identifier.range(),
                ),
                "subprocess" => check_and_push_diagnostic(
                    checker,
                    DiagnosticKind::from(SuspiciousSubprocessImport),
                    identifier.range(),
                ),
                "lxml" => check_and_push_diagnostic(
                    checker,
                    DiagnosticKind::from(SuspiciousLxmlImport),
                    identifier.range(),
                ),
                "xmlrpc" => check_and_push_diagnostic(
                    checker,
                    DiagnosticKind::from(SuspiciousXmlrpclibImport),
                    identifier.range(),
                ),
                "pyghmi" => check_and_push_diagnostic(
                    checker,
                    DiagnosticKind::from(SuspiciousPyghmiImport),
                    identifier.range(),
                ),
                _ => {}
            }
        }
        _ => panic!("Expected Stmt::Import | Stmt::ImportFrom"),
    };
}

fn check_and_push_diagnostic(
    checker: &mut Checker,
    diagnostic_kind: DiagnosticKind,
    range: TextRange,
) {
    let diagnostic = Diagnostic::new::<DiagnosticKind>(diagnostic_kind, range);
    if checker.enabled(diagnostic.kind.rule()) {
        checker.diagnostics.push(diagnostic);
    }
}
