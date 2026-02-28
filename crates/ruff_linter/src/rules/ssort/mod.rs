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
    use crate::settings::LinterSettings;
    use crate::test::test_path;

    #[test_case(Path::new("async_function.py"), false)]
    #[test_case(Path::new("async_function.py"), true)]
    #[test_case(Path::new("attribute_assign_class.py"), false)]
    #[test_case(Path::new("attribute_assign_class.py"), true)]
    #[test_case(Path::new("class_non_self_use.py"), false)]
    #[test_case(Path::new("class_non_self_use.py"), true)]
    #[test_case(Path::new("comments_inside_dependency.py"), false)]
    #[test_case(Path::new("comments_inside_dependency.py"), true)]
    #[test_case(Path::new("concat.py"), false)]
    #[test_case(Path::new("concat.py"), true)]
    #[test_case(Path::new("dependency_order.py"), false)]
    #[test_case(Path::new("dependency_order.py"), true)]
    #[test_case(Path::new("empty.py"), false)]
    #[test_case(Path::new("empty.py"), true)]
    #[test_case(Path::new("format_string.py"), false)]
    #[test_case(Path::new("format_string.py"), true)]
    #[test_case(Path::new("generic_class_method_locals.py"), false)]
    #[test_case(Path::new("generic_class_method_locals.py"), true)]
    #[test_case(Path::new("generic_function.py"), false)]
    #[test_case(Path::new("generic_function.py"), true)]
    #[test_case(Path::new("generic_function_inner.py"), false)]
    #[test_case(Path::new("generic_function_inner.py"), true)]
    #[test_case(Path::new("global_scope_conflict_dict_comp.py"), false)]
    #[test_case(Path::new("global_scope_conflict_dict_comp.py"), true)]
    #[test_case(Path::new("global_scope_conflict_generator_expression.py"), false)]
    #[test_case(Path::new("global_scope_conflict_generator_expression.py"), true)]
    #[test_case(Path::new("global_scope_conflict_list_comp.py"), false)]
    #[test_case(Path::new("global_scope_conflict_list_comp.py"), true)]
    #[test_case(Path::new("global_scope_conflict_set_comp.py"), false)]
    #[test_case(Path::new("global_scope_conflict_set_comp.py"), true)]
    #[test_case(Path::new("inner_class.py"), false)]
    #[test_case(Path::new("inner_class.py"), true)]
    #[test_case(Path::new("iter_unpack_in_class.py"), false)]
    #[test_case(Path::new("iter_unpack_in_class.py"), true)]
    #[test_case(Path::new("mixed_runtime_initialisation.py"), false)]
    #[test_case(Path::new("mixed_runtime_initialisation.py"), true)]
    #[test_case(Path::new("nested_class.py"), false)]
    #[test_case(Path::new("nested_class.py"), true)]
    #[test_case(Path::new("pretend_dunder_properties.py"), false)]
    #[test_case(Path::new("pretend_dunder_properties.py"), true)]
    #[test_case(Path::new("simple_decorator.py"), false)]
    #[test_case(Path::new("simple_decorator.py"), true)]
    #[test_case(Path::new("simple_dependency.py"), false)]
    #[test_case(Path::new("simple_dependency.py"), true)]
    #[test_case(Path::new("simple_dependency_compact_formatting.py"), false)]
    #[test_case(Path::new("simple_dependency_compact_formatting.py"), true)]
    #[test_case(Path::new("single_comment.py"), false)]
    #[test_case(Path::new("single_comment.py"), true)]
    #[test_case(Path::new("single_line_dummy_class.py"), false)]
    #[test_case(Path::new("single_line_dummy_class.py"), true)]
    #[test_case(Path::new("single_line_dummy_function.py"), false)]
    #[test_case(Path::new("single_line_dummy_function.py"), true)]
    #[test_case(Path::new("slots.py"), false)]
    #[test_case(Path::new("slots.py"), true)]
    #[test_case(Path::new("template_string.py"), false)]
    #[test_case(Path::new("template_string.py"), true)]
    #[test_case(Path::new("top_level_statement.py"), false)]
    #[test_case(Path::new("top_level_statement.py"), true)]
    #[test_case(Path::new("type_alias.py"), false)]
    #[test_case(Path::new("type_alias.py"), true)]
    fn default(path: &Path, narrative_order: bool) -> Result<()> {
        let snapshot = format!(
            "{}_{}",
            path.to_string_lossy(),
            if narrative_order {
                "narrative"
            } else {
                "dependency"
            }
        );
        let mut settings = LinterSettings::for_rule(Rule::UnsortedStatements);
        settings.ssort.narrative_order = narrative_order;
        let diagnostics = test_path(Path::new("ssort").join(path).as_path(), &settings)?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }
}
