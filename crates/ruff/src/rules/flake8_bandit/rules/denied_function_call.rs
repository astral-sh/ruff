//! Check for calls to suspicious functions, or calls into suspicious modules.
//!
//! See: <https://bandit.readthedocs.io/en/latest/blacklists/blacklist_calls.html>
use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::rules::flake8_bandit::settings::Severity;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Reason {
    Pickle,
    Marshal,
    InsecureHash,
    InsecureCipher,
    Mktemp,
    Eval,
    MarkSafe,
    URLOpen,
    NonCryptographicRandom,
    UntrustedXML,
    UnverifiedSSL,
    Telnet,
    FTPLib,
}

#[violation]
pub struct DeniedFunctionCall {
    pub reason: Reason,
}

impl Violation for DeniedFunctionCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DeniedFunctionCall { reason } = self;
        match reason {
            Reason::Pickle => format!("`pickle` and modules that wrap it can be unsafe when used to deserialize untrusted data, possible security issue"),
            Reason::Marshal => format!("Deserialization with the `marshal` module is possibly dangerous"),
            Reason::InsecureHash => format!("Use of insecure MD2, MD4, MD5, or SHA1 hash function"),
            Reason::InsecureCipher => format!("Use of insecure cipher or cipher mode, replace with a known secure cipher such as AES"),
            Reason::Mktemp => format!("Use of insecure and deprecated function (`mktemp`)"),
            Reason::Eval => format!("Use of possibly insecure function; consider using `ast.literal_eval`"),
            Reason::MarkSafe => format!("Use of `mark_safe` may expose cross-site scripting vulnerabilities"),
            Reason::URLOpen => format!("Audit URL open for permitted schemes. Allowing use of `file:` or custom schemes is often unexpected."),
            Reason::NonCryptographicRandom => format!("Standard pseudo-random generators are not suitable for cryptographic purposes"),
            Reason::UntrustedXML => format!("Using various XLM methods to parse untrusted XML data is known to be vulnerable to XML attacks; use `defusedxml` equivalents"),
            Reason::UnverifiedSSL => format!("Python allows using an insecure context via the `_create_unverified_context` that reverts to the previous behavior that does not validate certificates or perform hostname checks"),
            Reason::Telnet => format!("Telnet-related functions are being called. Telnet is considered insecure. Use SSH or some other encrypted protocol"),
            Reason::FTPLib => format!("FTP-related functions are being called. FTP is considered insecure. Use SSH/SFTP/SCP or some other encrypted protocol"),
        }
    }
}

struct SuspiciousMembers<'a> {
    members: &'a [&'a [&'a str]],
    reason: Reason,
    severity: Severity,
}

impl<'a> SuspiciousMembers<'a> {
    pub const fn new(members: &'a [&'a [&'a str]], reason: Reason, severity: Severity) -> Self {
        Self {
            members,
            reason,
            severity,
        }
    }
}

struct SuspiciousModule<'a> {
    name: &'a str,
    reason: Reason,
    severity: Severity,
}

impl<'a> SuspiciousModule<'a> {
    pub const fn new(name: &'a str, reason: Reason, severity: Severity) -> Self {
        Self {
            name,
            reason,
            severity,
        }
    }
}

const SUSPICIOUS_MEMBERS: &[SuspiciousMembers] = &[
    SuspiciousMembers::new(
        &[
            &["pickle", "loads"],
            &["pickle", "load"],
            &["pickle", "Unpickler"],
            &["dill", "loads"],
            &["dill", "load"],
            &["dill", "Unpickler"],
            &["shelve", "open"],
            &["shelve", "DbfilenameShelf"],
            &["jsonpickle", "decode"],
            &["jsonpickle", "unpickler", "decode"],
            &["pandas", "read_pickle"],
        ],
        Reason::Pickle,
        Severity::Medium,
    ),
    SuspiciousMembers::new(
        &[&["marshal", "loads"], &["marshal", "load"]],
        Reason::Marshal,
        Severity::Medium,
    ),
    SuspiciousMembers::new(
        &[
            &["hashlib", "md5"],
            &["hashlib", "sha1"],
            &["Crypto", "Hash", "MD5", "new"],
            &["Crypto", "Hash", "MD4", "new"],
            &["Crypto", "Hash", "MD3", "new"],
            &["Crypto", "Hash", "MD2", "new"],
            &["Crypto", "Hash", "SHA", "new"],
            &["Cryptodome", "Hash", "MD5", "new"],
            &["Cryptodome", "Hash", "MD4", "new"],
            &["Cryptodome", "Hash", "MD3", "new"],
            &["Cryptodome", "Hash", "MD2", "new"],
            &["Cryptodome", "Hash", "SHA", "new"],
            &["cryptography", "hazmat", "primitives", "hashes", "MD5"],
            &["cryptography", "hazmat", "primitives", "hashes", "SHA1"],
        ],
        Reason::InsecureHash,
        Severity::Medium,
    ),
    SuspiciousMembers::new(
        &[
            &["Crypto", "Cipher", "ARC2", "new"],
            &["Crypto", "Cipher", "ARC2", "new"],
            &["Crypto", "Cipher", "Blowfish", "new"],
            &["Crypto", "Cipher", "DES", "new"],
            &["Crypto", "Cipher", "XOR", "new"],
            &["Cryptodome", "Cipher", "ARC2", "new"],
            &["Cryptodome", "Cipher", "ARC2", "new"],
            &["Cryptodome", "Cipher", "Blowfish", "new"],
            &["Cryptodome", "Cipher", "DES", "new"],
            &["Cryptodome", "Cipher", "XOR", "new"],
            &[
                "cryptography",
                "hazmat",
                "primitives",
                "ciphers",
                "algorithms",
                "ARC4",
            ],
            &[
                "cryptography",
                "hazmat",
                "primitives",
                "ciphers",
                "algorithms",
                "Blowfish",
            ],
            &[
                "cryptography",
                "hazmat",
                "primitives",
                "ciphers",
                "algorithms",
                "IDEA",
            ],
            &[
                "cryptography",
                "hazmat",
                "primitives",
                "ciphers",
                "modes",
                "ECB",
            ],
        ],
        Reason::InsecureCipher,
        Severity::High,
    ),
    SuspiciousMembers::new(&[&["tempfile", "mktemp"]], Reason::Mktemp, Severity::Medium),
    SuspiciousMembers::new(&[&["eval"]], Reason::Eval, Severity::Medium),
    SuspiciousMembers::new(
        &[&["django", "utils", "safestring", "mark_safe"]],
        Reason::MarkSafe,
        Severity::Medium,
    ),
    SuspiciousMembers::new(
        &[
            &["urllib", "urlopen"],
            &["urllib", "request", "urlopen"],
            &["urllib", "urlretrieve"],
            &["urllib", "request", "urlretrieve"],
            &["urllib", "URLopener"],
            &["urllib", "request", "URLopener"],
            &["urllib", "FancyURLopener"],
            &["urllib", "request", "FancyURLopener"],
            &["urllib2", "urlopen"],
            &["urllib2", "Request"],
            &["six", "moves", "urllib", "request", "urlopen"],
            &["six", "moves", "urllib", "request", "urlretrieve"],
            &["six", "moves", "urllib", "request", "URLopener"],
            &["six", "moves", "urllib", "request", "FancyURLopener"],
        ],
        Reason::URLOpen,
        Severity::Medium,
    ),
    SuspiciousMembers::new(
        &[
            &["random", "random"],
            &["random", "randrange"],
            &["random", "randint"],
            &["random", "choice"],
            &["random", "choices"],
            &["random", "uniform"],
            &["random", "triangular"],
        ],
        Reason::NonCryptographicRandom,
        Severity::Low,
    ),
    SuspiciousMembers::new(
        &[
            &["xml", "etree", "cElementTree", "parse"],
            &["xml", "etree", "cElementTree", "iterparse"],
            &["xml", "etree", "cElementTree", "fromstring"],
            &["xml", "etree", "cElementTree", "XMLParser"],
            &["xml", "etree", "ElementTree", "parse"],
            &["xml", "etree", "ElementTree", "iterparse"],
            &["xml", "etree", "ElementTree", "fromstring"],
            &["xml", "etree", "ElementTree", "XMLParser"],
            &["xml", "sax", "expatreader", "create_parser"],
            &["xml", "dom", "expatbuilder", "parse"],
            &["xml", "dom", "expatbuilder", "parseString"],
            &["xml", "sax", "parse"],
            &["xml", "sax", "parseString"],
            &["xml", "sax", "make_parser"],
            &["xml", "dom", "minidom", "parse"],
            &["xml", "dom", "minidom", "parseString"],
            &["xml", "dom", "pulldom", "parse"],
            &["xml", "dom", "pulldom", "parseString"],
            &["lxml", "etree", "parse"],
            &["lxml", "etree", "fromstring"],
            &["lxml", "etree", "RestrictedElement"],
            &["lxml", "etree", "GlobalParserTLS"],
            &["lxml", "etree", "getDefaultParser"],
            &["lxml", "etree", "check_docinfo"],
        ],
        Reason::UntrustedXML,
        Severity::High,
    ),
    SuspiciousMembers::new(
        &[&["ssl", "_create_unverified_context"]],
        Reason::UnverifiedSSL,
        Severity::Medium,
    ),
];

const SUSPICIOUS_MODULES: &[SuspiciousModule] = &[
    SuspiciousModule::new("telnetlib", Reason::Telnet, Severity::High),
    SuspiciousModule::new("ftplib", Reason::FTPLib, Severity::High),
];

/// S001
pub fn denied_function_call(checker: &mut Checker, expr: &Expr) {
    let ExprKind::Call { func, .. } = &expr.node else {
        return;
    };

    let Some(reason) = checker.ctx.resolve_call_path(func).and_then(|call_path| {
        for module in SUSPICIOUS_MEMBERS {
            if module.severity >= checker.settings.flake8_bandit.severity {
                for member in module.members {
                    if call_path.as_slice() == *member {
                        return Some(module.reason);
                    }
                }
            }
        }
        for module in SUSPICIOUS_MODULES {
            if module.severity >= checker.settings.flake8_bandit.severity {
                if call_path.first() == Some(&module.name) {
                    return Some(module.reason);
                }
            }
        }
        None
    }) else {
        return;
    };

    let issue = DeniedFunctionCall { reason };
    checker
        .diagnostics
        .push(Diagnostic::new(issue, Range::from(expr)));
}
