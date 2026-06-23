use std::iter::{Enumerate, Peekable};

use compact_str::{CompactString, ToCompactString};
use indexmap::IndexMap;
use ruff_python_trivia::leading_indentation;
use ruff_source_file::{Line as SourceLine, UniversalNewlineIterator, UniversalNewlines};
use ruff_text_size::{Ranged, TextRange, TextSize};

use super::preformatted::PreformattedBlockScanner;

/// Parses all reST field lists in a docstring.
fn field_lists(raw: &str) -> Vec<FieldList> {
    FieldList::parse_all(raw)
}

/// Returns top-level field lists that begin at a reST block boundary.
///
/// `source` must have already undergone PEP-257 trimming and universal newline
/// normalization (typically via `docstring::documentation_trim`).
pub(in crate::docstring) fn top_level_field_lists(
    source: &str,
) -> impl Iterator<Item = FieldList> + '_ {
    field_lists(source).into_iter().filter(|field_list| {
        field_list.indent == TextSize::default()
            && starts_at_block_boundary(source, field_list.range.start())
    })
}

/// Returns the parameter documentation recognized in a reST docstring.
pub(super) fn parameter_documentation(raw: &str) -> IndexMap<String, String> {
    let mut parameters = IndexMap::new();
    let source = crate::docstring::documentation_trim(raw);

    for field_list in top_level_field_lists(&source) {
        for field in field_list.fields {
            let Field::Parameter {
                lookup_name,
                description,
                ..
            } = field
            else {
                continue;
            };

            if description.is_empty() {
                continue;
            }

            parameters.insert(lookup_name, description);
        }
    }

    parameters
}

fn starts_at_block_boundary(source: &str, start: TextSize) -> bool {
    source.get(..start.to_usize()).is_some_and(|prefix| {
        prefix
            .lines()
            .next_back()
            .is_none_or(|line| line.trim().is_empty())
    })
}

/// Cursor over docstring lines and their line numbers.
#[derive(Clone)]
struct Lines<'a> {
    inner: Peekable<Enumerate<UniversalNewlineIterator<'a>>>,
}

impl<'a> Lines<'a> {
    /// Constructs a line cursor from raw docstring text.
    fn new(raw: &'a str) -> Self {
        Self {
            inner: raw.universal_newlines().enumerate().peekable(),
        }
    }

    /// Returns the next line without advancing the cursor.
    fn peek(&mut self) -> Option<DocstringLine<'a>> {
        let (index, line) = self.inner.peek()?;
        Some(DocstringLine::new(*index, line))
    }

    /// Advances the cursor and returns the next line.
    fn next(&mut self) -> Option<DocstringLine<'a>> {
        let (index, line) = self.inner.next()?;
        Some(DocstringLine::new(index, &line))
    }
}

/// A docstring line with its source position.
#[derive(Debug, Clone)]
struct DocstringLine<'a> {
    index: usize,
    text: &'a str,
    range: TextRange,
}

impl<'a> DocstringLine<'a> {
    fn new(index: usize, line: &SourceLine<'a>) -> Self {
        Self {
            index,
            text: line.as_str(),
            range: line.range(),
        }
    }
}

/// Represents a contiguous list of reST fields.
///
/// <https://www.sphinx-doc.org/en/master/usage/restructuredtext/basics.html#field-lists>
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::docstring) struct FieldList {
    start_line: usize,
    end_line: usize,
    range: TextRange,
    indent: TextSize,
    fields: Vec<Field>,
}

impl FieldList {
    pub(in crate::docstring) fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// Parse all the field lists in the given lines of a docstring.
    fn parse_all(raw: &str) -> Vec<Self> {
        let mut field_lists = Vec::new();
        let mut preformatted_blocks = PreformattedBlockScanner::default();
        let mut lines = Lines::new(raw);

        while let Some(line) = lines.peek() {
            if preformatted_blocks.consume_preformatted_line(line.text) {
                lines.next();
                continue;
            }

            let Some(field_list) = Self::parse(&mut lines) else {
                preformatted_blocks.observe_line_outside_preformatted_block(line.text);
                lines.next();
                continue;
            };

            field_lists.push(field_list);
        }

        field_lists
    }

    /// Attempt to parse a single field list from the given lines of a docstring.
    fn parse(lines: &mut Lines<'_>) -> Option<Self> {
        let line = lines.peek()?;
        let start_line = line.index;
        let range_start = line.range.start();
        let header = FieldHeader::parse_list_member(line.text)?;
        lines.next();

        let field_list_indent = header.indent;
        let mut fields = Vec::new();
        let mut current = FieldBuilder::new(header);
        let mut end_line = start_line + 1;
        let mut range_end = line.range.end();

        while let Some(line) = lines.peek() {
            if line.text.trim().is_empty() {
                // Blank lines continue the field list only before another field or a continuation.

                if !Self::blank_line_continues_field_list(lines, field_list_indent) {
                    break;
                }

                while let Some(blank_line) = lines.peek()
                    && blank_line.text.trim().is_empty()
                {
                    current.lines.push(blank_line.text);
                    lines.next();
                    end_line = blank_line.index + 1;
                    range_end = blank_line.range.end();
                }
                continue;
            }

            if let Some(header) = FieldHeader::at_indent(line.text, field_list_indent) {
                // Same-indent field header starts the next field in this list.

                let previous = std::mem::replace(&mut current, FieldBuilder::new(header));
                fields.push(previous.finish());
                lines.next();
                end_line = line.index + 1;
                range_end = line.range.end();
                continue;
            }

            if FieldHeader::indentation(line.text) <= field_list_indent {
                // Same- or less-indented content ends this field list.
                break;
            }

            // More-indented non-blank lines continue the current field body
            // (and hence also the current field list).
            current.lines.push(line.text);
            lines.next();
            end_line = line.index + 1;
            range_end = line.range.end();
        }

        // Finalize the last field.
        fields.push(current.finish());

        Some(Self {
            start_line,
            end_line,
            range: TextRange::new(range_start, range_end),
            indent: field_list_indent,
            fields,
        })
    }

    /// Returns whether a blank line keeps the current field list open.
    ///
    /// A blank line before an indented continuation stays in the current field list:
    ///
    /// ```rst
    /// :param x: First paragraph.
    ///
    ///     Second paragraph.
    /// :param y: Next parameter.
    /// ```
    ///
    /// A blank line before another same-indent field also stays in the current field list:
    ///
    /// ```rst
    /// :param x: First parameter.
    ///
    /// :param y: Second parameter.
    /// ```
    ///
    /// A blank line before same-indent prose ends the field list:
    ///
    /// ```rst
    /// :param x: First parameter.
    ///
    /// This is normal prose.
    /// ```
    fn blank_line_continues_field_list(lines: &Lines<'_>, indent: TextSize) -> bool {
        let mut next = lines.clone();
        while let Some(line) = next.peek()
            && line.text.trim().is_empty()
        {
            next.next();
        }

        let Some(non_blank_line) = next.peek() else {
            return false;
        };

        FieldHeader::indentation(non_blank_line.text) > indent
            || FieldHeader::at_indent(non_blank_line.text, indent).is_some()
    }
}

impl Ranged for FieldList {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Constructs new instances of the model for a reST field.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldBuilder<'a> {
    kind: FieldKind<'a>,
    body: &'a str,
    lines: Vec<&'a str>,
}

impl<'a> FieldBuilder<'a> {
    /// Initializes a builder object for a new field instance.
    fn new(header: FieldHeader<'a>) -> Self {
        Self {
            kind: header.kind,
            body: header.body,
            lines: vec![header.raw],
        }
    }

    /// Emits the field that was constructed with this builder.
    fn finish(self) -> Field {
        let body = self.normalized_body();

        match self.kind {
            FieldKind::Parameter {
                display_name,
                lookup_name,
                ty,
            } => Field::Parameter {
                display_name: display_name.to_compact_string(),
                lookup_name: lookup_name.to_string(),
                ty: ty.map(|ty| ty.to_compact_string()),
                description: body,
            },
            FieldKind::ParameterType { lookup_name } => Field::ParameterType {
                lookup_name: lookup_name.to_compact_string(),
                ty: body,
            },
            FieldKind::Attribute { name, ty } => Field::Attribute {
                name: name.to_compact_string(),
                ty: ty.map(|ty| ty.to_compact_string()),
                description: body,
            },
            FieldKind::AttributeType { name } => Field::AttributeType {
                name: name.to_compact_string(),
                ty: body,
            },
            FieldKind::Returns { name } => Field::Returns {
                name: name.map(|name| name.to_compact_string()),
                description: body,
            },
            FieldKind::ReturnType => Field::ReturnType { ty: body },
            FieldKind::Raises { exception } => Field::Raises {
                exception: exception.map(|exception| exception.to_compact_string()),
                description: body,
            },
            FieldKind::Metadata => Field::Metadata { body },
            FieldKind::Unknown { name, argument } => Field::Unknown {
                name: name.to_compact_string(),
                argument: argument.to_compact_string(),
                body,
            },
        }
    }

    /// Normalizes the text of the body of a field (e.g., the documentation for a parameter).
    fn normalized_body(&self) -> String {
        // Skip the field header line.
        let continuation_lines = self.lines.iter().skip(1);

        // Use the smallest indentation from all non-blank continuation lines as the normalized
        // continuation indent.
        let continuation_indent = continuation_lines
            .clone()
            .filter(|line| !line.trim().is_empty())
            .map(|line| FieldHeader::indentation(line))
            .min()
            .unwrap_or_default();

        let mut lines = Vec::with_capacity(self.lines.len());

        // Begin with the inline body text parsed from the field header line.
        lines.push(self.body.trim_end().to_string());

        // Then normalize and add all continuation lines.
        lines.extend(continuation_lines.map(|line| {
            if line.trim().is_empty() {
                // Any pure whitespace line becomes an empty line.
                String::new()
            } else {
                // For any other line we strip the shared continuation indent and trailing whitespace.
                line.get(continuation_indent.to_usize()..)
                    .unwrap_or_default()
                    .trim_end()
                    .to_string()
            }
        }));

        // Find non-empty start and end lines.
        let Some(start) = lines.iter().position(|line| !line.is_empty()) else {
            return String::new();
        };
        let end = lines
            .iter()
            .rposition(|line| !line.is_empty())
            .map_or(start, |index| index + 1);

        // Trim empty lines from either end of the result.
        lines[start..end].join("\n")
    }
}

/// Represents a parsed reST field header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FieldHeader<'a> {
    indent: TextSize,
    kind: FieldKind<'a>,
    body: &'a str,
    raw: &'a str,
}

impl<'a> FieldHeader<'a> {
    /// Parses either a valid field header or a malformed header for a supported field.
    fn parse_list_member(line: &'a str) -> Option<Self> {
        Self::parse(line).or_else(|| Self::parse_malformed_supported_field(line))
    }

    /// Finds the start of a reST field (if any) on the given line and at the
    /// given indentation level.
    fn at_indent(line: &'a str, indent: TextSize) -> Option<Self> {
        (Self::indentation(line) == indent)
            .then(|| Self::parse_list_member(line))
            .flatten()
    }

    /// Parses a reST field header of the form `:name [argument]: [body]`.
    ///
    /// The argument may consist of multiple, whitespace-delimited tokens, and both the argument
    /// and the body are optional, so all of the following are accepted:
    ///
    /// ```rst
    /// :meta:
    /// :param count:
    /// :param int count:
    /// :param int count: Number of items.
    /// ```
    ///
    /// Leading indentation is allowed and recorded, so this is also accepted:
    ///
    /// ```rst
    ///     :param int count: Number of items.
    /// ```
    ///
    /// Lines without a field name or without whitespace before a non-empty body are rejected:
    ///
    /// ```rst
    /// ::
    /// :param name:Description.
    /// ```
    fn parse(line: &'a str) -> Option<Self> {
        let trimmed = line.trim_start();
        let after_opening_colon = trimmed.strip_prefix(':')?;
        let (name_and_argument, body) = after_opening_colon.split_once(':')?;
        // Escaped colons belong to the field name, not the field marker.
        if name_and_argument.ends_with('\\') {
            return None;
        }
        if body
            .chars()
            .next()
            .is_some_and(|char| !char.is_whitespace())
        {
            return None;
        }

        let (name, argument) = Self::split_name_and_argument(name_and_argument)?;

        Some(Self {
            indent: Self::indentation(line),
            kind: FieldKind::parse(name, argument),
            body: body.trim_start(),
            raw: line,
        })
    }

    /// Recognizes malformed syntax for a supported field so it remains part of
    /// the surrounding field list and prevents partial structured rendering.
    fn parse_malformed_supported_field(line: &'a str) -> Option<Self> {
        let after_opening_colon = line.trim_start().strip_prefix(':')?;
        let name_and_argument = after_opening_colon
            .split_once(':')
            .map_or(after_opening_colon, |(header, _)| header);
        let (name, argument) = Self::split_name_and_argument(name_and_argument)?;
        FieldKind::parse_supported(name, argument)?;

        Some(Self {
            indent: Self::indentation(line),
            kind: FieldKind::Unknown { name, argument },
            body: "",
            raw: line,
        })
    }

    fn split_name_and_argument(value: &'a str) -> Option<(&'a str, &'a str)> {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }

        let name_end = value.find(char::is_whitespace).unwrap_or(value.len());
        Some((&value[..name_end], value[name_end..].trim()))
    }

    /// Returns the leading indentation of the given source line.
    fn indentation(line: &str) -> TextSize {
        TextSize::of(leading_indentation(line))
    }
}

/// Categorizes the type of a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldKind<'a> {
    Parameter {
        display_name: &'a str,
        lookup_name: &'a str,
        ty: Option<&'a str>,
    },
    ParameterType {
        lookup_name: &'a str,
    },
    Attribute {
        name: &'a str,
        ty: Option<&'a str>,
    },
    AttributeType {
        name: &'a str,
    },
    Returns {
        name: Option<&'a str>,
    },
    ReturnType,
    Raises {
        exception: Option<&'a str>,
    },
    Metadata,
    Unknown {
        name: &'a str,
        argument: &'a str,
    },
}

impl<'a> FieldKind<'a> {
    /// Categorizes a parsed field as a supported field or an unknown field.
    fn parse(name: &'a str, argument: &'a str) -> Self {
        Self::parse_supported(name, argument).unwrap_or(Self::Unknown { name, argument })
    }

    /// Categorizes a supported field, returning `None` only for an unknown field name.
    fn parse_supported(name: &'a str, argument: &'a str) -> Option<Self> {
        let normalized_name = name.to_ascii_lowercase();
        let kind = match normalized_name.as_str() {
            "param" | "parameter" | "arg" | "argument" | "key" | "keyword" | "kwarg"
            | "kwparam" => Self::parse_parameter_argument(argument)
                .map(|(ty, name)| Self::Parameter {
                    display_name: name.display,
                    lookup_name: name.lookup,
                    ty,
                })
                .unwrap_or(Self::Unknown { name, argument }),
            "type" | "paramtype" | "kwtype" => Self::parse_parameter_name(argument)
                .map(|name| Self::ParameterType {
                    lookup_name: name.lookup,
                })
                .unwrap_or(Self::Unknown { name, argument }),
            "var" | "ivar" | "cvar" => Self::parse_attribute_argument(argument)
                .map(|(ty, attribute_name)| Self::Attribute {
                    name: attribute_name,
                    ty,
                })
                .unwrap_or(Self::Unknown { name, argument }),
            "vartype" => Self::parse_attribute_name(argument)
                .map(|attribute_name| Self::AttributeType {
                    name: attribute_name,
                })
                .unwrap_or(Self::Unknown { name, argument }),
            "return" | "returns" => Self::Returns {
                name: Self::parse_parameter_name(argument).map(|name| name.lookup),
            },
            "rtype" if argument.is_empty() => Self::ReturnType,
            "rtype" => Self::Unknown { name, argument },
            "raises" | "raise" | "except" | "exception" => {
                let exception = argument.trim();
                Self::Raises {
                    exception: (!exception.is_empty()).then_some(exception),
                }
            }
            "meta" if !argument.is_empty() => Self::Metadata,
            "meta" => Self::Unknown { name, argument },
            _ => return None,
        };
        Some(kind)
    }

    /// Parses a parameter name and an optional parameter type from a raw field argument.
    /// Returns None if we fail to parse the argument.
    fn parse_parameter_argument(argument: &'a str) -> Option<(Option<&'a str>, ParameterName<'a>)> {
        let argument = argument.trim();
        if argument.is_empty() {
            return None;
        }

        let (ty, name) = Self::split_type_and_name(argument);
        Some((ty, Self::parse_parameter_name(name)?))
    }

    /// Splits up a field argument into an optional type and name.
    fn split_type_and_name(argument: &'a str) -> (Option<&'a str>, &'a str) {
        for (index, char) in argument.char_indices().rev() {
            if char.is_whitespace() {
                let ty = argument[..index].trim();
                let name = &argument[index + char.len_utf8()..];
                return ((!ty.is_empty()).then_some(ty), name);
            }
        }

        (None, argument)
    }

    fn parse_attribute_argument(argument: &'a str) -> Option<(Option<&'a str>, &'a str)> {
        let argument = argument.trim();
        if argument.is_empty() {
            return None;
        }

        let (ty, name) = Self::split_type_and_name(argument);
        Some((ty, Self::parse_attribute_name(name)?))
    }

    fn parse_attribute_name(name: &'a str) -> Option<&'a str> {
        let name = name.trim();
        (!name.is_empty()).then_some(name)
    }

    /// Normalizes a parameter name into display and lookup identifiers.
    fn parse_parameter_name(name: &'a str) -> Option<ParameterName<'a>> {
        let display = name.trim();
        let lookup = display.trim_start_matches('*');
        (!lookup.is_empty()).then_some(ParameterName { display, lookup })
    }
}

/// Represents the reST fields captured by the parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::docstring) enum Field {
    Parameter {
        display_name: CompactString,
        lookup_name: String,
        ty: Option<CompactString>,
        description: String,
    },
    ParameterType {
        lookup_name: CompactString,
        ty: String,
    },
    Attribute {
        name: CompactString,
        ty: Option<CompactString>,
        description: String,
    },
    AttributeType {
        name: CompactString,
        ty: String,
    },
    Returns {
        name: Option<CompactString>,
        description: String,
    },
    ReturnType {
        ty: String,
    },
    Raises {
        exception: Option<CompactString>,
        description: String,
    },
    Metadata {
        body: String,
    },
    Unknown {
        name: CompactString,
        argument: CompactString,
        body: String,
    },
}

/// Container for the display name (shown to the user) and the lookup name
/// (used to look up semantic information) for a particular parameter.
///
/// For instance, typical variadic positional parameters will have a `display`
/// of "*args" and `lookup` of "args".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParameterName<'a> {
    display: &'a str,
    lookup: &'a str,
}

#[cfg(test)]
mod tests {
    use std::iter::repeat_n;

    use insta::{assert_debug_snapshot, assert_snapshot};

    use super::{
        Field, FieldList, Lines, field_lists,
        parameter_documentation as extract_parameter_documentation,
    };

    #[test]
    fn parameter_documentation_extracts_rest_parameters() {
        let docstring = r#"
        This is a function description.

        :param str param1: The first parameter description
        :param int param2: The second parameter description
            This is a continuation of param2 description.
        :param **kwargs: Extra keyword arguments.
        :returns: The return value description
        "#;
        let param_docs = parameter_documentation(docstring);

        assert_snapshot!(param_docs, @r"
        param1: The first parameter description
        param2: The second parameter description
          This is a continuation of param2 description.
        kwargs: Extra keyword arguments.
        ");
    }

    #[test]
    fn parameter_documentation_ignores_fields_in_block_quotes() {
        let parameters = extract_parameter_documentation(
            "\
Summary.

    :param example: Quoted example.

:param real: Real parameter.",
        );

        assert!(!parameters.contains_key("example"));
        assert_eq!(
            parameters.get("real").map(String::as_str),
            Some("Real parameter.")
        );
    }

    #[test]
    fn parameter_documentation_ignores_fields_continuing_paragraphs() {
        let parameters = extract_parameter_documentation(
            "\
Summary.
:param example: Paragraph text.
Continuation.

:param real: Real parameter.",
        );

        assert!(!parameters.contains_key("example"));
        assert_eq!(
            parameters.get("real").map(String::as_str),
            Some("Real parameter.")
        );
    }

    #[test]
    fn parameter_documentation_supports_parameter_aliases() {
        let docstring = r#"
        :parameter first: The first parameter.
        :arg second: The second parameter.
        :argument third: The third parameter.
        :key fourth: The fourth parameter.
        :keyword fifth: The fifth parameter.
        :kwarg sixth: The sixth parameter.
        :kwparam seventh: The seventh parameter.
        "#;
        let param_docs = parameter_documentation(docstring);
        assert_snapshot!(param_docs, @r"
        first: The first parameter.
        second: The second parameter.
        third: The third parameter.
        fourth: The fourth parameter.
        fifth: The fifth parameter.
        sixth: The sixth parameter.
        seventh: The seventh parameter.
        ");
    }

    #[test]
    fn parser_supports_complex_inline_parameter_types() {
        let parsed = field_lists(
            "\
:param list[str] items: Item descriptions.
:param dict[str, list[int | None]] mapping: Mapping description.
:param Callable[[int, str], bool] callback: Callback description.",
        );

        assert_debug_snapshot!(&parsed[0].fields, @r#"
        [
            Parameter {
                display_name: "items",
                lookup_name: "items",
                ty: Some(
                    "list[str]",
                ),
                description: "Item descriptions.",
            },
            Parameter {
                display_name: "mapping",
                lookup_name: "mapping",
                ty: Some(
                    "dict[str, list[int | None]]",
                ),
                description: "Mapping description.",
            },
            Parameter {
                display_name: "callback",
                lookup_name: "callback",
                ty: Some(
                    "Callable[[int, str], bool]",
                ),
                description: "Callback description.",
            },
        ]
        "#);
    }

    #[test]
    fn parameter_documentation_stops_at_field_boundaries() {
        let docstring = r#"
        :param param: The parameter description
        :type param: bool
        :returns value: The return value description
        :rtype: str
        "#;
        let param_docs = parameter_documentation(docstring);

        assert_snapshot!(param_docs, @"param: The parameter description");
    }

    #[test]
    fn parameter_documentation_ignores_parameters_without_names_after_normalization() {
        assert_snapshot!(
            parameter_documentation(":param **: Missing a parameter name."),
            @""
        );
    }

    #[test]
    fn parser_preserves_supported_and_unknown_fields() {
        let docstring = "\
:param tuple[str, ...] *args: Extra positional arguments.
:type args: tuple[str, ...]
:var dict[str, int] cache: Cached values.
:vartype cache: dict[str, int]
:returns result: Return description.
:rtype: str
:raises ValueError: Error description.
:meta private:
:unknown with argument: Unknown description.";
        let parsed = field_lists(docstring);

        assert_eq!(parsed[0].start_line, 0);
        assert_eq!(parsed[0].end_line, 9);
        assert_eq!(
            &docstring[parsed[0].range.start().to_usize()..parsed[0].range.end().to_usize()],
            docstring
        );
        assert_debug_snapshot!(&parsed[0].fields, @r#"
        [
            Parameter {
                display_name: "*args",
                lookup_name: "args",
                ty: Some(
                    "tuple[str, ...]",
                ),
                description: "Extra positional arguments.",
            },
            ParameterType {
                lookup_name: "args",
                ty: "tuple[str, ...]",
            },
            Attribute {
                name: "cache",
                ty: Some(
                    "dict[str, int]",
                ),
                description: "Cached values.",
            },
            AttributeType {
                name: "cache",
                ty: "dict[str, int]",
            },
            Returns {
                name: Some(
                    "result",
                ),
                description: "Return description.",
            },
            ReturnType {
                ty: "str",
            },
            Raises {
                exception: Some(
                    "ValueError",
                ),
                description: "Error description.",
            },
            Metadata {
                body: "",
            },
            Unknown {
                name: "unknown",
                argument: "with argument",
                body: "Unknown description.",
            },
        ]
        "#);
    }

    #[test]
    fn parser_records_field_list_ranges() {
        let docstring = "\
Intro paragraph.

:param first: First parameter.

Intervening prose.

:param second: Second parameter.
    Continued::

        literal block

Trailing prose.
";
        let parsed = field_lists(docstring);

        assert_eq!(parsed.len(), 2);

        let first = &parsed[0];
        assert_eq!(first.start_line, 2);
        assert_eq!(first.end_line, 3);

        let second = &parsed[1];
        assert_eq!(second.start_line, 6);
        assert_eq!(second.end_line, 10);
        let second_description = second.fields.iter().find_map(|field| match field {
            Field::Parameter { description, .. } => Some(description),
            _ => None,
        });
        assert_eq!(
            second_description.map(String::as_str),
            Some("Second parameter.\nContinued::\n\n    literal block")
        );

        assert_snapshot!(field_list_ranges(docstring, &parsed), @r"
        | Intro paragraph.
        |
        | :param first: First parameter.
        | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        |
        | Intervening prose.
        |
        | :param second: Second parameter.
        | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        |     Continued::
        | ^^^^^^^^^^^^^^^
        |
        |         literal block
        | ^^^^^^^^^^^^^^^^^^^^^
        |
        | Trailing prose.
        ");
    }

    #[test]
    fn parser_recovers_from_partial_and_malformed_fields() {
        let docstring = "\
:param first: Parsed before malformed input.
:param missing-space:This is malformed because body text must be separated by whitespace.
:param:
:param **: Invalid after parameter-name normalization.
:param empty:
:param list[str] second: Parsed after malformed and partial fields.
:param
:param third: Parsed after an incomplete field marker.";

        assert_eq!(field_lists(docstring).len(), 1);
        let param_docs = parameter_documentation(docstring);

        assert_snapshot!(param_docs, @r"
        first: Parsed before malformed input.
        second: Parsed after malformed and partial fields.
        third: Parsed after an incomplete field marker.
        ");
    }

    #[test]
    fn parameter_documentation_supports_continuation_only_descriptions() {
        let param_docs = parameter_documentation(
            "\
:param value:
  First paragraph.


  Second paragraph.
:param other: Other parameter.",
        );

        assert_snapshot!(param_docs, @r"
        value: First paragraph.


          Second paragraph.
        other: Other parameter.
        ");
    }

    #[test]
    fn parser_treats_indented_field_like_text_as_continuation() {
        let param_docs = parameter_documentation(
            "\
:param first: First line.
    :param fake: This is continuation text, not a new field.
:param second: Real second parameter.",
        );

        assert_snapshot!(param_docs, @r"
        first: First line.
          :param fake: This is continuation text, not a new field.
        second: Real second parameter.
        ");
    }

    #[test]
    fn parameter_documentation_recovers_sibling_fields_after_missing_literal_block() {
        let param_docs = parameter_documentation(
            "\
:param quoted: Example::

:param sample: This is sample input.
:returns: This is still sample input.

:param real: Real parameter.",
        );

        assert_snapshot!(param_docs, @r"
        quoted: Example::
        sample: This is sample input.
        real: Real parameter.
        ");
    }

    #[test]
    fn parameter_documentation_requires_blank_line_before_field_body_literal_block() {
        let param_docs = parameter_documentation(
            "\
:param first: Explanation::
:param second: Second parameter.",
        );

        assert_snapshot!(param_docs, @r"
        first: Explanation::
        second: Second parameter.
        ");
    }

    #[test]
    fn literal_blocks_take_precedence_over_markdown_fences_in_preformatted_blocks() {
        let docstring = "\
Literal block::

    ```python
    :param fake: This is sample input.
    ```

:param real: Real parameter.";

        let param_docs = parameter_documentation(docstring);

        assert_snapshot!(param_docs, @"real: Real parameter.");
    }

    #[test]
    fn literal_blocks_use_marker_indentation_as_exit_threshold() {
        let docstring = "\
Literal block::

        sample
    :param fake: This is sample input.

:param real: Real parameter.";

        let param_docs = parameter_documentation(docstring);

        assert_snapshot!(param_docs, @"real: Real parameter.");
    }

    #[test]
    fn quoted_literal_blocks_are_preformatted_blocks() {
        let docstring = "\
Literal block::

:param fake: This is sample input.
:param also_fake: This is more sample input.

:param real: Real parameter.";

        let param_docs = parameter_documentation(docstring);

        assert_snapshot!(param_docs, @"real: Real parameter.");
    }

    #[test]
    fn parameter_documentation_recovers_after_same_indent_one_line_directive() {
        let docstring = "\
.. seealso:: other
:param value: Value parameter.

Section::

    :param fake: This is sample input.

:param next: Next parameter.";

        let param_docs = parameter_documentation(docstring);

        assert_snapshot!(param_docs, @"next: Next parameter.");
    }

    #[test]
    fn doctests_take_precedence_over_markdown_fences_in_preformatted_blocks() {
        let docstring = "\
>>> print(\"field list\")
```
:param fake: This is doctest output.

:param real: Real parameter.";

        let param_docs = parameter_documentation(docstring);

        assert_snapshot!(param_docs, @"real: Real parameter.");
    }

    fn parameter_documentation(docstring: &str) -> String {
        let parameters = super::parameter_documentation(docstring);
        let mut rendered = String::new();

        for (name, description) in parameters {
            if !rendered.is_empty() {
                rendered.push('\n');
            }

            rendered.push_str(name.as_str());
            rendered.push_str(": ");

            let mut lines = description.lines();
            let Some(first_line) = lines.next() else {
                continue;
            };
            rendered.push_str(first_line);

            for line in lines {
                rendered.push('\n');
                if !line.is_empty() {
                    rendered.push_str("  ");
                    rendered.push_str(line);
                }
            }
        }

        rendered
    }

    fn field_list_ranges(docstring: &str, field_lists: &[FieldList]) -> String {
        let mut lines = Lines::new(docstring);
        let mut rendered = String::new();

        while let Some(line) = lines.next() {
            if !rendered.is_empty() {
                rendered.push('\n');
            }

            rendered.push('|');
            if !line.text.is_empty() {
                rendered.push(' ');
                rendered.push_str(line.text);
            }

            if let Some(intersection) = field_lists
                .iter()
                .filter_map(|field_list| line.range.intersect(field_list.range))
                .find(|intersection| !intersection.is_empty())
            {
                let start = intersection.start().to_usize() - line.range.start().to_usize();
                let end = intersection.end().to_usize() - line.range.start().to_usize();

                rendered.push('\n');
                rendered.push_str("| ");
                rendered.extend(repeat_n(' ', start));
                rendered.extend(repeat_n('^', end - start));
            }
        }

        rendered
    }
}
