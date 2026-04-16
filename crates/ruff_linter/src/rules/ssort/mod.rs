//! Rules from [ssort](https://pypi.org/project/ssort).

mod dependencies;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::assert_diagnostics;
    use crate::registry::Rule;
    use crate::rules::ssort::settings::Order;
    use crate::settings::LinterSettings;
    use crate::test::test_path;

    #[test_case(Path::new("async_function.py"), Order::Newspaper)]
    #[test_case(Path::new("async_function.py"), Order::Narrative)]
    #[test_case(Path::new("attribute_assign_class.py"), Order::Newspaper)]
    #[test_case(Path::new("attribute_assign_class.py"), Order::Narrative)]
    #[test_case(Path::new("class_non_self_use.py"), Order::Newspaper)]
    #[test_case(Path::new("class_non_self_use.py"), Order::Narrative)]
    #[test_case(Path::new("comments_inside_dependency.py"), Order::Newspaper)]
    #[test_case(Path::new("comments_inside_dependency.py"), Order::Narrative)]
    #[test_case(Path::new("concat.py"), Order::Newspaper)]
    #[test_case(Path::new("concat.py"), Order::Narrative)]
    #[test_case(Path::new("dependency_order.py"), Order::Newspaper)]
    #[test_case(Path::new("dependency_order.py"), Order::Narrative)]
    #[test_case(Path::new("empty.py"), Order::Newspaper)]
    #[test_case(Path::new("empty.py"), Order::Narrative)]
    #[test_case(Path::new("format_string.py"), Order::Newspaper)]
    #[test_case(Path::new("format_string.py"), Order::Narrative)]
    #[test_case(Path::new("generic_class_method_locals.py"), Order::Newspaper)]
    #[test_case(Path::new("generic_class_method_locals.py"), Order::Narrative)]
    #[test_case(Path::new("generic_function.py"), Order::Newspaper)]
    #[test_case(Path::new("generic_function.py"), Order::Narrative)]
    #[test_case(Path::new("generic_function_inner.py"), Order::Newspaper)]
    #[test_case(Path::new("generic_function_inner.py"), Order::Narrative)]
    #[test_case(Path::new("global_scope_conflict_dict_comp.py"), Order::Newspaper)]
    #[test_case(Path::new("global_scope_conflict_dict_comp.py"), Order::Narrative)]
    #[test_case(
        Path::new("global_scope_conflict_generator_expression.py"),
        Order::Newspaper
    )]
    #[test_case(
        Path::new("global_scope_conflict_generator_expression.py"),
        Order::Narrative
    )]
    #[test_case(Path::new("global_scope_conflict_list_comp.py"), Order::Newspaper)]
    #[test_case(Path::new("global_scope_conflict_list_comp.py"), Order::Narrative)]
    #[test_case(Path::new("global_scope_conflict_set_comp.py"), Order::Newspaper)]
    #[test_case(Path::new("global_scope_conflict_set_comp.py"), Order::Narrative)]
    #[test_case(Path::new("inner_class.py"), Order::Newspaper)]
    #[test_case(Path::new("inner_class.py"), Order::Narrative)]
    #[test_case(Path::new("iter_unpack_in_class.py"), Order::Newspaper)]
    #[test_case(Path::new("iter_unpack_in_class.py"), Order::Narrative)]
    #[test_case(Path::new("mixed_runtime_initialisation.py"), Order::Newspaper)]
    #[test_case(Path::new("mixed_runtime_initialisation.py"), Order::Narrative)]
    #[test_case(Path::new("nested_class.py"), Order::Newspaper)]
    #[test_case(Path::new("nested_class.py"), Order::Narrative)]
    #[test_case(Path::new("pretend_dunder_properties.py"), Order::Newspaper)]
    #[test_case(Path::new("pretend_dunder_properties.py"), Order::Narrative)]
    #[test_case(Path::new("simple_decorator.py"), Order::Newspaper)]
    #[test_case(Path::new("simple_decorator.py"), Order::Narrative)]
    #[test_case(Path::new("simple_dependency.py"), Order::Newspaper)]
    #[test_case(Path::new("simple_dependency.py"), Order::Narrative)]
    #[test_case(Path::new("simple_dependency_compact_formatting.py"), Order::Newspaper)]
    #[test_case(Path::new("simple_dependency_compact_formatting.py"), Order::Narrative)]
    #[test_case(Path::new("single_comment.py"), Order::Newspaper)]
    #[test_case(Path::new("single_comment.py"), Order::Narrative)]
    #[test_case(Path::new("single_line_dummy_class.py"), Order::Newspaper)]
    #[test_case(Path::new("single_line_dummy_class.py"), Order::Narrative)]
    #[test_case(Path::new("single_line_dummy_function.py"), Order::Newspaper)]
    #[test_case(Path::new("single_line_dummy_function.py"), Order::Narrative)]
    #[test_case(Path::new("slots.py"), Order::Newspaper)]
    #[test_case(Path::new("slots.py"), Order::Narrative)]
    #[test_case(Path::new("template_string.py"), Order::Newspaper)]
    #[test_case(Path::new("template_string.py"), Order::Narrative)]
    #[test_case(Path::new("top_level_statement.py"), Order::Newspaper)]
    #[test_case(Path::new("top_level_statement.py"), Order::Narrative)]
    #[test_case(Path::new("type_alias.py"), Order::Newspaper)]
    #[test_case(Path::new("type_alias.py"), Order::Narrative)]
    fn default(path: &Path, order: Order) -> Result<()> {
        let snapshot = format!(
            "{}_{}",
            path.to_string_lossy(),
            if order == Order::Narrative {
                "narrative"
            } else {
                "dependency"
            }
        );
        let mut settings = LinterSettings::for_rule(Rule::UnsortedStatements);
        settings.ssort.order = order;
        let diagnostics = test_path(Path::new("ssort").join(path).as_path(), &settings)?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }
}
