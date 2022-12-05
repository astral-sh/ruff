use crate::isort::types::{AliasData, CommentSet, ImportFromData, Importable};

// Hard-code four-space indentation for the imports themselves, to match Black.
const INDENT: &str = "    ";

// Guess a capacity to use for string allocation.
const CAPACITY: usize = 200;

/// Add a plain import statement to the `RopeBuilder`.
pub fn format_import(alias: &AliasData, comments: &CommentSet, is_first: bool) -> String {
    let mut output = String::with_capacity(CAPACITY);
    if !is_first && !comments.atop.is_empty() {
        output.push('\n');
    }
    for comment in &comments.atop {
        output.push_str(comment);
        output.push('\n');
    }
    if let Some(asname) = alias.asname {
        output.push_str("import ");
        output.push_str(alias.name);
        output.push_str(" as ");
        output.push_str(asname);
    } else {
        output.push_str("import ");
        output.push_str(alias.name);
    }
    for comment in &comments.inline {
        output.push_str("  ");
        output.push_str(comment);
    }
    output.push('\n');
    output
}

/// Add an import-from statement to the `RopeBuilder`.
pub fn format_import_from(
    import_from: &ImportFromData,
    comments: &CommentSet,
    aliases: &[(AliasData, CommentSet)],
    line_length: usize,
    force_wrap_aliases: bool,
    is_first: bool,
) -> String {
    if aliases.len() == 1
        && aliases
            .iter()
            .all(|(alias, _)| alias.name == "*" && alias.asname.is_none())
    {
        let (single_line, ..) = format_single_line(import_from, comments, aliases, is_first);
        return single_line;
    }

    // We can only inline if: (1) none of the aliases have atop comments, and (3)
    // only the last alias (if any) has inline comments.
    if aliases
        .iter()
        .all(|(_, CommentSet { atop, .. })| atop.is_empty())
        && aliases
            .iter()
            .rev()
            .skip(1)
            .all(|(_, CommentSet { inline, .. })| inline.is_empty())
        && (!force_wrap_aliases
            || aliases.len() == 1
            || aliases.iter().all(|(alias, _)| alias.asname.is_none()))
    {
        let (single_line, import_length) =
            format_single_line(import_from, comments, aliases, is_first);
        if import_length <= line_length || aliases.iter().any(|(alias, _)| alias.name == "*") {
            return single_line;
        }
    }

    format_multi_line(import_from, comments, aliases, is_first)
}

/// Format an import-from statement in single-line format.
///
/// This method assumes that the output source code is syntactically valid.
fn format_single_line(
    import_from: &ImportFromData,
    comments: &CommentSet,
    aliases: &[(AliasData, CommentSet)],
    is_first: bool,
) -> (String, usize) {
    let mut output = String::with_capacity(CAPACITY);
    let mut line_length = 0;

    if !is_first && !comments.atop.is_empty() {
        output.push('\n');
    }
    for comment in &comments.atop {
        output.push_str(comment);
        output.push('\n');
    }

    let module_name = import_from.module_name();
    output.push_str("from ");
    output.push_str(&module_name);
    output.push_str(" import ");
    line_length += 5 + module_name.len() + 8;

    for (index, (AliasData { name, asname }, comments)) in aliases.iter().enumerate() {
        if let Some(asname) = asname {
            output.push_str(name);
            output.push_str(" as ");
            output.push_str(asname);
            line_length += name.len() + 4 + asname.len();
        } else {
            output.push_str(name);
            line_length += name.len();
        }
        if index < aliases.len() - 1 {
            output.push_str(", ");
            line_length += 2;
        }

        for comment in &comments.inline {
            output.push(' ');
            output.push(' ');
            output.push_str(comment);
            line_length += 2 + comment.len();
        }
    }

    for comment in &comments.inline {
        output.push(' ');
        output.push(' ');
        output.push_str(comment);
        line_length += 2 + comment.len();
    }

    output.push('\n');

    (output, line_length)
}

/// Format an import-from statement in multi-line format.
fn format_multi_line(
    import_from: &ImportFromData,
    comments: &CommentSet,
    aliases: &[(AliasData, CommentSet)],
    is_first: bool,
) -> String {
    let mut output = String::with_capacity(CAPACITY);

    if !is_first && !comments.atop.is_empty() {
        output.push('\n');
    }
    for comment in &comments.atop {
        output.push_str(comment);
        output.push('\n');
    }

    output.push_str("from ");
    output.push_str(&import_from.module_name());
    output.push_str(" import ");
    output.push('(');
    for comment in &comments.inline {
        output.push(' ');
        output.push(' ');
        output.push_str(comment);
    }
    output.push('\n');

    for (AliasData { name, asname }, comments) in aliases {
        for comment in &comments.atop {
            output.push_str(INDENT);
            output.push_str(comment);
            output.push('\n');
        }
        output.push_str(INDENT);
        if let Some(asname) = asname {
            output.push_str(name);
            output.push_str(" as ");
            output.push_str(asname);
        } else {
            output.push_str(name);
        }
        output.push(',');

        for comment in &comments.inline {
            output.push(' ');
            output.push(' ');
            output.push_str(comment);
        }
        output.push('\n');
    }

    output.push(')');
    output.push('\n');

    output
}
