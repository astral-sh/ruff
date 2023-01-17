const KEYWORDS: [&str; 35] = [
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

/// This function currently does not work with python emoji synatx. For example "/N{dog}" will be
/// converted to "/N{{dog}}" which is NOT correct and WILL cause issues. Please check for emoji
/// syntax before using this command, and if it exists, issue a warning instead of attempting to
/// fix
pub fn curly_escape(string: &str) -> String {
    string.replace('{', "{{").replace('}', "}}")
}

/// Whether or not a given string is a python keyword
pub fn is_keyword(string: &str) -> bool {
    KEYWORDS.contains(&string)
}
