//! Check for calls to suspicious functions, or calls into suspicious modules.
//!
//! See: <https://bandit.readthedocs.io/en/latest/blacklists/blacklist_calls.html>
use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, DiagnosticKind, Violation};
use ruff_macros::{derive_message_formats, violation};

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

/// S001
pub(crate) fn suspicious_function_call(checker: &mut Checker, expr: &Expr) {
    let Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return;
    };

    let Some(diagnostic_kind) = checker.semantic().resolve_call_path(func).and_then(|call_path| {
        match call_path.as_slice() {
            // Pickle
            ["pickle" | "dill", "load" | "loads" | "Unpickler"] |
            ["shelve", "open" | "DbfilenameShelf"] |
            ["jsonpickle", "decode"] |
            ["jsonpickle", "unpickler", "decode"] |
            ["pandas", "read_pickle"]  => Some(SuspiciousPickleUsage.into()),
            // Marshal
            ["marshal", "load" | "loads"] => Some(SuspiciousMarshalUsage.into()),
            // InsecureHash
            ["Crypto" | "Cryptodome", "Hash", "SHA" | "MD2" | "MD3" | "MD4" | "MD5", "new"] |
            ["cryptography", "hazmat", "primitives", "hashes", "SHA1" | "MD5"] => Some(SuspiciousInsecureHashUsage.into()),
            // InsecureCipher
            ["Crypto" | "Cryptodome", "Cipher", "ARC2" | "Blowfish"  | "DES" | "XOR", "new"] |
            ["cryptography", "hazmat", "primitives", "ciphers", "algorithms", "ARC4" | "Blowfish" | "IDEA" ] => Some(SuspiciousInsecureCipherUsage.into()),
            // InsecureCipherMode
            ["cryptography", "hazmat", "primitives", "ciphers", "modes", "ECB"] => Some(SuspiciousInsecureCipherModeUsage.into()),
            // Mktemp
            ["tempfile", "mktemp"] => Some(SuspiciousMktempUsage.into()),
            // Eval
            ["eval"] => Some(SuspiciousEvalUsage.into()),
            // MarkSafe
            ["django", "utils", "safestring", "mark_safe"] => Some(SuspiciousMarkSafeUsage.into()),
            // URLOpen
            ["urllib", "urlopen" | "urlretrieve" | "URLopener" | "FancyURLopener" | "Request"] |
            ["urllib", "request", "urlopen" | "urlretrieve" | "URLopener" | "FancyURLopener"] |
            ["six", "moves", "urllib", "request", "urlopen" | "urlretrieve" | "URLopener" | "FancyURLopener"] => Some(SuspiciousURLOpenUsage.into()),
            // NonCryptographicRandom
            ["random", "random" | "randrange" | "randint" | "choice" | "choices" | "uniform" | "triangular"] => Some(SuspiciousNonCryptographicRandomUsage.into()),
            // UnverifiedContext
            ["ssl", "_create_unverified_context"] => Some(SuspiciousUnverifiedContextUsage.into()),
            // XMLCElementTree
            ["xml", "etree", "cElementTree", "parse" | "iterparse" | "fromstring" | "XMLParser"] => Some(SuspiciousXMLCElementTreeUsage.into()),
            // XMLElementTree
            ["xml", "etree", "ElementTree", "parse" | "iterparse" | "fromstring" | "XMLParser"] => Some(SuspiciousXMLElementTreeUsage.into()),
            // XMLExpatReader
            ["xml", "sax", "expatreader", "create_parser"] => Some(SuspiciousXMLExpatReaderUsage.into()),
            // XMLExpatBuilder
            ["xml", "dom", "expatbuilder", "parse" | "parseString"] => Some(SuspiciousXMLExpatBuilderUsage.into()),
            // XMLSax
            ["xml", "sax", "parse" | "parseString" | "make_parser"] => Some(SuspiciousXMLSaxUsage.into()),
            // XMLMiniDOM
            ["xml", "dom", "minidom", "parse" | "parseString"] => Some(SuspiciousXMLMiniDOMUsage.into()),
            // XMLPullDOM
            ["xml", "dom", "pulldom", "parse" | "parseString"] => Some(SuspiciousXMLPullDOMUsage.into()),
            // XMLETree
            ["lxml", "etree", "parse" | "fromstring" | "RestrictedElement" | "GlobalParserTLS" | "getDefaultParser" | "check_docinfo"] => Some(SuspiciousXMLETreeUsage.into()),
            // Telnet
            ["telnetlib", ..] => Some(SuspiciousTelnetUsage.into()),
            // FTPLib
            ["ftplib", ..] => Some(SuspiciousFTPLibUsage.into()),
            _ => None
        }
    }) else {
        return;
    };

    let diagnostic = Diagnostic::new::<DiagnosticKind>(diagnostic_kind, expr.range());
    if checker.enabled(diagnostic.kind.rule()) {
        checker.diagnostics.push(diagnostic);
    }
}
