//! Check for calls to suspicious functions, or calls into suspicious modules.
//!
//! See: <https://bandit.readthedocs.io/en/latest/blacklists/blacklist_calls.html>
use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, DiagnosticKind, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for calls to `pickle` functions or modules that wrap them.
///
/// ## Why is this bad?
/// Deserializing untrusted data with `pickle` and other deserialization
/// modules is insecure as it can allow for the creation of arbitrary objects,
/// which can then be used to achieve arbitrary code execution and otherwise
/// unexpected behavior.
///
/// Avoid deserializing untrusted data with `pickle` and other deserialization
/// modules. Instead, consider safer formats, such as JSON.
///
/// If you must deserialize untrusted data with `pickle`, consider signing the
/// data with a secret key and verifying the signature before deserializing
/// (such as with `hmac`). This will prevent an attacker from modifying the
/// serialized data to inject arbitrary objects.
///
/// ## Example
/// ```python
/// import pickle
///
/// with open("foo.pickle", "rb") as file:
///     foo = pickle.load(file)
/// ```
///
/// Use instead:
/// ```python
/// import json
///
/// with open("foo.json", "rb") as file:
///     foo = json.load(file)
/// ```
///
/// ## References
/// - [Python documentation: `pickle` — Python object serialization](https://docs.python.org/3/library/pickle.html)
/// - [Common Weakness Enumeration: CWE-502](https://cwe.mitre.org/data/definitions/502.html)
#[violation]
pub struct SuspiciousPickleUsage;

impl Violation for SuspiciousPickleUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`pickle` and modules that wrap it can be unsafe when used to deserialize untrusted data, possible security issue")
    }
}

/// ## What it does
/// Checks for calls to `marshal` functions.
///
/// ## Why is this bad?
/// Deserializing untrusted data with `marshal` is insecure as it can allow for
/// the creation of arbitrary objects, which can then be used to achieve
/// arbitrary code execution and otherwise unexpected behavior.
///
/// Avoid deserializing untrusted data with `marshal`. Instead, consider safer
/// formats, such as JSON.
///
/// If you must deserialize untrusted data with `marshal`, consider signing the
/// data with a secret key and verifying the signature before deserializing
/// (such as with `hmac`). This will prevent an attacker from modifying the
/// serialized data to inject arbitrary objects.
///
/// ## Example
/// ```python
/// import marshal
///
/// with open("foo.marshal", "rb") as file:
///     foo = pickle.load(file)
/// ```
///
/// Use instead:
/// ```python
/// import json
///
/// with open("foo.json", "rb") as file:
///     foo = json.load(file)
/// ```
///
/// ## References
/// - [Python documentation: `marshal` — Internal Python object serialization](https://docs.python.org/3/library/marshal.html)
/// - [Common Weakness Enumeration: CWE-502](https://cwe.mitre.org/data/definitions/502.html)
#[violation]
pub struct SuspiciousMarshalUsage;

impl Violation for SuspiciousMarshalUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Deserialization with the `marshal` module is possibly dangerous")
    }
}

/// ## What it does
/// Checks for uses of weak or broken cryptographic hash functions.
///
/// ## Why is this bad?
/// Weak or broken cryptographic hash functions may be susceptible to
/// collision attacks (where two different inputs produce the same hash) or
/// prei-image attacks (where an attacker can find an input that produces a
/// given hash). This can lead to security vulnerabilities in applications
/// that rely on these hash functions.
///
/// Avoid using weak or broken cryptographic hash functions in security
/// contexts. Instead, use a known secure hash function such as SHA256.
///
/// ## Example
/// ```python
/// import hashlib
///
///
/// def certificate_is_valid(certificate: bytes, known_hash: str) -> bool:
///     hash = hashlib.md5(certificate).hexdigest()
///     return hash == known_hash
/// ```
///
/// Use instead:
/// ```python
/// import hashlib
///
///
/// def certificate_is_valid(certificate: bytes, known_hash: str) -> bool:
///     hash = hashlib.sha256(certificate).hexdigest()
///     return hash == known_hash
/// ```
///
/// ## References
/// - [Python documentation: `hashlib` — Secure hashes and message digests](https://docs.python.org/3/library/hashlib.html)
/// - [Common Weakness Enumeration: CWE-327](https://cwe.mitre.org/data/definitions/327.html)
/// - [Common Weakness Enumeration: CWE-328](https://cwe.mitre.org/data/definitions/328.html)
/// - [Common Weakness Enumeration: CWE-916](https://cwe.mitre.org/data/definitions/916.html)
#[violation]
pub struct SuspiciousInsecureHashUsage;

impl Violation for SuspiciousInsecureHashUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of insecure MD2, MD4, MD5, or SHA1 hash function")
    }
}

/// ## What it does
/// Checks for uses of weak or broken cryptographic ciphers.
///
/// ## Why is this bad?
/// Weak or broken cryptographic ciphers may be susceptible to attacks that
/// allow an attacker to decrypt ciphertext without knowing the key or
/// otherwise compromise the security of the cipher, such as forgeries.
///
/// Use strong, modern cryptographic ciphers instead of weak or broken ones.
///
/// ## Example
/// ```python
/// from cryptography.hazmat.primitives.ciphers import Cipher, algorithms
///
/// algorithm = algorithms.ARC4(key)
/// cipher = Cipher(algorithm, mode=None)
/// encryptor = cipher.encryptor()
/// ```
///
/// Use instead:
/// ```python
/// from cryptography.fernet import Fernet
///
/// fernet = Fernet(key)
/// ```
///
/// ## References
/// - [Common Weakness Enumeration: CWE-327](https://cwe.mitre.org/data/definitions/327.html)
#[violation]
pub struct SuspiciousInsecureCipherUsage;

impl Violation for SuspiciousInsecureCipherUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of insecure cipher, replace with a known secure cipher such as AES")
    }
}

/// ## What it does
/// Checks for uses of weak or broken cryptographic cipher modes.
///
/// ## Why is this bad?
/// Weak or broken cryptographic ciphers may be susceptible to attacks that
/// allow an attacker to decrypt ciphertext without knowing the key or
/// otherwise compromise the security of the cipher, such as forgeries.
///
/// Use strong, modern cryptographic ciphers instead of weak or broken ones.
///
/// ## Example
/// ```python
/// from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
///
/// algorithm = algorithms.ARC4(key)
/// cipher = Cipher(algorithm, mode=modes.ECB(iv))
/// encryptor = cipher.encryptor()
/// ```
///
/// Use instead:
/// ```python
/// from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
///
/// algorithm = algorithms.ARC4(key)
/// cipher = Cipher(algorithm, mode=modes.CTR(iv))
/// encryptor = cipher.encryptor()
/// ```
///
/// ## References
/// - [Common Weakness Enumeration: CWE-327](https://cwe.mitre.org/data/definitions/327.html)
#[violation]
pub struct SuspiciousInsecureCipherModeUsage;

impl Violation for SuspiciousInsecureCipherModeUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of insecure cipher mode, replace with a known secure cipher such as AES")
    }
}

/// ## What it does
/// Checks for uses of `tempfile.mktemp`.
///
/// ## Why is this bad?
/// `tempfile.mktemp` returns a pathname of a file that does not exist at the
/// time the call is made; then, the caller is responsible for creating the
/// file and subsequently using it. This is insecure because another process
/// could create a file with the same name between the time the function
/// returns and the time the caller creates the file.
///
/// `tempfile.mktemp` is deprecated in favor of `tempfile.mkstemp` which
/// creates the file when it is called. Consider using `tempfile.mkstemp`
/// instead, either directly or via a context manager such as
/// `tempfile.TemporaryFile`.
///
///
/// ## Example
/// ```python
/// import tempfile
///
/// tmp_file = tempfile.mktemp()
/// with open(tmp_file, "w") as file:
///     file.write("Hello, world!")
/// ```
///
/// Use instead:
/// ```python
/// import tempfile
///
/// with tempfile.TemporaryFile() as file:
///     file.write("Hello, world!")
/// ```
///
/// ## References
/// - [Python documentation:`mktemp`](https://docs.python.org/3/library/tempfile.html#tempfile.mktemp)
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

/// S301, S302, S303, S304, S305, S306, S307, S308, S310, S311, S312, S313, S314, S315, S316, S317, S318, S319, S320, S321, S323
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
            ["" | "builtins", "eval"] => Some(SuspiciousEvalUsage.into()),
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
