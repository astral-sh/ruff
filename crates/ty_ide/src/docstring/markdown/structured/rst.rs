use ruff_text_size::Ranged;
use rustc_hash::FxHashMap;

use crate::docstring::document::rst;

use super::{Section, SectionItem, SectionKind};

/// Returns the portions of the docstring that describe canonical docstring data
/// (e.g., parameters, return value, etc.).
pub(super) fn structured_sections(source: &str) -> Vec<Section> {
    let mut sections = Vec::new();

    for field_list in rst::top_level_field_lists(source) {
        let Some(section) = RenderPlan::from_fields(field_list.fields())
            .and_then(|plan| Section::new(field_list.range(), plan.to_items()))
        else {
            continue;
        };

        sections.push(section);
    }

    sections
}

/// Validates a reST field list and stores cross-field metadata needed while rendering.
struct RenderPlan<'a> {
    fields: &'a [rst::Field],
    parameter_types: SupplementalTypeFields<'a>,
    attribute_types: SupplementalTypeFields<'a>,
    return_type: Option<&'a str>,
    has_returns: bool,
}

impl<'a> RenderPlan<'a> {
    /// Validates `fields` and builds a plan for structured rendering.
    ///
    /// This factory conservatively returns `None` when we aren't certain that
    /// we can interpret the field list correctly. This indicates to the caller
    /// that the field list should be left raw rather than replaced with
    /// Markdown.
    fn from_fields(fields: &'a [rst::Field]) -> Option<Self> {
        let mut has_returns = false;
        let mut parameter_types = SupplementalTypeFields::default();
        let mut attribute_types = SupplementalTypeFields::default();
        let mut return_type = None;

        for field in fields {
            match field {
                rst::Field::Parameter {
                    lookup_name, ty, ..
                } => {
                    parameter_types.record_value_field(lookup_name.as_str(), ty.is_some());
                }
                rst::Field::Attribute { name, ty, .. } => {
                    attribute_types.record_value_field(name.as_str(), ty.is_some());
                }
                rst::Field::Returns { .. } => {
                    has_returns = true;
                }
                rst::Field::Raises { .. }
                | rst::Field::ParameterType { .. }
                | rst::Field::AttributeType { .. } => {}
                rst::Field::ReturnType { ty } => {
                    let ty = ty.as_str();
                    if ty.is_empty() || return_type.replace(ty).is_some() {
                        return None;
                    }
                }
                rst::Field::Metadata { body } if body.is_empty() => {
                    // Sphinx metadata fields are not user-facing hover sections.
                }
                rst::Field::Metadata { .. } => return None,
                rst::Field::Unknown { .. } => {
                    // Unknown or unsupported fields may have section semantics we
                    // do not understand, so leave the full field list unstructured.
                    return None;
                }
            }
        }

        // Resolve supplemental types in a second pass so their matching value fields are known
        // even when the type fields appear first.
        for field in fields {
            match field {
                rst::Field::ParameterType { lookup_name, ty } => {
                    let name = lookup_name.as_str();
                    parameter_types.record_type_field(name, ty.as_str())?;
                }
                rst::Field::AttributeType { name, ty } => {
                    attribute_types.record_type_field(name.as_str(), ty.as_str())?;
                }
                _ => {}
            }
        }

        Some(Self {
            fields,
            parameter_types,
            attribute_types,
            return_type,
            has_returns,
        })
    }

    fn to_items(&self) -> Vec<SectionItem> {
        let mut items = Vec::new();

        for field in self.fields {
            match field {
                rst::Field::Parameter {
                    display_name,
                    lookup_name,
                    ty,
                    description,
                } => items.push(SectionItem::new(
                    SectionKind::Parameters,
                    Some(display_name.as_str()),
                    self.parameter_types
                        .type_for_value_field(lookup_name.as_str(), ty.as_deref()),
                    description.as_str(),
                )),
                rst::Field::Attribute {
                    name,
                    ty,
                    description,
                } => items.push(SectionItem::new(
                    SectionKind::Attributes,
                    Some(name.as_str()),
                    self.attribute_types
                        .type_for_value_field(name.as_str(), ty.as_deref()),
                    description.as_str(),
                )),
                rst::Field::Returns { name, description } => items.push(SectionItem::new(
                    SectionKind::Returns,
                    name.as_deref(),
                    self.return_type,
                    description.as_str(),
                )),
                rst::Field::Raises {
                    exception,
                    description,
                } => items.push(SectionItem::new(
                    SectionKind::Raises,
                    exception.as_deref(),
                    None,
                    description.as_str(),
                )),
                rst::Field::ReturnType { .. } if !self.has_returns => {
                    if let Some(return_type) = self.return_type {
                        items.push(SectionItem::new(
                            SectionKind::Returns,
                            None,
                            Some(return_type),
                            "",
                        ));
                    }
                }
                rst::Field::ParameterType { .. }
                | rst::Field::AttributeType { .. }
                | rst::Field::ReturnType { .. } => {
                    // Supplemental types render with their value fields.
                }
                rst::Field::Metadata { .. } => {
                    // Metadata is omitted because it isn't user-facing.
                }
                rst::Field::Unknown { .. } => {
                    // Required for exhaustiveness. `RenderPlan::from_fields` rejects this
                    // variant before constructing `Self`.
                }
            }
        }

        items
    }
}

/// Tracks `:type name:` fields that supplement matching value fields.
///
/// A separate type field is usable only when the corresponding value field
/// exists and did not already include an inline type.
#[derive(Default)]
struct SupplementalTypeFields<'a> {
    types: FxHashMap<&'a str, &'a str>,
    value_fields_accepting_type: FxHashMap<&'a str, bool>,
}

impl<'a> SupplementalTypeFields<'a> {
    /// Returns the type to render for a value field.
    ///
    /// reST allows types inline on the value field:
    ///
    /// ```python
    /// """
    /// :param str value: The value.
    /// """
    /// ```
    ///
    /// It also allows types in separate supplemental fields:
    ///
    /// ```python
    /// """
    /// :param value: The value.
    /// :type value: str
    /// """
    /// ```
    ///
    /// Inline types win; supplemental types are only used for matching fields
    /// without inline types.
    fn type_for_value_field(&self, name: &str, inline_ty: Option<&'a str>) -> Option<&'a str> {
        inline_ty.or_else(|| self.types.get(name).copied())
    }

    fn record_value_field(&mut self, name: &'a str, has_inline_type: bool) {
        self.value_fields_accepting_type
            .entry(name)
            .and_modify(|accepts_separate_type| *accepts_separate_type &= !has_inline_type)
            .or_insert(!has_inline_type);
    }

    fn record_type_field(&mut self, name: &'a str, ty: &'a str) -> Option<()> {
        (!ty.is_empty()
            && self.accepts_separate_type(name)
            && self.types.insert(name, ty).is_none())
        .then_some(())
    }

    fn accepts_separate_type(&self, name: &str) -> bool {
        self.value_fields_accepting_type
            .get(name)
            .is_some_and(|accepts_separate_type| *accepts_separate_type)
    }
}

#[cfg(test)]
mod tests {
    use insta::{Settings, assert_snapshot};

    use super::super::render_into;

    #[test]
    fn render_parameters_with_inline_and_supplemental_types() {
        let docstring = "\
Summary.

:param str value: The value.
:param other: Another value.
:type other: int
";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered, @"
        Summary.

        ## Parameters
        **value**: `str`  
        The value.

        **other**: `int`  
        Another value.
        ");
    }

    #[test]
    fn preserve_code_span_wrapped_type() {
        let docstring = "\
:param value: The value.
:type value: `str`
";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered, @"
        ## Parameters
        **value**: `str`  
        The value.
        ");
    }

    #[test]
    fn render_returns_with_supplemental_type() {
        let docstring = "\
Summary.

:returns: Whether validation passed.
:rtype: bool
";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered, @"
        Summary.

        ## Returns
        `bool`  
        Whether validation passed.
        ");
    }

    #[test]
    fn preserve_field_description_rst_directives() {
        let _snap = bind_markdown_snapshot_filters();
        let docstring = "\
:param value: The first sentence wraps
    before the directive.

    .. versionchanged:: 2.0
       Directive text keeps its own wrapping.

    The final sentence wraps
    after the directive.
";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered, @"
        ## Parameters
        **value**<HB>
        The first sentence wraps<HB>
        before the directive.

        **Changed in version 2.0:**<HB>
        Directive text keeps its own wrapping.<HB>
        <HB>
        The final sentence wraps<HB>
        after the directive.
        ");
    }

    #[test]
    fn preserve_duplicate_parameters() {
        let docstring = "\
:param value: Stale description.
:param value: Corrected description.
";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered, @"
        ## Parameters
        **value**  
        Stale description.

        **value**  
        Corrected description.
        ");
    }

    #[test]
    fn render_parameter_and_standalone_return_type() {
        let docstring = "\
Summary.

:param value: The value.
:rtype: str
";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered, @"
        Summary.

        ## Parameters
        **value**  
        The value.

        ## Returns
        `str`
        ");
    }

    #[test]
    fn render_standalone_return_type() {
        let docstring = "\
Summary.

:rtype: str
";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered, @"
        Summary.

        ## Returns
        `str`
        ");
    }

    #[test]
    fn preserve_inline_roles_in_prose() {
        let _snap = bind_markdown_snapshot_filters();
        let docstring = "\
This is a function description.
:class:`Foo` instances can be passed here.

:param value: The value.
";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered, @"
        This is a function description.<HB>
        :class:`Foo` instances can be passed here.

        ## Parameters
        **value**<HB>
        The value.
        ");
    }

    #[test]
    fn ignore_metadata_fields() {
        let docstring = "\
:meta private:

Leading prose.

:param value: The value.
:meta private:
:returns: The result.
:meta hide-value:

Trailing prose.

:meta private:
";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered, @"
        Leading prose.

        ## Parameters
        **value**  
        The value.

        ## Returns
        The result.

        Trailing prose.
        ");
    }

    #[test]
    fn render_parameter_aliases_and_variadics() {
        let docstring = "\
:param param: The parameter description.
:type param: int
:kwparam retries: Retry attempts.
:paramtype retries: int
:keyword timeout: Timeout in seconds.
:kwtype timeout: float
:param *args: Extra positional arguments.
:type args: tuple[str, ...]
:param **kwargs: Extra keyword arguments.
:type **kwargs: dict[str, object]
";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered, @r"
        ## Parameters
        **param**: `int`  
        The parameter description.

        **retries**: `int`  
        Retry attempts.

        **timeout**: `float`  
        Timeout in seconds.

        **\*args**: `tuple[str, ...]`  
        Extra positional arguments.

        **\*\*kwargs**: `dict[str, object]`  
        Extra keyword arguments.
        ");
    }

    #[test]
    fn render_field_names_case_insensitively() {
        let docstring = "\
:Param value: The value.
:TYPE value: str
:KeY option: The option.
:TyPe option: int
:KwPaRaM retries: Retry attempts.
:KwTyPe retries: int
:VaR state: Current state.
:VaRtYpE state: bool
:ReTuRnS: Whether validation passed.
:RTyPe: bool
:RaIsEs ValueError: If validation fails.
:MeTa private:
";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered);
    }

    #[test]
    fn render_attribute_aliases() {
        let docstring = "\
:var cache: Cached data.
:vartype cache: dict[str,
    object]
:ivar state: Instance state.
:var str title: Display title.
:cvar VERSION: Package version.
:vartype VERSION: str
";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered, @"
        ## Attributes
        **cache**: `dict[str, object]`  
        Cached data.

        **state**  
        Instance state.

        **title**: `str`  
        Display title.

        **VERSION**: `str`  
        Package version.
        ");
    }

    #[test]
    fn render_named_returns_and_exceptions() {
        let docstring = "\
:returns baz: The return value description
:rtype: dict[str,
    int]
:raises ValueError: If the value is invalid.
:exception RuntimeError: If the system is unavailable.";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered, @"
        ## Returns
        **baz**: `dict[str, int]`  
        The return value description

        ## Raises
        `ValueError`  
        If the value is invalid.

        `RuntimeError`  
        If the system is unavailable.
        ");
    }

    #[test]
    fn unstructured_field_lists_stay_raw() {
        for docstring in [
            // Recognized fields that would lose information or render no
            // visible item if converted structurally.
            // `:type orphan:` has no matching value field, so rendering the
            // field list structurally would drop the type information.
            "\
:param first: First parameter.
:type orphan: str
",
            // Malformed or body-bearing metadata may contain user-visible text,
            // so preserve the full field list rather than dropping it.
            "\
:param value: The value.
:meta: Preserve this field list.
",
            "\
:param value: The value.
:meta private: Preserve this field list.
",
            // Unsupported or ambiguous field lists.
            // `:unknown field:` is not a supported Sphinx-style field.
            "\
Summary.

:param value: The value.
:unknown field: Preserve this field list.
",
            // A malformed supported field must keep the surrounding contiguous
            // field list from being partially converted.
            "\
:param first: First parameter.
:param second:Missing whitespace before the body.
:param third: Third parameter.
",
            // Empty `:returns:` and `:raises:` fields would render as empty
            // section items.
            "\
Summary.

:returns:
:raises:
",
            // `:type value:` cannot supplement a parameter that already has
            // an inline type.
            "\
Summary.

:param str value: The value.
:type value: int
",
            // Duplicate `:type value:` fields make the supplemental type
            // ambiguous.
            "\
Summary.

:param value: The value.
:type value: str
:type value: int
",
            // The empty `:returns:` field would render as an empty section item.
            "\
Summary.

:param value: The value.
:returns:
",
            // Empty supplemental type fields would otherwise be dropped from
            // an otherwise renderable field list.
            "\
:param value: The value.
:type value:
",
            "\
:var value: The value.
:vartype value:
",
            "\
:param value: The value.
:rtype:
",
            // `:rtype:` does not accept an argument, which the structured
            // representation would otherwise drop.
            "\
:param value: The value.
:rtype result: int
",
            // A field list requires a preceding block boundary. Without one,
            // field-like lines remain part of the surrounding paragraph.
            "\
Summary.
:param value: This is paragraph text.
Continuation.
",
            "\
Summary.
:meta private:
Continuation.
",
            // Escaped colons belong to the field name. Leave the field list raw
            // rather than interpreting one as the end of the field header.
            r#"\
:param first: First parameter.
:param Literal["a\: b"] value: The value.
:param last: Last parameter.
"#,
        ] {
            let rendered = render_docstring(docstring);
            assert_eq!(rendered, render_general(docstring));
        }
    }

    #[test]
    fn preserve_preformatted_field_lists() {
        let _snap = bind_markdown_snapshot_filters();
        let docstring = "\
Markdown input:

```text
:param sample: This is sample input
```

Doctest output:

>>> print(\"field list\")
:param sample: This is sample output

Literal block::

    :param sample: This is sample input

:param quoted: Example::

:param sample: This is sample input
:returns: This is still sample input

:param second: Options:

    - First option.
        - Nested detail.
    - Second option.
:param third:
    1. Validate the input.
    2. Return the result.
:param done: Whether work is done.";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered, @r#"
        Markdown input:<HB>
        <HB>
        ```text
        :param sample: This is sample input
        ```<HB>
        <HB>
        Doctest output:<HB>
        <HB>
        ```````````python
        >>> print("field list")
        :param sample: This is sample output
        ```````````<HB>
        Literal block:  <HB>
        ```````````python
            :param sample: This is sample input
        ```````````

        ## Parameters
        **quoted**<HB>
        Example:

        **sample**<HB>
        This is sample input

        **second**<HB>
        Options:

        - First option.<HB>
            - Nested detail.<HB>
        - Second option.

        **third**

        1. Validate the input.<HB>
        2. Return the result.

        **done**<HB>
        Whether work is done.

        ## Returns
        This is still sample input
        "#);
    }

    #[test]
    fn render_field_like_quoted_literal_blocks() {
        let docstring = "\
Summary.

:param quoted: Example::

:param sample: This is sample input.
:returns: This is still sample input.

:param real: Real parameter.
";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered, @"
        Summary.

        ## Parameters
        **quoted**  
        Example:

        **sample**  
        This is sample input.

        **real**  
        Real parameter.

        ## Returns
        This is still sample input.
        ");
    }

    #[test]
    fn render_non_field_like_quoted_literal_blocks() {
        let _snap = bind_markdown_snapshot_filters();
        let docstring = "\
:param quoted: Example::

!param sample: This is sample input.
!returns: This is still sample input.

:param real: Real parameter.";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered, @"
        ## Parameters
        **quoted**<HB>
        Example:

        !param sample: This is sample input.<HB>
        !returns: This is still sample input.

        ## Parameters
        **real**<HB>
        Real parameter.
        ");
    }

    #[test]
    fn field_lists_in_block_quotes_remain_raw() {
        let docstring = "\
Summary.

    :param value: The value.
    :returns: Another value.
";
        let rendered = render_docstring(docstring);

        assert_eq!(rendered, render_general(docstring));
    }

    #[test]
    fn literal_backticks_do_not_swallow_the_following_field() {
        let rendered = render_docstring(
            "\
:param first: Example::

    ```
:param second: Visible parameter.",
        );

        assert_snapshot!(rendered, @"
        ## Parameters
        **first**  
        Example:

        ```````````python
        ```
        ```````````

        **second**  
        Visible parameter.
        ");
    }

    fn render_docstring(raw: &str) -> String {
        let mut output = String::new();
        render_into(&mut output, raw);
        output
    }

    fn render_general(raw: &str) -> String {
        let mut output = String::new();
        crate::docstring::markdown::general::render_into(&mut output, raw);
        output
    }

    fn bind_markdown_snapshot_filters() -> impl Drop {
        let mut settings = Settings::clone_current();
        settings.add_filter("  \n", "<HB>\n");
        settings.bind_to_scope()
    }
}
