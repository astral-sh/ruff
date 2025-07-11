//! Docstring parsing utilities for language server features.
//!
//! This module provides functionality for extracting structured information from
//! Python docstrings, including parameter documentation for signature help.
//! Supports Google-style, NumPy-style, and reST/Sphinx-style docstrings.
//! There are no formal specifications for any of these formats, so the parsing
//! logic needs to be tolerant of variations.

use regex::Regex;
use ruff_python_trivia::leading_indentation;
use ruff_source_file::UniversalNewlines;
use std::collections::HashMap;
use std::sync::LazyLock;

// Static regex instances to avoid recompilation
static GOOGLE_SECTION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(Args|Arguments|Parameters)\s*:\s*$")
        .expect("Google section regex should be valid")
});

static GOOGLE_PARAM_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(\*?\*?\w+)\s*(\(.*?\))?\s*:\s*(.+)")
        .expect("Google parameter regex should be valid")
});

static NUMPY_SECTION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*Parameters\s*$").expect("NumPy section regex should be valid")
});

static NUMPY_UNDERLINE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*-+\s*$").expect("NumPy underline regex should be valid"));

static REST_PARAM_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*:param\s+(?:(\w+)\s+)?(\w+)\s*:\s*(.+)")
        .expect("reST parameter regex should be valid")
});

/// Extract parameter documentation from popular docstring formats.
/// Returns a map of parameter names to their documentation.
pub fn get_parameter_documentation(docstring: &str) -> HashMap<String, String> {
    let mut param_docs = HashMap::new();

    // Google-style docstrings
    param_docs.extend(extract_google_style_params(docstring));

    // NumPy-style docstrings
    param_docs.extend(extract_numpy_style_params(docstring));

    // reST/Sphinx-style docstrings
    param_docs.extend(extract_rest_style_params(docstring));

    param_docs
}

/// Extract parameter documentation from Google-style docstrings.
fn extract_google_style_params(docstring: &str) -> HashMap<String, String> {
    let mut param_docs = HashMap::new();

    let mut in_args_section = false;
    let mut current_param: Option<String> = None;
    let mut current_doc = String::new();

    for line_obj in docstring.universal_newlines() {
        let line = line_obj.as_str();
        if GOOGLE_SECTION_REGEX.is_match(line) {
            in_args_section = true;
            continue;
        }

        if in_args_section {
            // Check if we hit another section (starts with a word followed by colon at line start)
            if !line.starts_with(' ') && !line.starts_with('\t') && line.contains(':') {
                if let Some(colon_pos) = line.find(':') {
                    let section_name = line[..colon_pos].trim();
                    // If this looks like another section, stop processing args
                    if !section_name.is_empty()
                        && section_name
                            .chars()
                            .all(|c| c.is_alphabetic() || c.is_whitespace())
                    {
                        // Check if this is a known section name
                        let known_sections = [
                            "Returns", "Return", "Raises", "Yields", "Yield", "Examples",
                            "Example", "Note", "Notes", "Warning", "Warnings",
                        ];
                        if known_sections.contains(&section_name) {
                            if let Some(param_name) = current_param.take() {
                                param_docs.insert(param_name, current_doc.trim().to_string());
                                current_doc.clear();
                            }
                            in_args_section = false;
                            continue;
                        }
                    }
                }
            }

            if let Some(captures) = GOOGLE_PARAM_REGEX.captures(line) {
                // Save previous parameter if exists
                if let Some(param_name) = current_param.take() {
                    param_docs.insert(param_name, current_doc.trim().to_string());
                    current_doc.clear();
                }

                // Start new parameter
                if let (Some(param), Some(desc)) = (captures.get(1), captures.get(3)) {
                    current_param = Some(param.as_str().to_string());
                    current_doc = desc.as_str().to_string();
                }
            } else if line.starts_with(' ') || line.starts_with('\t') {
                // This is a continuation of the current parameter documentation
                if current_param.is_some() {
                    if !current_doc.is_empty() {
                        current_doc.push('\n');
                    }
                    current_doc.push_str(line.trim());
                }
            } else {
                // This is a line that doesn't start with whitespace and isn't a parameter
                // It might be a section or other content, so stop processing args
                if let Some(param_name) = current_param.take() {
                    param_docs.insert(param_name, current_doc.trim().to_string());
                    current_doc.clear();
                }
                in_args_section = false;
            }
        }
    }

    // Don't forget the last parameter
    if let Some(param_name) = current_param {
        param_docs.insert(param_name, current_doc.trim().to_string());
    }

    param_docs
}

/// Calculate the indentation level of a line (number of leading whitespace characters)
fn get_indentation_level(line: &str) -> usize {
    leading_indentation(line).len()
}

/// Extract parameter documentation from NumPy-style docstrings.
fn extract_numpy_style_params(docstring: &str) -> HashMap<String, String> {
    let mut param_docs = HashMap::new();

    let mut lines = docstring
        .universal_newlines()
        .map(|line| line.as_str())
        .peekable();
    let mut in_params_section = false;
    let mut found_underline = false;
    let mut current_param: Option<String> = None;
    let mut current_doc = String::new();
    let mut base_param_indent: Option<usize> = None;
    let mut base_content_indent: Option<usize> = None;

    while let Some(line) = lines.next() {
        if NUMPY_SECTION_REGEX.is_match(line) {
            // Check if the next line is an underline
            if let Some(next_line) = lines.peek() {
                if NUMPY_UNDERLINE_REGEX.is_match(next_line) {
                    in_params_section = true;
                    found_underline = false;
                    base_param_indent = None;
                    base_content_indent = None;
                    continue;
                }
            }
        }

        if in_params_section && !found_underline {
            if NUMPY_UNDERLINE_REGEX.is_match(line) {
                found_underline = true;
                continue;
            }
        }

        if in_params_section && found_underline {
            let current_indent = get_indentation_level(line);
            let trimmed = line.trim();

            // Skip empty lines
            if trimmed.is_empty() {
                continue;
            }

            // Check if we hit another section
            if current_indent == 0 {
                if let Some(next_line) = lines.peek() {
                    if NUMPY_UNDERLINE_REGEX.is_match(next_line) {
                        // This is another section
                        if let Some(param_name) = current_param.take() {
                            param_docs.insert(param_name, current_doc.trim().to_string());
                            current_doc.clear();
                        }
                        in_params_section = false;
                        continue;
                    }
                }
            }

            // Determine if this could be a parameter line
            let could_be_param = if let Some(base_indent) = base_param_indent {
                // We've seen parameters before - check if this matches the expected parameter indentation
                current_indent == base_indent
            } else {
                // First potential parameter - check if it has reasonable indentation and content
                current_indent > 0
                    && (trimmed.contains(':')
                        || trimmed.chars().all(|c| c.is_alphanumeric() || c == '_'))
            };

            if could_be_param {
                // Check if this could be a section header by looking at the next line
                if let Some(next_line) = lines.peek() {
                    if NUMPY_UNDERLINE_REGEX.is_match(next_line) {
                        // This is a section header, not a parameter
                        if let Some(param_name) = current_param.take() {
                            param_docs.insert(param_name, current_doc.trim().to_string());
                            current_doc.clear();
                        }
                        in_params_section = false;
                        continue;
                    }
                }

                // Set base indentation levels on first parameter
                if base_param_indent.is_none() {
                    base_param_indent = Some(current_indent);
                }

                // Handle parameter with type annotation (param : type)
                if trimmed.contains(':') {
                    // Save previous parameter if exists
                    if let Some(param_name) = current_param.take() {
                        param_docs.insert(param_name, current_doc.trim().to_string());
                        current_doc.clear();
                    }

                    // Extract parameter name and description
                    let parts: Vec<&str> = trimmed.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        let param_name = parts[0].trim();

                        // Extract just the parameter name (before any type info)
                        let param_name = param_name.split_whitespace().next().unwrap_or(param_name);
                        current_param = Some(param_name.to_string());
                        current_doc.clear(); // Description comes on following lines, not on this line
                    }
                } else {
                    // Handle parameter without type annotation
                    // Save previous parameter if exists
                    if let Some(param_name) = current_param.take() {
                        param_docs.insert(param_name, current_doc.trim().to_string());
                        current_doc.clear();
                    }

                    // This line is the parameter name
                    current_param = Some(trimmed.to_string());
                    current_doc.clear();
                }
            } else if current_param.is_some() {
                // Determine if this is content for the current parameter
                let is_content = if let Some(base_content) = base_content_indent {
                    // We've seen content before - check if this matches expected content indentation
                    current_indent >= base_content
                } else {
                    // First potential content line - should be more indented than parameter
                    if let Some(base_param) = base_param_indent {
                        current_indent > base_param
                    } else {
                        // Fallback: any indented content
                        current_indent > 0
                    }
                };

                if is_content {
                    // Set base content indentation on first content line
                    if base_content_indent.is_none() {
                        base_content_indent = Some(current_indent);
                    }

                    // This is a continuation of the current parameter documentation
                    if !current_doc.is_empty() {
                        current_doc.push('\n');
                    }
                    current_doc.push_str(trimmed);
                } else {
                    // This line doesn't match our expected indentation patterns
                    // Save current parameter and stop processing
                    if let Some(param_name) = current_param.take() {
                        param_docs.insert(param_name, current_doc.trim().to_string());
                        current_doc.clear();
                    }
                    in_params_section = false;
                }
            }
        }
    }

    // Don't forget the last parameter
    if let Some(param_name) = current_param {
        param_docs.insert(param_name, current_doc.trim().to_string());
    }

    param_docs
}

/// Extract parameter documentation from reST/Sphinx-style docstrings.
fn extract_rest_style_params(docstring: &str) -> HashMap<String, String> {
    let mut param_docs = HashMap::new();

    let mut current_param: Option<String> = None;
    let mut current_doc = String::new();

    for line_obj in docstring.universal_newlines() {
        let line = line_obj.as_str();
        if let Some(captures) = REST_PARAM_REGEX.captures(line) {
            // Save previous parameter if exists
            if let Some(param_name) = current_param.take() {
                param_docs.insert(param_name, current_doc.trim().to_string());
                current_doc.clear();
            }

            // Extract parameter name and description
            if let (Some(param_match), Some(desc_match)) = (captures.get(2), captures.get(3)) {
                current_param = Some(param_match.as_str().to_string());
                current_doc = desc_match.as_str().to_string();
            }
        } else if current_param.is_some() {
            let trimmed = line.trim();

            // Check if this is a new section - stop processing if we hit section headers
            if trimmed == "Parameters" || trimmed == "Args" || trimmed == "Arguments" {
                // Save current param and stop processing
                if let Some(param_name) = current_param.take() {
                    param_docs.insert(param_name, current_doc.trim().to_string());
                    current_doc.clear();
                }
                break;
            }

            // Check if this is another directive line starting with ':'
            if trimmed.starts_with(':') {
                // This is a new directive, save current param
                if let Some(param_name) = current_param.take() {
                    param_docs.insert(param_name, current_doc.trim().to_string());
                    current_doc.clear();
                }
                // Let the next iteration handle this directive
                continue;
            }

            // Check if this is a continuation line (indented)
            if line.starts_with("    ") && !trimmed.is_empty() {
                // This is a continuation line
                if !current_doc.is_empty() {
                    current_doc.push('\n');
                }
                current_doc.push_str(trimmed);
            } else if !trimmed.is_empty() && !line.starts_with(' ') && !line.starts_with('\t') {
                // This is a non-indented line - likely end of the current parameter
                if let Some(param_name) = current_param.take() {
                    param_docs.insert(param_name, current_doc.trim().to_string());
                    current_doc.clear();
                }
                break;
            }
        }
    }

    // Don't forget the last parameter
    if let Some(param_name) = current_param {
        param_docs.insert(param_name, current_doc.trim().to_string());
    }

    param_docs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_google_style_parameter_documentation() {
        let docstring = r#"
        This is a function description.

        Args:
            param1 (str): The first parameter description
            param2 (int): The second parameter description
                This is a continuation of param2 description.
            param3: A parameter without type annotation

        Returns:
            str: The return value description
        "#;

        let param_docs = get_parameter_documentation(docstring);

        assert_eq!(param_docs.len(), 3);
        assert_eq!(&param_docs["param1"], "The first parameter description");
        assert_eq!(
            &param_docs["param2"],
            "The second parameter description\nThis is a continuation of param2 description."
        );
        assert_eq!(&param_docs["param3"], "A parameter without type annotation");
    }

    #[test]
    fn test_numpy_style_parameter_documentation() {
        let docstring = r#"
        This is a function description.

        Parameters
        ----------
        param1 : str
            The first parameter description
        param2 : int
            The second parameter description
            This is a continuation of param2 description.
        param3
            A parameter without type annotation

        Returns
        -------
        str
            The return value description
        "#;

        let param_docs = get_parameter_documentation(docstring);

        assert_eq!(param_docs.len(), 3);
        assert_eq!(
            param_docs.get("param1").expect("param1 should exist"),
            "The first parameter description"
        );
        assert_eq!(
            param_docs.get("param2").expect("param2 should exist"),
            "The second parameter description\nThis is a continuation of param2 description."
        );
        assert_eq!(
            param_docs.get("param3").expect("param3 should exist"),
            "A parameter without type annotation"
        );
    }

    #[test]
    fn test_no_parameter_documentation() {
        let docstring = r#"
        This is a simple function description without parameter documentation.
        "#;

        let param_docs = get_parameter_documentation(docstring);
        assert!(param_docs.is_empty());
    }

    #[test]
    fn test_mixed_style_parameter_documentation() {
        let docstring = r#"
        This is a function description.

        Args:
            param1 (str): Google-style parameter
            param2 (int): Another Google-style parameter

        Parameters
        ----------
        param3 : bool
            NumPy-style parameter
        "#;

        let param_docs = get_parameter_documentation(docstring);

        assert_eq!(param_docs.len(), 3);
        assert_eq!(
            param_docs.get("param1").expect("param1 should exist"),
            "Google-style parameter"
        );
        assert_eq!(
            param_docs.get("param2").expect("param2 should exist"),
            "Another Google-style parameter"
        );
        assert_eq!(
            param_docs.get("param3").expect("param3 should exist"),
            "NumPy-style parameter"
        );
    }

    #[test]
    fn test_rest_style_parameter_documentation() {
        let docstring = r#"
        This is a function description.

        :param str param1: The first parameter description
        :param int param2: The second parameter description
            This is a continuation of param2 description.
        :param param3: A parameter without type annotation
        :returns: The return value description
        :rtype: str
        "#;

        let param_docs = get_parameter_documentation(docstring);

        assert_eq!(param_docs.len(), 3);
        assert_eq!(
            param_docs.get("param1").expect("param1 should exist"),
            "The first parameter description"
        );
        assert_eq!(
            param_docs.get("param2").expect("param2 should exist"),
            "The second parameter description\nThis is a continuation of param2 description."
        );
        assert_eq!(
            param_docs.get("param3").expect("param3 should exist"),
            "A parameter without type annotation"
        );
    }

    #[test]
    fn test_mixed_style_with_rest_parameter_documentation() {
        let docstring = r#"
        This is a function description.

        Args:
            param1 (str): Google-style parameter

        :param int param2: reST-style parameter
        :param param3: Another reST-style parameter

        Parameters
        ----------
        param4 : bool
            NumPy-style parameter
        "#;

        let param_docs = get_parameter_documentation(docstring);

        assert_eq!(param_docs.len(), 4);
        assert_eq!(
            param_docs.get("param1").expect("param1 should exist"),
            "Google-style parameter"
        );
        assert_eq!(
            param_docs.get("param2").expect("param2 should exist"),
            "reST-style parameter"
        );
        assert_eq!(
            param_docs.get("param3").expect("param3 should exist"),
            "Another reST-style parameter"
        );
        assert_eq!(
            param_docs.get("param4").expect("param4 should exist"),
            "NumPy-style parameter"
        );
    }

    #[test]
    fn test_numpy_style_with_different_indentation() {
        let docstring = r#"
        This is a function description.

        Parameters
        ----------
        param1 : str
            The first parameter description
        param2 : int
            The second parameter description
            This is a continuation of param2 description.
        param3
            A parameter without type annotation

        Returns
        -------
        str
            The return value description
        "#;

        let param_docs = get_parameter_documentation(docstring);

        assert_eq!(param_docs.len(), 3);
        assert_eq!(
            param_docs.get("param1").expect("param1 should exist"),
            "The first parameter description"
        );
        assert_eq!(
            param_docs.get("param2").expect("param2 should exist"),
            "The second parameter description\nThis is a continuation of param2 description."
        );
        assert_eq!(
            param_docs.get("param3").expect("param3 should exist"),
            "A parameter without type annotation"
        );
    }

    #[test]
    fn test_numpy_style_with_tabs_and_mixed_indentation() {
        // Using raw strings to avoid tab/space conversion issues in the test
        let docstring = "
        This is a function description.

        Parameters
        ----------
\tparam1 : str
\t\tThe first parameter description
\tparam2 : int
\t\tThe second parameter description
\t\tThis is a continuation of param2 description.
\tparam3
\t\tA parameter without type annotation
        ";

        let param_docs = get_parameter_documentation(docstring);

        assert_eq!(param_docs.len(), 3);
        assert_eq!(
            param_docs.get("param1").expect("param1 should exist"),
            "The first parameter description"
        );
        assert_eq!(
            param_docs.get("param2").expect("param2 should exist"),
            "The second parameter description\nThis is a continuation of param2 description."
        );
        assert_eq!(
            param_docs.get("param3").expect("param3 should exist"),
            "A parameter without type annotation"
        );
    }

    #[test]
    fn test_universal_newlines() {
        // Test with Windows-style line endings (\r\n)
        let docstring_windows = "This is a function description.\r\n\r\nArgs:\r\n    param1 (str): The first parameter\r\n    param2 (int): The second parameter\r\n";

        // Test with old Mac-style line endings (\r)
        let docstring_mac = "This is a function description.\r\rArgs:\r    param1 (str): The first parameter\r    param2 (int): The second parameter\r";

        // Test with Unix-style line endings (\n) - should work the same
        let docstring_unix = "This is a function description.\n\nArgs:\n    param1 (str): The first parameter\n    param2 (int): The second parameter\n";

        let param_docs_windows = get_parameter_documentation(docstring_windows);
        let param_docs_mac = get_parameter_documentation(docstring_mac);
        let param_docs_unix = get_parameter_documentation(docstring_unix);

        // All should produce the same results
        assert_eq!(param_docs_windows.len(), 2);
        assert_eq!(param_docs_mac.len(), 2);
        assert_eq!(param_docs_unix.len(), 2);

        assert_eq!(
            param_docs_windows.get("param1"),
            Some(&"The first parameter".to_string())
        );
        assert_eq!(
            param_docs_mac.get("param1"),
            Some(&"The first parameter".to_string())
        );
        assert_eq!(
            param_docs_unix.get("param1"),
            Some(&"The first parameter".to_string())
        );
    }
}
