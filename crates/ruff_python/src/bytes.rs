/// See: <https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals>
pub const TRIPLE_QUOTE_PREFIXES: &[&str] = &[
    "br'''", "rb'''", "bR'''", "Rb'''", "Br'''", "rB'''", "RB'''", "BR'''", "b'''", "br\"\"\"",
    "rb\"\"\"", "bR\"\"\"", "Rb\"\"\"", "Br\"\"\"", "rB\"\"\"", "RB\"\"\"", "BR\"\"\"", "b\"\"\"",
    "B\"\"\"",
];
pub const SINGLE_QUOTE_PREFIXES: &[&str] = &[
    "br'", "rb'", "bR'", "Rb'", "Br'", "rB'", "RB'", "BR'", "b'", "br\"", "rb\"", "bR\"", "Rb\"",
    "Br\"", "rB\"", "RB\"", "BR\"", "b\"", "B\"",
];
