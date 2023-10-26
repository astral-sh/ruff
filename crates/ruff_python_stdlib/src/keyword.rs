static KEYWORDS: phf::Set<&'static str> = phf::phf_set! {
    "False",
    "None",
    "True",
    "and",
    "as",
    "assert",
    "async",
    "await",
    "break",
    "class",
    "continue",
    "def",
    "del",
    "elif",
    "else",
    "except",
    "finally",
    "for",
    "from",
    "global",
    "if",
    "import",
    "in",
    "is",
    "lambda",
    "nonlocal",
    "not",
    "or",
    "pass",
    "raise",
    "return",
    "try",
    "while",
    "with",
    "yield"
};

// See: https://github.com/python/cpython/blob/9d692841691590c25e6cf5b2250a594d3bf54825/Lib/keyword.py#L18
pub(crate) fn is_keyword(name: &str) -> bool {
    KEYWORDS.contains(name)
}
