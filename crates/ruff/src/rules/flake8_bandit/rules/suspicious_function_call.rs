//! Check for calls to suspicious functions, or calls into suspicious modules.
//!
//! See: <https://bandit.readthedocs.io/en/latest/blacklists/blacklist_calls.html>
use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, DiagnosticKind, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct SuspiciousPickleUsage;

impl Violation for SuspiciousPickleUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`pickle` and modules that wrap it can be unsafe when used to deserialize untrusted data, possible security issue")
    }
}

#[violation]
pub struct SuspiciousMarshalUsage;

impl Violation for SuspiciousMarshalUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Deserialization with the `marshal` module is possibly dangerous")
    }
}

#[violation]
pub struct SuspiciousInsecureHashUsage;

impl Violation for SuspiciousInsecureHashUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of insecure MD2, MD4, MD5, or SHA1 hash function")
    }
}

#[violation]
pub struct SuspiciousInsecureCipherUsage;

impl Violation for SuspiciousInsecureCipherUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of insecure cipher, replace with a known secure cipher such as AES")
    }
}

#[violation]
pub struct SuspiciousInsecureCipherModeUsage;

impl Violation for SuspiciousInsecureCipherModeUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of insecure cipher mode, replace with a known secure cipher such as AES")
    }
}

#[violation]
pub struct SuspiciousMktempUsage;

impl Violation for SuspiciousMktempUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of insecure and deprecated function (`mktemp`)")
    }
}

#[violation]
pub struct SuspiciousEvalUsage;

impl Violation for SuspiciousEvalUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of possibly insecure function; consider using `ast.literal_eval`")
    }
}

#[violation]
pub struct SuspiciousMarkSafeUsage;

impl Violation for SuspiciousMarkSafeUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `mark_safe` may expose cross-site scripting vulnerabilities")
    }
}

#[violation]
pub struct SuspiciousURLOpenUsage;

impl Violation for SuspiciousURLOpenUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Audit URL open for permitted schemes. Allowing use of `file:` or custom schemes is often unexpected.")
    }
}

#[violation]
pub struct SuspiciousNonCryptographicRandomUsage;

impl Violation for SuspiciousNonCryptographicRandomUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Standard pseudo-random generators are not suitable for cryptographic purposes")
    }
}

#[violation]
pub struct SuspiciousXMLCElementTreeUsage;

impl Violation for SuspiciousXMLCElementTreeUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using `xml` to parse untrusted data is known to be vulnerable to XML attacks; use `defusedxml` equivalents")
    }
}

#[violation]
pub struct SuspiciousXMLElementTreeUsage;

impl Violation for SuspiciousXMLElementTreeUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using `xml` to parse untrusted data is known to be vulnerable to XML attacks; use `defusedxml` equivalents")
    }
}

#[violation]
pub struct SuspiciousXMLExpatReaderUsage;

impl Violation for SuspiciousXMLExpatReaderUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using `xml` to parse untrusted data is known to be vulnerable to XML attacks; use `defusedxml` equivalents")
    }
}

#[violation]
pub struct SuspiciousXMLExpatBuilderUsage;

impl Violation for SuspiciousXMLExpatBuilderUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using `xml` to parse untrusted data is known to be vulnerable to XML attacks; use `defusedxml` equivalents")
    }
}

#[violation]
pub struct SuspiciousXMLSaxUsage;

impl Violation for SuspiciousXMLSaxUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using `xml` to parse untrusted data is known to be vulnerable to XML attacks; use `defusedxml` equivalents")
    }
}

#[violation]
pub struct SuspiciousXMLMiniDOMUsage;

impl Violation for SuspiciousXMLMiniDOMUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using `xml` to parse untrusted data is known to be vulnerable to XML attacks; use `defusedxml` equivalents")
    }
}

#[violation]
pub struct SuspiciousXMLPullDOMUsage;

impl Violation for SuspiciousXMLPullDOMUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using `xml` to parse untrusted data is known to be vulnerable to XML attacks; use `defusedxml` equivalents")
    }
}

#[violation]
pub struct SuspiciousXMLETreeUsage;

impl Violation for SuspiciousXMLETreeUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using `xml` to parse untrusted data is known to be vulnerable to XML attacks; use `defusedxml` equivalents")
    }
}

#[violation]
pub struct SuspiciousUnverifiedContextUsage;

impl Violation for SuspiciousUnverifiedContextUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Python allows using an insecure context via the `_create_unverified_context` that reverts to the previous behavior that does not validate certificates or perform hostname checks.")
    }
}

#[violation]
pub struct SuspiciousTelnetUsage;

impl Violation for SuspiciousTelnetUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Telnet-related functions are being called. Telnet is considered insecure. Use SSH or some other encrypted protocol.")
    }
}

#[violation]
pub struct SuspiciousFTPLibUsage;

impl Violation for SuspiciousFTPLibUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("FTP-related functions are being called. FTP is considered insecure. Use SSH/SFTP/SCP or some other encrypted protocol.")
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Reason {
    Pickle,
    Marshal,
    InsecureHash,
    InsecureCipher,
    InsecureCipherMode,
    Mktemp,
    Eval,
    MarkSafe,
    URLOpen,
    NonCryptographicRandom,
    XMLCElementTree,
    XMLElementTree,
    XMLExpatReader,
    XMLExpatBuilder,
    XMLSax,
    XMLMiniDOM,
    XMLPullDOM,
    XMLETree,
    UnverifiedContext,
    Telnet,
    FTPLib,
}

struct SuspiciousMembers<'a> {
    members: &'a [&'a [&'a str]],
    reason: Reason,
}

impl<'a> SuspiciousMembers<'a> {
    pub const fn new(members: &'a [&'a [&'a str]], reason: Reason) -> Self {
        Self { members, reason }
    }
}

struct SuspiciousModule<'a> {
    name: &'a str,
    reason: Reason,
}

impl<'a> SuspiciousModule<'a> {
    pub const fn new(name: &'a str, reason: Reason) -> Self {
        Self { name, reason }
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
    ),
    SuspiciousMembers::new(
        &[&["marshal", "loads"], &["marshal", "load"]],
        Reason::Marshal,
    ),
    SuspiciousMembers::new(
        &[
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
        ],
        Reason::InsecureCipher,
    ),
    SuspiciousMembers::new(
        &[&[
            "cryptography",
            "hazmat",
            "primitives",
            "ciphers",
            "modes",
            "ECB",
        ]],
        Reason::InsecureCipherMode,
    ),
    SuspiciousMembers::new(&[&["tempfile", "mktemp"]], Reason::Mktemp),
    SuspiciousMembers::new(&[&["eval"]], Reason::Eval),
    SuspiciousMembers::new(
        &[&["django", "utils", "safestring", "mark_safe"]],
        Reason::MarkSafe,
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
    ),
    SuspiciousMembers::new(
        &[&["ssl", "_create_unverified_context"]],
        Reason::UnverifiedContext,
    ),
    SuspiciousMembers::new(
        &[
            &["xml", "etree", "cElementTree", "parse"],
            &["xml", "etree", "cElementTree", "iterparse"],
            &["xml", "etree", "cElementTree", "fromstring"],
            &["xml", "etree", "cElementTree", "XMLParser"],
        ],
        Reason::XMLCElementTree,
    ),
    SuspiciousMembers::new(
        &[
            &["xml", "etree", "ElementTree", "parse"],
            &["xml", "etree", "ElementTree", "iterparse"],
            &["xml", "etree", "ElementTree", "fromstring"],
            &["xml", "etree", "ElementTree", "XMLParser"],
        ],
        Reason::XMLElementTree,
    ),
    SuspiciousMembers::new(
        &[&["xml", "sax", "expatreader", "create_parser"]],
        Reason::XMLExpatReader,
    ),
    SuspiciousMembers::new(
        &[
            &["xml", "dom", "expatbuilder", "parse"],
            &["xml", "dom", "expatbuilder", "parseString"],
        ],
        Reason::XMLExpatBuilder,
    ),
    SuspiciousMembers::new(
        &[
            &["xml", "sax", "parse"],
            &["xml", "sax", "parseString"],
            &["xml", "sax", "make_parser"],
        ],
        Reason::XMLSax,
    ),
    SuspiciousMembers::new(
        &[
            &["xml", "dom", "minidom", "parse"],
            &["xml", "dom", "minidom", "parseString"],
        ],
        Reason::XMLMiniDOM,
    ),
    SuspiciousMembers::new(
        &[
            &["xml", "dom", "pulldom", "parse"],
            &["xml", "dom", "pulldom", "parseString"],
        ],
        Reason::XMLPullDOM,
    ),
    SuspiciousMembers::new(
        &[
            &["lxml", "etree", "parse"],
            &["lxml", "etree", "fromstring"],
            &["lxml", "etree", "RestrictedElement"],
            &["lxml", "etree", "GlobalParserTLS"],
            &["lxml", "etree", "getDefaultParser"],
            &["lxml", "etree", "check_docinfo"],
        ],
        Reason::XMLETree,
    ),
];

const SUSPICIOUS_MODULES: &[SuspiciousModule] = &[
    SuspiciousModule::new("telnetlib", Reason::Telnet),
    SuspiciousModule::new("ftplib", Reason::FTPLib),
];

/// S001
pub fn suspicious_function_call(checker: &mut Checker, expr: &Expr) {
    let ExprKind::Call { func, .. } = &expr.node else {
        return;
    };

    let Some(reason) = checker.ctx.resolve_call_path(func).and_then(|call_path| {
        for module in SUSPICIOUS_MEMBERS {
            for member in module.members {
                if call_path.as_slice() == *member {
                    return Some(module.reason);
                }
            }
        }
        for module in SUSPICIOUS_MODULES {
            if call_path.first() == Some(&module.name) {
                return Some(module.reason);
            }
        }
        None
    }) else {
        return;
    };

    let diagnostic_kind = match reason {
        Reason::Pickle => SuspiciousPickleUsage.into(),
        Reason::Marshal => SuspiciousMarshalUsage.into(),
        Reason::InsecureHash => SuspiciousInsecureHashUsage.into(),
        Reason::InsecureCipher => SuspiciousInsecureCipherUsage.into(),
        Reason::InsecureCipherMode => SuspiciousInsecureCipherModeUsage.into(),
        Reason::Mktemp => SuspiciousMktempUsage.into(),
        Reason::Eval => SuspiciousEvalUsage.into(),
        Reason::MarkSafe => SuspiciousMarkSafeUsage.into(),
        Reason::URLOpen => SuspiciousURLOpenUsage.into(),
        Reason::NonCryptographicRandom => SuspiciousNonCryptographicRandomUsage.into(),
        Reason::XMLCElementTree => SuspiciousXMLCElementTreeUsage.into(),
        Reason::XMLElementTree => SuspiciousXMLElementTreeUsage.into(),
        Reason::XMLExpatReader => SuspiciousXMLExpatReaderUsage.into(),
        Reason::XMLExpatBuilder => SuspiciousXMLExpatBuilderUsage.into(),
        Reason::XMLSax => SuspiciousXMLSaxUsage.into(),
        Reason::XMLMiniDOM => SuspiciousXMLMiniDOMUsage.into(),
        Reason::XMLPullDOM => SuspiciousXMLPullDOMUsage.into(),
        Reason::XMLETree => SuspiciousXMLETreeUsage.into(),
        Reason::UnverifiedContext => SuspiciousUnverifiedContextUsage.into(),
        Reason::Telnet => SuspiciousTelnetUsage.into(),
        Reason::FTPLib => SuspiciousFTPLibUsage.into(),
    };
    let diagnostic = Diagnostic::new::<DiagnosticKind>(diagnostic_kind, Range::from(expr));
    if checker.settings.rules.enabled(diagnostic.kind.rule()) {
        checker.diagnostics.push(diagnostic);
    }
}
