use crate::docstring::document::google;

use super::{ParameterHeading, Section, SectionItem, SectionKind};

/// Returns Google-style sections that can be rendered structurally.
pub(super) fn structured_sections(normalized_source: &str) -> Vec<Section> {
    google::sections(normalized_source)
        .filter_map(section)
        .collect()
}

fn section(parsed: google::Section) -> Option<Section> {
    let kind = parsed.kind();
    let range = parsed.range();
    let fragments = parsed.into_renderable_fragments()?;
    if fragments.is_empty() {
        return None;
    }
    let items = fragments
        .into_iter()
        .map(|fragment| section_item(kind, fragment))
        .collect();
    Section::new_with_parameter_heading(range, items, ParameterHeading::Arguments)
}

fn section_item(kind: SectionKind, fragment: google::BodyFragment) -> SectionItem {
    match fragment {
        google::BodyFragment::Prose(description) => {
            SectionItem::from_owned_parts(kind, None, None, description)
        }
        google::BodyFragment::Item(item) => {
            let (display_name, ty, description) = item.into_display_name_type_and_description();
            SectionItem::from_owned_parts(kind, Some(display_name), ty, description)
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::{Settings, assert_snapshot};

    use super::super::render_sections_into;
    use super::structured_sections;

    #[test]
    fn renders_supported_sections() {
        let _snap = bind_markdown_snapshot_filters();
        let docstring = "\
Summary.

Keyword Args:
    optional (`int`): Optional value.

Other Parameters:
    timeout (float): Maximum wait.

Attributes:
    name (str): Display name.

Yields:
    The next `int` value.

Raises:
    ValueError: If invalid.
";

        assert_snapshot!(render_google(docstring), @"
        Summary.

        ## Keyword Arguments
        **optional**: `int`<HB>
        Optional value.

        ## Other Parameters
        **timeout**: `float`<HB>
        Maximum wait.

        ## Attributes
        **name**: `str`<HB>
        Display name.

        ## Yields
        The next `int` value.

        ## Raises
        `ValueError`<HB>
        If invalid.
        ");
    }

    #[test]
    fn renders_aligned_parameter_continuation() {
        let _snap = bind_markdown_snapshot_filters();
        let docstring = "\
Args:
    value: The value.
    For example: pass an absolute path.
";

        assert_snapshot!(render_google(docstring), @"
        ## Arguments
        **value**<HB>
        The value.<HB>
        For example: pass an absolute path.
        ");
    }

    #[test]
    fn renders_parameter_url_continuation() {
        let _snap = bind_markdown_snapshot_filters();
        let docstring = "\
Args:
    endpoint: Service endpoint, for example:
    https://example.com/api
";

        assert_snapshot!(render_google(docstring), @"
        ## Arguments
        **endpoint**<HB>
        Service endpoint, for example:<HB>
        https://example.com/api
        ");
    }

    #[test]
    fn renders_parameter_windows_path_continuation() {
        let _snap = bind_markdown_snapshot_filters();
        let docstring = "\
Args:
    output: Output path, for example:
    C:\\temp\\result.txt
";

        assert_snapshot!(render_google(docstring), @r"
        ## Arguments
        **output**<HB>
        Output path, for example:<HB>
        C:\temp\result.txt
        ");
    }

    #[test]
    fn renders_preformatted_parameter_descriptions() {
        let _snap = bind_markdown_snapshot_filters();
        let docstring = "\
Args:
    value: Description.
        ```python
        Args:
            nested: Still code.
        Returns:
            Still code.
        ```
    url (Literal[\"http://\"]): URL.
";

        assert_snapshot!(render_google(docstring), @r#"
        ## Arguments
        **value**<HB>
        Description.

        ```python
        Args:
            nested: Still code.
        Returns:
            Still code.
        ```

        **url**: `Literal["http://"]`<HB>
        URL.
        "#);
    }

    #[test]
    fn renders_return_body_as_prose() {
        let _snap = bind_markdown_snapshot_filters();
        let docstring = "\
Returns:
    int: The **count**.
    str: The `name`.
";

        assert_snapshot!(render_google(docstring), @"
        ## Returns
        int: The **count**.<HB>
        str: The `name`.
        ");
    }

    #[test]
    fn renders_nested_list_in_return_prose() {
        let _snap = bind_markdown_snapshot_filters();
        let docstring = "\
Returns:
    - first
      - nested
";

        assert_snapshot!(render_google(docstring), @"
        ## Returns
        - first<HB>
          - nested
        ");
    }

    #[test]
    fn renders_fenced_block_in_return_prose() {
        let _snap = bind_markdown_snapshot_filters();
        let docstring = "\
Returns:
    str: Example output.
        ```python
        Args:
            still code.
        ```
";

        assert_snapshot!(render_google(docstring), @"
        ## Returns
        str: Example output.

        ```python
        Args:
            still code.
        ```
        ");
    }

    #[test]
    fn renders_nested_heading_in_return_prose() {
        let _snap = bind_markdown_snapshot_filters();
        let docstring = "\
Returns:
    The result.

    Examples:
    This heading is part of the description.
";

        assert_snapshot!(render_google(docstring), @"
        ## Returns
        The result.

        Examples:<HB>
        This heading is part of the description.
        ");
    }

    #[test]
    fn declines_to_render_inline_section_heading_in_parameter_body() {
        let docstring = "\
Summary.

Args:
    value: The value.
    Examples: Try it.
";

        assert!(parsed_sections(docstring).is_empty());
    }

    #[test]
    fn declines_to_render_unclosed_parameter_fence() {
        let docstring = "\
Summary.

Args:
    value: Example.
        ```python

Args:
    nested = 1
        ```
";

        assert!(parsed_sections(docstring).is_empty());
    }

    #[test]
    fn declines_to_render_parameter_section_with_unrecognized_leading_content() {
        let docstring = "\
Args:
    (data, indices) : data and indices in batched COO format.
    shape : shape of sparse array.
";

        assert!(parsed_sections(docstring).is_empty());
    }

    #[test]
    fn declines_to_render_conjunction_separated_parameter_names() {
        let docstring = "\
Arguments:
    args: Program arguments.

    stdin, stdout and stderr: Standard stream handles.
";

        assert!(parsed_sections(docstring).is_empty());
    }

    #[test]
    fn declines_to_render_parameter_section_with_unbalanced_type_brackets() {
        let docstring = "\
Args:
    query_embeddings (`Union[torch.Tensor, list[torch.Tensor]`): Query embeddings.
";

        assert!(parsed_sections(docstring).is_empty());
    }

    #[test]
    fn declines_to_render_empty_return_section() {
        let docstring = "\
Summary.

Returns:

After.
";

        assert!(parsed_sections(docstring).is_empty());
    }

    fn render_google(normalized_source: &str) -> String {
        let mut output = String::new();
        render_sections_into(
            &mut output,
            normalized_source,
            parsed_sections(normalized_source),
        );
        output
    }

    fn parsed_sections(normalized_source: &str) -> Vec<super::Section> {
        structured_sections(normalized_source)
    }

    fn bind_markdown_snapshot_filters() -> impl Drop {
        let mut settings = Settings::clone_current();
        settings.add_filter("  \n", "<HB>\n");
        settings.bind_to_scope()
    }
}
