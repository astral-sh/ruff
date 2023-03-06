use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};
use smallvec::SmallVec;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::rules::flake8_bandit::helpers::Severity;
use crate::violation::Violation;

define_violation!(
    pub struct DenylistCall {
        pub message: String,
    }
);
impl Violation for DenylistCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("{}", self.message)
    }
}

struct DLCall<'a> {
    calls: &'a [&'a [&'a str]],
    message: &'a str,
    severity: Severity,
}

impl<'a> DLCall<'a> {
    pub const fn new(calls: &'a [&'a [&'a str]], message: &'a str, severity: Severity) -> Self {
        Self {
            calls,
            message,
            severity,
        }
    }
}

// List comes from: https://bandit.readthedocs.io/en/latest/blacklists/blacklist_calls.html
const DENYLIST_CALLS: &[DLCall] = &[
    DLCall::new(
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
        "Pickle and modules that wrap it can be unsafe when used to deserialize untrusted data, possible security issue",
        Severity::Medium
    ),
    DLCall::new(
        &[
            &["marshal", "loads"],
            &["marshal", "load"],
        ],
        "Deserialization with the marshal module is possibly dangerous",
        Severity::Medium
    ),
    DLCall::new(
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
        "Use of insecure MD2, MD4, MD5, or SHA1 hash function",
        Severity::Medium
    ),
    DLCall::new(
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
            &["cryptography", "hazmat", "primitives", "ciphers", "algorithms", "ARC4"],
            &["cryptography", "hazmat", "primitives", "ciphers", "algorithms", "Blowfish"],
            &["cryptography", "hazmat", "primitives", "ciphers", "algorithms", "IDEA"],
            &["cryptography", "hazmat", "primitives", "ciphers", "modes", "ECB"],
        ],
        "Use of insecure cipher or cipher mode, replace with a known secure cipher such as AES",
        Severity::High
    ),
    DLCall::new(&[&["tempfile", "mktemp"]], "Use of insecure and deprecated function (mktemp)", Severity::Medium),
    DLCall::new(&[&["eval"]], "Use of possibly insecure function - consider using safer ast.literal_eval", Severity::Medium),
    DLCall::new(&[&["django", "utils", "safestring", "mark_safe"]], "Use of mark_safe() may expose cross-site scripting vulnerabilities and should be reviewed.", Severity::Medium),
    DLCall::new(
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
        "Audit url open for permitted schemes. Allowing use of ‘file:’’ or custom schemes is often unexpected",
        Severity::Medium
    ),
    DLCall::new(
        &[
            &["random", "random"],
            &["random", "randrange"],
            &["random", "randint"],
            &["random", "choice"],
            &["random", "choices"],
            &["random", "uniform"],
            &["random", "triangular"]
        ],
        "Standard pseudo-random generators are not suitable for security/cryptographic purposes",
        Severity::Low
        ),
    DLCall::new(&[
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
        "Using various XLM methods to parse untrusted XML data is known to be vulnerable to XML attacks. Methods should be replaced with their defusedxml equivalents",
        Severity::High
    ),
    DLCall::new(&[&["ssl", "_create_unverified_context"]], "Python allows using an insecure context via the _create_unverified_context that reverts to the previous behavior that does not validate certificates or perform hostname checks", Severity::Medium)
];

fn comp_small_norm(small: &SmallVec<[&str; 8]>, norm: &&[&str]) -> bool {
    if small.len() != norm.len() {
        return false;
    }
    for (s, n) in small.iter().zip(norm.iter()) {
        if s != n {
            return false;
        }
    }
    true
}

/// S001
pub fn denylist_calls(checker: &mut Checker, expr: &Expr) {
    if let ExprKind::Call { func, .. } = &expr.node {
        if let Some(message) = checker.ctx.resolve_call_path(func).and_then(|call_path| {
            for bl_call in DENYLIST_CALLS {
                if bl_call.severity >= checker.settings.flake8_bandit.severity {
                    for path in bl_call.calls {
                        if comp_small_norm(&call_path, path) {
                            return Some(bl_call.message);
                        }
                    }
                }
            }
            if let Some(first_path) = call_path.first() {
                // Both of these are high, so I am not adding conidtions for severity
                if first_path == &"telnetlib" {
                    return Some("Telnet-related functions are being called. Telnet is considered insecure. Use SSH or some other encrypted protocol");
                } else if first_path == &"ftplib" {
                    return Some("FTP-related functions are being called. FTP is considered insecure. Use SSH/SFTP/SCP or some other encrypted protocol");
                }
            }
            None
        }) {
            let issue = DenylistCall {
                message: message.to_string(),
            };
            checker
                .diagnostics
                .push(Diagnostic::new(issue, Range::from_located(expr)));
        }
    }
}
