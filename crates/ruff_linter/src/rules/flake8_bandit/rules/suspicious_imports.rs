//! Check for imports of or from suspicious modules.
//!
//! See: <https://bandit.readthedocs.io/en/latest/blacklists/blacklist_imports.html>
use ruff_diagnostics::{Diagnostic, DiagnosticKind, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Stmt};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for imports of the `telnetlib` module.
///
/// ## Why is this bad?
/// Telnet is considered insecure. It is deprecated since version 3.11, and
/// was removed in version 3.13. Instead, use SSH or another encrypted
/// protocol.
///
/// ## Example
/// ```python
/// import telnetlib
/// ```
///
/// ## References
/// - [Python documentation: `telnetlib` - Telnet client](https://docs.python.org/3.12/library/telnetlib.html#module-telnetlib)
/// - [PEP 594: `telnetlib`](https://peps.python.org/pep-0594/#telnetlib)
#[derive(ViolationMetadata)]
pub(crate) struct SuspiciousTelnetlibImport;

impl Violation for SuspiciousTelnetlibImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`telnetlib` and related modules are considered insecure. Use SSH or another encrypted protocol.".to_string()
    }
}

/// ## What it does
/// Checks for imports of the `ftplib` module.
///
/// ## Why is this bad?
/// FTP is considered insecure. Instead, use SSH, SFTP, SCP, or another
/// encrypted protocol.
///
/// ## Example
/// ```python
/// import ftplib
/// ```
///
/// ## References
/// - [Python documentation: `ftplib` - FTP protocol client](https://docs.python.org/3/library/ftplib.html)
#[derive(ViolationMetadata)]
pub(crate) struct SuspiciousFtplibImport;

impl Violation for SuspiciousFtplibImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`ftplib` and related modules are considered insecure. Use SSH, SFTP, SCP, or another encrypted protocol.".to_string()
    }
}

/// ## What it does
/// Checks for imports of the `pickle`, `cPickle`, `dill`, and `shelve` modules.
///
/// ## Why is this bad?
/// It is possible to construct malicious pickle data which will execute
/// arbitrary code during unpickling. Consider possible security implications
/// associated with these modules.
///
/// ## Example
/// ```python
/// import pickle
/// ```
///
/// ## References
/// - [Python documentation: `pickle` — Python object serialization](https://docs.python.org/3/library/pickle.html)
#[derive(ViolationMetadata)]
pub(crate) struct SuspiciousPickleImport;

impl Violation for SuspiciousPickleImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`pickle`, `cPickle`, `dill`, and `shelve` modules are possibly insecure".to_string()
    }
}

/// ## What it does
/// Checks for imports of the `subprocess` module.
///
/// ## Why is this bad?
/// It is possible to inject malicious commands into subprocess calls. Consider
/// possible security implications associated with this module.
///
/// ## Example
/// ```python
/// import subprocess
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct SuspiciousSubprocessImport;

impl Violation for SuspiciousSubprocessImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`subprocess` module is possibly insecure".to_string()
    }
}

/// ## What it does
/// Checks for imports of the `xml.etree.cElementTree` and `xml.etree.ElementTree` modules
///
/// ## Why is this bad?
/// Using various methods from these modules to parse untrusted XML data is
/// known to be vulnerable to XML attacks. Replace vulnerable imports with the
/// equivalent `defusedxml` package, or make sure `defusedxml.defuse_stdlib()` is
/// called before parsing XML data.
///
/// ## Example
/// ```python
/// import xml.etree.cElementTree
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct SuspiciousXmlEtreeImport;

impl Violation for SuspiciousXmlEtreeImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`xml.etree` methods are vulnerable to XML attacks".to_string()
    }
}

/// ## What it does
/// Checks for imports of the `xml.sax` module.
///
/// ## Why is this bad?
/// Using various methods from these modules to parse untrusted XML data is
/// known to be vulnerable to XML attacks. Replace vulnerable imports with the
/// equivalent `defusedxml` package, or make sure `defusedxml.defuse_stdlib()` is
/// called before parsing XML data.
///
/// ## Example
/// ```python
/// import xml.sax
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct SuspiciousXmlSaxImport;

impl Violation for SuspiciousXmlSaxImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`xml.sax` methods are vulnerable to XML attacks".to_string()
    }
}

/// ## What it does
/// Checks for imports of the `xml.dom.expatbuilder` module.
///
/// ## Why is this bad?
/// Using various methods from these modules to parse untrusted XML data is
/// known to be vulnerable to XML attacks. Replace vulnerable imports with the
/// equivalent `defusedxml` package, or make sure `defusedxml.defuse_stdlib()` is
/// called before parsing XML data.
///
/// ## Example
/// ```python
/// import xml.dom.expatbuilder
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct SuspiciousXmlExpatImport;

impl Violation for SuspiciousXmlExpatImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`xml.dom.expatbuilder` is vulnerable to XML attacks".to_string()
    }
}

/// ## What it does
/// Checks for imports of the `xml.dom.minidom` module.
///
/// ## Why is this bad?
/// Using various methods from these modules to parse untrusted XML data is
/// known to be vulnerable to XML attacks. Replace vulnerable imports with the
/// equivalent `defusedxml` package, or make sure `defusedxml.defuse_stdlib()` is
/// called before parsing XML data.
///
/// ## Example
/// ```python
/// import xml.dom.minidom
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct SuspiciousXmlMinidomImport;

impl Violation for SuspiciousXmlMinidomImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`xml.dom.minidom` is vulnerable to XML attacks".to_string()
    }
}

/// ## What it does
/// Checks for imports of the `xml.dom.pulldom` module.
///
/// ## Why is this bad?
/// Using various methods from these modules to parse untrusted XML data is
/// known to be vulnerable to XML attacks. Replace vulnerable imports with the
/// equivalent `defusedxml` package, or make sure `defusedxml.defuse_stdlib()` is
/// called before parsing XML data.
///
/// ## Example
/// ```python
/// import xml.dom.pulldom
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct SuspiciousXmlPulldomImport;

impl Violation for SuspiciousXmlPulldomImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`xml.dom.pulldom` is vulnerable to XML attacks".to_string()
    }
}

/// ## Removed
/// This rule was removed as the `lxml` library has been modified to address
/// known vulnerabilities and unsafe defaults. As such, the `defusedxml`
/// library is no longer necessary, `defusedxml` has [deprecated] its `lxml`
/// module.
///
/// ## What it does
/// Checks for imports of the `lxml` module.
///
/// ## Why is this bad?
/// Using various methods from the `lxml` module to parse untrusted XML data is
/// known to be vulnerable to XML attacks. Replace vulnerable imports with the
/// equivalent `defusedxml` package.
///
/// ## Example
/// ```python
/// import lxml
/// ```
///
/// [deprecated]: https://github.com/tiran/defusedxml/blob/c7445887f5e1bcea470a16f61369d29870cfcfe1/README.md#defusedxmllxml
#[derive(ViolationMetadata)]
pub(crate) struct SuspiciousLxmlImport;

impl Violation for SuspiciousLxmlImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`lxml` is vulnerable to XML attacks".to_string()
    }
}

/// ## What it does
/// Checks for imports of the `xmlrpc` module.
///
/// ## Why is this bad?
/// XMLRPC is a particularly dangerous XML module, as it is also concerned with
/// communicating data over a network. Use the `defused.xmlrpc.monkey_patch()`
/// function to monkey-patch the `xmlrpclib` module and mitigate remote XML
/// attacks.
///
/// ## Example
/// ```python
/// import xmlrpc
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct SuspiciousXmlrpcImport;

impl Violation for SuspiciousXmlrpcImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        "XMLRPC is vulnerable to remote XML attacks".to_string()
    }
}

/// ## What it does
/// Checks for imports of `wsgiref.handlers.CGIHandler` and
/// `twisted.web.twcgi.CGIScript`.
///
/// ## Why is this bad?
/// httpoxy is a set of vulnerabilities that affect application code running in
/// CGI or CGI-like environments. The use of CGI for web applications should be
/// avoided to prevent this class of attack.
///
/// ## Example
/// ```python
/// import wsgiref.handlers.CGIHandler
/// ```
///
/// ## References
/// - [httpoxy website](https://httpoxy.org/)
#[derive(ViolationMetadata)]
pub(crate) struct SuspiciousHttpoxyImport;

impl Violation for SuspiciousHttpoxyImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`httpoxy` is a set of vulnerabilities that affect application code running inCGI, or CGI-like environments. The use of CGI for web applications should be avoided".to_string()
    }
}

/// ## What it does
/// Checks for imports of several unsafe cryptography modules.
///
/// ## Why is this bad?
/// The `pycrypto` library is known to have a publicly disclosed buffer
/// overflow vulnerability. It is no longer actively maintained and has been
/// deprecated in favor of the `pyca/cryptography` library.
///
/// ## Example
/// ```python
/// import Crypto.Random
/// ```
///
/// ## References
/// - [Buffer Overflow Issue](https://github.com/pycrypto/pycrypto/issues/176)
#[derive(ViolationMetadata)]
pub(crate) struct SuspiciousPycryptoImport;

impl Violation for SuspiciousPycryptoImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`pycrypto` library is known to have publicly disclosed buffer overflow vulnerability"
            .to_string()
    }
}

/// ## What it does
/// Checks for imports of the `pyghmi` module.
///
/// ## Why is this bad?
/// `pyghmi` is an IPMI-related module, but IPMI is considered insecure.
/// Instead, use an encrypted protocol.
///
/// ## Example
/// ```python
/// import pyghmi
/// ```
///
/// ## References
/// - [Buffer Overflow Issue](https://github.com/pycrypto/pycrypto/issues/176)
#[derive(ViolationMetadata)]
pub(crate) struct SuspiciousPyghmiImport;

impl Violation for SuspiciousPyghmiImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        "An IPMI-related module is being imported. Prefer an encrypted protocol over IPMI."
            .to_string()
    }
}

/// S401, S402, S403, S404, S405, S406, S407, S408, S409, S410, S411, S412, S413, S415
pub(crate) fn suspicious_imports(checker: &Checker, stmt: &Stmt) {
    // Skip stub files.
    if checker.source_type.is_stub() {
        return;
    }

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
                    "xml.etree.cElementTree" | "xml.etree.ElementTree" => {
                        check_and_push_diagnostic(
                            checker,
                            DiagnosticKind::from(SuspiciousXmlEtreeImport),
                            name.range,
                        );
                    }
                    "xml.sax" => check_and_push_diagnostic(
                        checker,
                        DiagnosticKind::from(SuspiciousXmlSaxImport),
                        name.range,
                    ),
                    "xml.dom.expatbuilder" => check_and_push_diagnostic(
                        checker,
                        DiagnosticKind::from(SuspiciousXmlExpatImport),
                        name.range,
                    ),
                    "xml.dom.minidom" => check_and_push_diagnostic(
                        checker,
                        DiagnosticKind::from(SuspiciousXmlMinidomImport),
                        name.range,
                    ),
                    "xml.dom.pulldom" => check_and_push_diagnostic(
                        checker,
                        DiagnosticKind::from(SuspiciousXmlPulldomImport),
                        name.range,
                    ),
                    "lxml" => check_and_push_diagnostic(
                        checker,
                        DiagnosticKind::from(SuspiciousLxmlImport),
                        name.range,
                    ),
                    "xmlrpc" => check_and_push_diagnostic(
                        checker,
                        DiagnosticKind::from(SuspiciousXmlrpcImport),
                        name.range,
                    ),
                    "Crypto.Cipher" | "Crypto.Hash" | "Crypto.IO" | "Crypto.Protocol"
                    | "Crypto.PublicKey" | "Crypto.Random" | "Crypto.Signature" | "Crypto.Util" => {
                        check_and_push_diagnostic(
                            checker,
                            DiagnosticKind::from(SuspiciousPycryptoImport),
                            name.range,
                        );
                    }
                    "pyghmi" => check_and_push_diagnostic(
                        checker,
                        DiagnosticKind::from(SuspiciousPyghmiImport),
                        name.range,
                    ),
                    _ => {}
                }
            }
        }
        Stmt::ImportFrom(ast::StmtImportFrom { module, names, .. }) => {
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
                "xml.etree" => {
                    for name in names {
                        if matches!(name.name.as_str(), "cElementTree" | "ElementTree") {
                            check_and_push_diagnostic(
                                checker,
                                DiagnosticKind::from(SuspiciousXmlEtreeImport),
                                identifier.range(),
                            );
                        }
                    }
                }
                "xml.etree.cElementTree" | "xml.etree.ElementTree" => {
                    check_and_push_diagnostic(
                        checker,
                        DiagnosticKind::from(SuspiciousXmlEtreeImport),
                        identifier.range(),
                    );
                }
                "xml" => {
                    for name in names {
                        if name.name.as_str() == "sax" {
                            check_and_push_diagnostic(
                                checker,
                                DiagnosticKind::from(SuspiciousXmlSaxImport),
                                identifier.range(),
                            );
                        }
                    }
                }
                "xml.sax" => check_and_push_diagnostic(
                    checker,
                    DiagnosticKind::from(SuspiciousXmlSaxImport),
                    identifier.range(),
                ),
                "xml.dom" => {
                    for name in names {
                        match name.name.as_str() {
                            "expatbuilder" => check_and_push_diagnostic(
                                checker,
                                DiagnosticKind::from(SuspiciousXmlExpatImport),
                                identifier.range(),
                            ),
                            "minidom" => check_and_push_diagnostic(
                                checker,
                                DiagnosticKind::from(SuspiciousXmlMinidomImport),
                                identifier.range(),
                            ),
                            "pulldom" => check_and_push_diagnostic(
                                checker,
                                DiagnosticKind::from(SuspiciousXmlPulldomImport),
                                identifier.range(),
                            ),
                            _ => (),
                        }
                    }
                }
                "xml.dom.expatbuilder" => check_and_push_diagnostic(
                    checker,
                    DiagnosticKind::from(SuspiciousXmlExpatImport),
                    identifier.range(),
                ),
                "xml.dom.minidom" => check_and_push_diagnostic(
                    checker,
                    DiagnosticKind::from(SuspiciousXmlMinidomImport),
                    identifier.range(),
                ),
                "xml.dom.pulldom" => check_and_push_diagnostic(
                    checker,
                    DiagnosticKind::from(SuspiciousXmlPulldomImport),
                    identifier.range(),
                ),
                "lxml" => check_and_push_diagnostic(
                    checker,
                    DiagnosticKind::from(SuspiciousLxmlImport),
                    identifier.range(),
                ),
                "xmlrpc" => check_and_push_diagnostic(
                    checker,
                    DiagnosticKind::from(SuspiciousXmlrpcImport),
                    identifier.range(),
                ),
                "wsgiref.handlers" => {
                    for name in names {
                        if name.name.as_str() == "CGIHandler" {
                            check_and_push_diagnostic(
                                checker,
                                DiagnosticKind::from(SuspiciousHttpoxyImport),
                                identifier.range(),
                            );
                        }
                    }
                }
                "twisted.web.twcgi" => {
                    for name in names {
                        if name.name.as_str() == "CGIScript" {
                            check_and_push_diagnostic(
                                checker,
                                DiagnosticKind::from(SuspiciousHttpoxyImport),
                                identifier.range(),
                            );
                        }
                    }
                }
                "Crypto" => {
                    for name in names {
                        if matches!(
                            name.name.as_str(),
                            "Cipher"
                                | "Hash"
                                | "IO"
                                | "Protocol"
                                | "PublicKey"
                                | "Random"
                                | "Signature"
                                | "Util"
                        ) {
                            check_and_push_diagnostic(
                                checker,
                                DiagnosticKind::from(SuspiciousPycryptoImport),
                                identifier.range(),
                            );
                        }
                    }
                }
                "Crypto.Cipher" | "Crypto.Hash" | "Crypto.IO" | "Crypto.Protocol"
                | "Crypto.PublicKey" | "Crypto.Random" | "Crypto.Signature" | "Crypto.Util" => {
                    check_and_push_diagnostic(
                        checker,
                        DiagnosticKind::from(SuspiciousPycryptoImport),
                        identifier.range(),
                    );
                }
                "pyghmi" => check_and_push_diagnostic(
                    checker,
                    DiagnosticKind::from(SuspiciousPyghmiImport),
                    identifier.range(),
                ),
                _ => {}
            }
        }
        _ => panic!("Expected Stmt::Import | Stmt::ImportFrom"),
    }
}

fn check_and_push_diagnostic(checker: &Checker, diagnostic_kind: DiagnosticKind, range: TextRange) {
    let diagnostic = Diagnostic::new::<DiagnosticKind>(diagnostic_kind, range);
    if checker.enabled(diagnostic.kind.rule()) {
        checker.report_diagnostic(diagnostic);
    }
}
