/// See: <https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals>

pub const TRIPLE_QUOTE_PREFIXES: &[&str] = &[
    "u\"\"\"", "u'''", "r\"\"\"", "r'''", "U\"\"\"", "U'''", "R\"\"\"", "R'''", "\"\"\"", "'''",
];

pub const SINGLE_QUOTE_PREFIXES: &[&str] = &[
    "u\"", "u'", "r\"", "r'", "u\"", "u'", "r\"", "r'", "U\"", "U'", "R\"", "R'", "\"", "'",
];

pub const TRIPLE_QUOTE_SUFFIXES: &[&str] = &["\"\"\"", "'''"];

pub const SINGLE_QUOTE_SUFFIXES: &[&str] = &["\"", "'"];
