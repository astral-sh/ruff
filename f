    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.07s
    Starting 420 tests across 2 binaries (36 skipped; run ID: 2725b2f2-8ffa-4189-a51b-09ecf8cc1196, nextest profile: default)
        PASS [   0.023s] ty_python_semantic ast_node_ref::tests::debug
        PASS [   0.023s] ty_python_semantic ast_node_ref::tests::inequality
        PASS [   0.023s] ty_python_semantic ast_node_ref::tests::equality
        PASS [   0.023s] ty_python_semantic list::tests::can_insert_into_set
        PASS [   0.023s] ty_python_semantic list::tests::can_insert_into_map
        PASS [   0.023s] ty_python_semantic list::tests::can_intersect_maps
        PASS [   0.023s] ty_python_semantic list::tests::can_intersect_sets
        PASS [   0.023s] ty_python_semantic module_resolver::module::tests::known_module_roundtrip_from_str
        PASS [   0.022s] ty_python_semantic module_resolver::path::tests::mocked_typeshed_existing_namespace_stdlib_pkg_py39
        PASS [   0.022s] ty_python_semantic module_resolver::path::tests::mocked_typeshed_existing_namespace_stdlib_pkg_py38
        PASS [   0.022s] ty_python_semantic module_resolver::path::tests::mocked_typeshed_existing_regular_stdlib_pkg_py38
        PASS [   0.022s] ty_python_semantic module_resolver::path::tests::mocked_typeshed_existing_regular_stdlib_pkgs_py39
        PASS [   0.022s] ty_python_semantic module_resolver::path::tests::mocked_typeshed_nonexistent_regular_stdlib_pkg_py38
        PASS [   0.022s] ty_python_semantic module_resolver::path::tests::mocked_typeshed_nonexistent_namespace_stdlib_pkg_py39
        PASS [   0.022s] ty_python_semantic module_resolver::path::tests::mocked_typeshed_nonexistent_namespace_stdlib_pkg_py38
        PASS [   0.022s] ty_python_semantic module_resolver::path::tests::mocked_typeshed_nonexistent_single_file_module_py38
        PASS [   0.022s] ty_python_semantic module_resolver::path::tests::mocked_typeshed_single_file_stdlib_module_py38
        PASS [   0.024s] ty_python_semantic module_resolver::path::tests::module_name_1_part
        PASS [   0.005s] ty_python_semantic module_resolver::path::tests::module_name_2_parts
        PASS [   0.005s] ty_python_semantic module_resolver::path::tests::module_name_3_parts
        PASS [   0.006s] ty_python_semantic module_resolver::path::tests::relativize_path
        PASS [   0.006s] ty_python_semantic module_resolver::path::tests::stdlib_path_invalid_join_py
        PASS [   0.006s] ty_python_semantic module_resolver::path::tests::relativize_stdlib_path_errors
        PASS [   0.006s] ty_python_semantic module_resolver::path::tests::non_stdlib_path_invalid_join_rs
        PASS [   0.005s] ty_python_semantic module_resolver::path::tests::with_extension_methods
        PASS [   0.005s] ty_python_semantic module_resolver::path::tests::too_many_extensions
        PASS [   0.006s] ty_python_semantic module_resolver::path::tests::relativize_non_stdlib_path_errors
        PASS [   0.006s] ty_python_semantic module_resolver::path::tests::stdlib_path_invalid_join_rs
        PASS [   0.006s] ty_python_semantic module_resolver::resolver::tests::adding_file_to_search_path_with_lower_priority_does_not_invalidate_query
        PASS [   0.006s] ty_python_semantic module_resolver::resolver::tests::builtins_custom
        PASS [   0.006s] ty_python_semantic module_resolver::resolver::tests::adding_file_to_search_path_with_higher_priority_invalidates_the_query
        PASS [   0.006s] ty_python_semantic module_resolver::resolver::tests::adding_file_on_which_module_resolution_depends_invalidates_previously_failing_query_that_now_succeeds
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::deleting_editable_install_on_which_module_resolution_depends_invalidates_cache
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::case_sensitive_resolution_with_symlinked_directory
        PASS [   0.006s] ty_python_semantic module_resolver::resolver::tests::editable_install_absolute_path
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::builtins_vendored
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::deleting_file_from_higher_priority_search_path_invalidates_the_query
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::deleting_an_unrelated_file_doesnt_change_module_resolution
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::deleting_pth_file_on_which_module_resolution_depends_invalidates_cache
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::file_to_module_where_one_search_path_is_subdirectory_of_other
        PASS [   0.006s] ty_python_semantic module_resolver::resolver::tests::module_resolution_paths_cached_between_different_module_resolutions
        PASS [   0.006s] ty_python_semantic module_resolver::resolver::tests::no_duplicate_search_paths_added
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::editable_install_pth_file_with_whitespace
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::editable_install_multiple_pth_files_with_multiple_paths
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::editable_install_relative_path
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::module_search_path_priority
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::first_party_precedence_over_stdlib
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::first_party_module
        PASS [   0.006s] ty_python_semantic module_resolver::resolver::tests::namespace_package
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::multiple_site_packages_with_editables
        PASS [   0.006s] ty_python_semantic module_resolver::resolver::tests::package_priority_over_module
        PASS [   0.005s] ty_python_semantic module_resolver::resolver::tests::resolve_package
        PASS [   0.006s] ty_python_semantic module_resolver::resolver::tests::regular_package_in_namespace_package
        PASS [   0.003s] ty_python_semantic module_resolver::typeshed::tests::invalid_typeshed_versions_bad_colon_number
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::removing_file_on_which_module_resolution_depends_invalidates_previously_successful_query_that_now_fails
        PASS [   0.004s] ty_python_semantic module_resolver::typeshed::tests::implicit_submodule_queried_correctly
        PASS [   0.004s] ty_python_semantic module_resolver::typeshed::tests::explicit_submodule_parsed_correctly
        PASS [   0.003s] ty_python_semantic module_resolver::typeshed::tests::invalid_typeshed_versions_bad_period_number
        PASS [   0.003s] ty_python_semantic module_resolver::typeshed::tests::invalid_typeshed_versions_bad_hyphen_number
        PASS [   0.006s] ty_python_semantic module_resolver::resolver::tests::stdlib_resolution_respects_versions_file_py39_nonexisting_modules
        PASS [   0.006s] ty_python_semantic module_resolver::resolver::tests::stdlib_resolution_respects_versions_file_py38_existing_modules
        PASS [   0.006s] ty_python_semantic module_resolver::resolver::tests::stdlib_resolution_respects_versions_file_py38_nonexisting_modules
        PASS [   0.006s] ty_python_semantic module_resolver::resolver::tests::stdlib
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::single_file_takes_priority_over_namespace_package
        PASS [   0.003s] ty_python_semantic module_resolver::typeshed::tests::nonexistent_module_queried_correctly
        PASS [   0.003s] ty_python_semantic module_resolver::typeshed::tests::invalid_typeshed_versions_non_identifier_modules
        PASS [   0.005s] ty_python_semantic module_resolver::typeshed::tests::invalid_typeshed_versions_non_digits
        PASS [   0.006s] ty_python_semantic module_resolver::resolver::tests::symlink
        PASS [   0.008s] ty_python_semantic module_resolver::resolver::tests::stdlib_resolution_respects_versions_file_py39_existing_modules
        PASS [   0.004s] ty_python_semantic module_resolver::typeshed::tests::version_within_range_parsed_correctly
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::sub_packages
        PASS [   0.007s] ty_python_semantic module_resolver::resolver::tests::stdlib_uses_vendored_typeshed_when_no_custom_typeshed_supplied
        PASS [   0.004s] ty_python_semantic module_resolver::typeshed::tests::version_from_range_parsed_correctly
        PASS [   0.006s] ty_python_semantic module_resolver::resolver::tests::typing_stub_over_module
        PASS [   0.006s] ty_python_semantic module_resolver::typeshed::tests::can_parse_vendored_versions_file
        PASS [   0.006s] ty_python_semantic module_resolver::typeshed::tests::typeshed_versions_consistent_with_vendored_stubs
        PASS [   0.006s] ty_python_semantic semantic_index::tests::augmented_assignment
        PASS [   0.006s] ty_python_semantic semantic_index::tests::comprehension_scope
        PASS [   0.006s] ty_python_semantic semantic_index::tests::dupes
        PASS [   0.007s] ty_python_semantic semantic_index::tests::annotation_only
        PASS [   0.007s] ty_python_semantic semantic_index::tests::assign
        PASS [   0.007s] ty_python_semantic semantic_index::tests::class_scope
        PASS [   0.003s] ty_python_semantic semantic_index::use_def::symbol_state::tests::merge
        PASS [   0.007s] ty_python_semantic semantic_index::tests::empty
        PASS [   0.007s] ty_python_semantic semantic_index::tests::expression_scope
        PASS [   0.007s] ty_python_semantic semantic_index::tests::function_parameter_symbols
        PASS [   0.007s] ty_python_semantic semantic_index::tests::for_loops_simple_unpacking
        PASS [   0.007s] ty_python_semantic semantic_index::tests::for_loops_single_assignment
        PASS [   0.007s] ty_python_semantic semantic_index::tests::for_loops_complex_unpacking
        PASS [   0.003s] ty_python_semantic semantic_index::use_def::symbol_state::tests::no_declaration
        PASS [   0.003s] ty_python_semantic semantic_index::use_def::symbol_state::tests::record_declaration_override
        PASS [   0.003s] ty_python_semantic semantic_index::use_def::symbol_state::tests::record_declaration_merge_partial_undeclared
        PASS [   0.004s] ty_python_semantic semantic_index::use_def::symbol_state::tests::record_declaration
        PASS [   0.004s] ty_python_semantic semantic_index::use_def::symbol_state::tests::record_constraint
        PASS [   0.004s] ty_python_semantic semantic_index::use_def::symbol_state::tests::record_declaration_merge
        PASS [   0.008s] ty_python_semantic semantic_index::tests::generic_class
        PASS [   0.007s] ty_python_semantic semantic_index::tests::import
        PASS [   0.007s] ty_python_semantic semantic_index::tests::import_as
        PASS [   0.004s] ty_python_semantic semantic_index::use_def::symbol_state::tests::with
        PASS [   0.009s] ty_python_semantic semantic_index::tests::lambda_parameter_symbols
        PASS [   0.008s] ty_python_semantic semantic_index::tests::multiple_generators
        PASS [   0.008s] ty_python_semantic semantic_index::tests::reachability_trivial
        PASS [   0.004s] ty_python_semantic semantic_index::use_def::symbol_state::tests::unbound
        PASS [   0.008s] ty_python_semantic semantic_index::tests::nested_match_case
        PASS [   0.009s] ty_python_semantic semantic_index::tests::generic_function
        PASS [   0.008s] ty_python_semantic semantic_index::tests::scope_iterators
        PASS [   0.009s] ty_python_semantic semantic_index::tests::import_sub
        PASS [   0.008s] ty_python_semantic semantic_index::tests::nested_generators
        PASS [   0.009s] ty_python_semantic semantic_index::tests::import_from
        PASS [   0.008s] ty_python_semantic semantic_index::tests::simple
        PASS [   0.010s] ty_python_semantic semantic_index::tests::function_scope
        PASS [   0.008s] ty_python_semantic semantic_index::tests::with_item_definition
        PASS [   0.008s] ty_python_semantic semantic_index::tests::match_stmt
        PASS [   0.004s] ty_python_semantic site_packages::tests::can_find_site_packages_directory_no_version_field_in_pyvenv_cfg
        PASS [   0.004s] ty_python_semantic site_packages::tests::can_find_site_packages_directory_venv_style_version_field_in_pyvenv_cfg
        PASS [   0.004s] ty_python_semantic site_packages::tests::can_find_site_packages_directory_virtualenv_style_version_field_in_pyvenv_cfg
        PASS [   0.005s] ty_python_semantic site_packages::tests::can_find_site_packages_directory_freethreaded_build
        PASS [   0.010s] ty_python_semantic semantic_index::tests::with_item_unpacked_definition
        PASS [   0.004s] ty_python_semantic site_packages::tests::can_find_site_packages_directory_uv_style_version_field_in_pyvenv_cfg
        PASS [   0.003s] ty_python_semantic site_packages::tests::parsing_pyvenv_cfg_with_key_but_no_value_fails
        PASS [   0.004s] ty_python_semantic site_packages::tests::finds_system_site_packages
        PASS [   0.003s] ty_python_semantic site_packages::tests::parsing_pyvenv_cfg_with_invalid_home_key_fails
        PASS [   0.003s] ty_python_semantic site_packages::tests::parsing_pyvenv_cfg_with_value_but_no_key_fails
        PASS [   0.005s] ty_python_semantic site_packages::tests::parsing_pyvenv_cfg_with_no_home_key_fails
        PASS [   0.004s] ty_python_semantic site_packages::tests::reject_venv_with_no_pyvenv_cfg_file
        PASS [   0.004s] ty_python_semantic site_packages::tests::parsing_pyvenv_cfg_with_too_many_equals
        PASS [   0.007s] ty_python_semantic semantic_model::tests::class_type
        PASS [   0.004s] ty_python_semantic site_packages::tests::reject_venv_that_is_not_a_directory
        PASS [   0.004s] ty_python_semantic site_packages::tests::reject_venv_that_does_not_exist
        PASS [   0.007s] ty_python_semantic semantic_model::tests::function_type
        PASS [   0.008s] ty_python_semantic semantic_model::tests::alias_type
        PASS [   0.008s] ty_python_semantic symbol::tests::test_symbol_or_fall_back_to
        PASS [   0.007s] ty_python_semantic types::builder::tests::build_union_single_element
        PASS [   0.007s] ty_python_semantic types::builder::tests::build_union_no_elements
        PASS [   0.007s] ty_python_semantic types::builder::tests::build_union_two_elements
        PASS [   0.007s] ty_python_semantic types::display::tests::string_literal_display
        PASS [   0.010s] ty_python_semantic types::class::tests::known_class_roundtrip_from_str
        PASS [   0.030s] ty_python_semantic module_resolver::typeshed::tests::can_parse_mock_versions_file
        PASS [   0.024s] ty_python_semantic symbol::implicit_globals::tests::module_type_symbols_includes_declared_types_but_not_referenced_types
        PASS [   0.029s] ty_python_semantic suppression::tests::invalid_type_ignore_valid_type_ignore
        PASS [   0.030s] ty_python_semantic suppression::tests::type_ignore_explanation
        PASS [   0.030s] ty_python_semantic suppression::tests::type_ignore_before_fmt_off
        PASS [   0.031s] ty_python_semantic suppression::tests::multiple_type_ignore_comments
        PASS [   0.032s] ty_python_semantic suppression::tests::fmt_comment_before_type_ignore
        PASS [   0.032s] ty_python_semantic suppression::tests::valid_type_ignore_invalid_type_ignore
        PASS [   0.032s] ty_python_semantic suppression::tests::type_ignore_multiple_codes
        PASS [   0.032s] ty_python_semantic suppression::tests::type_ignore_no_codes
        PASS [   0.033s] ty_python_semantic suppression::tests::type_ignore_single_code
        PASS [   0.008s] ty_python_semantic types::signatures::tests::empty
        PASS [   0.008s] ty_python_semantic types::signatures::tests::generic_not_deferred
        PASS [   0.011s] ty_python_semantic types::signatures::tests::deferred_in_stub
        PASS [   0.009s] ty_python_semantic types::signatures::tests::generic_deferred_in_stub
        PASS [   0.008s] ty_python_semantic types::signatures::tests::not_deferred
        PASS [   0.073s] ty_python_semantic types::builder::tests::build_intersection_empty_intersection_equals_object
        PASS [   0.082s] ty_python_semantic types::infer::tests::dependency_unrelated_symbol
        PASS [   0.095s] ty_python_semantic types::builder::tests::build_intersection_simplify_split_bool::type_alwaystruthy_expects
        PASS [   0.095s] ty_python_semantic types::builder::tests::build_intersection_simplify_split_bool::type_booleanliteral_false_expects
        PASS [   0.005s] ty_python_semantic util::subscript::tests::py_index_empty
        PASS [   0.094s] ty_python_semantic types::infer::tests::dependency_internal_symbol_change
        PASS [   0.009s] ty_python_semantic util::subscript::tests::py_index_more_elements
        PASS [   0.005s] ty_python_semantic util::subscript::tests::py_index_uses_full_index_range
        PASS [   0.008s] ty_python_semantic util::subscript::tests::py_index_single_element
        PASS [   0.005s] ty_python_semantic util::subscript::tests::py_slice_empty_input
        PASS [   0.002s] ty_python_semantic util::subscript::tests::py_slice_nonnegative_indices
        PASS [   0.003s] ty_python_semantic util::subscript::tests::py_slice_single_element_input
        PASS [   0.086s] ty_python_semantic types::signatures::tests::external_signature_no_decorator
        PASS [   0.009s] ty_python_semantic util::subscript::tests::py_slice_mixed_positive_negative_indices
        PASS [   0.019s] ty_python_semantic util::subscript::tests::py_slice_negatice_indices
        PASS [   0.118s] ty_python_semantic types::infer::tests::dependency_public_symbol_type_change
        PASS [   0.012s] ty_python_semantic util::subscript::tests::py_slice_step_backward
        PASS [   0.088s] ty_python_semantic types::tests::call_type_doesnt_rerun_when_only_callee_changed
        PASS [   0.013s] ty_python_semantic util::subscript::tests::py_slice_step_forward
        PASS [   0.127s] ty_python_semantic types::display::tests::signature_display
        PASS [   0.129s] ty_python_semantic types::builder::tests::build_intersection_simplify_split_bool::type_alwaysfalsy_expects
        PASS [   0.131s] ty_python_semantic types::infer::tests::adding_string_literals_and_literal_string
        PASS [   0.138s] ty_python_semantic symbol::tests::implicit_sys_globals
        PASS [   0.130s] ty_python_semantic types::infer::tests::multiplied_string
        PASS [   0.131s] ty_python_semantic types::infer::tests::multiplied_literal_string
        PASS [   0.147s] ty_python_semantic symbol::tests::implicit_typing_globals
        PASS [   0.147s] ty_python_semantic types::builder::tests::build_intersection_simplify_split_bool::type_booleanliteral_true_expects
        PASS [   0.149s] ty_python_semantic symbol::tests::implicit_builtin_globals
        PASS [   0.168s] ty_python_semantic module_resolver::typeshed::tests::invalid_huge_versions_file
        PASS [   0.124s] ty_python_semantic types::infer::tests::truncated_string_literals_become_literal_string
        PASS [   0.152s] ty_python_semantic types::display::tests::synthesized_protocol_display
        PASS [   0.086s] ty_python_semantic types::tests::todo_types
        PASS [   0.161s] ty_python_semantic symbol::tests::implicit_typing_extensions_globals
        PASS [   0.117s] ty_python_semantic types::tests::no_default_type_is_singleton::pythonversion_py313_expects
        PASS [   0.157s] ty_python_semantic types::infer::tests::dependency_implicit_instance_attribute
        PASS [   0.129s] ty_python_semantic types::tests::no_default_type_is_singleton::pythonversion_py312_expects
        PASS [   0.161s] ty_python_semantic types::infer::tests::dependency_own_instance_member
        PASS [   0.151s] ty_python_semantic types::infer::tests::not_literal_string
        PASS [   0.150s] ty_python_semantic types::infer::tests::pep695_type_params
        PASS [   0.154s] ty_python_semantic types::infer::tests::unbound_symbol_no_visibility_constraint_check
        PASS [   0.154s] ty_python_semantic types::signatures::tests::full
        PASS [   0.154s] ty_python_semantic types::tests::known_function_roundtrip_from_str
        PASS [   0.215s] ty_python_semantic types::class::tests::known_class_doesnt_fallback_to_unknown_unexpectedly_on_latest_version
        PASS [   0.154s] ty_python_semantic types::tests::typing_vs_typeshed_no_default
        PASS [   0.153s] ty_python_semantic::mdtest mdtest__annotations_int_float_complex
        PASS [   0.164s] ty_python_semantic::mdtest mdtest__annotations_new_types
        PASS [   0.177s] ty_python_semantic::mdtest mdtest__annotations_annotated
        PASS [   0.144s] ty_python_semantic::mdtest mdtest__assignment_multi_target
        PASS [   0.158s] ty_python_semantic::mdtest mdtest__assignment_walrus
        PASS [   0.163s] ty_python_semantic::mdtest mdtest__assignment_unbound
        PASS [   0.179s] ty_python_semantic::mdtest mdtest__annotations_unsupported_type_qualifiers
        PASS [   0.193s] ty_python_semantic::mdtest mdtest__annotations_starred
        FAIL [   0.201s] ty_python_semantic::mdtest mdtest__annotations_optional

--- STDOUT:              ty_python_semantic::mdtest mdtest__annotations_optional ---

running 1 test

optional.md - Optional - Typing Extensions

  crates/ty_python_semantic/resources/mdtest/annotations/optional.md:46 unmatched assertion: revealed: int | None
  crates/ty_python_semantic/resources/mdtest/annotations/optional.md:46 unexpected error: 17 [revealed-type] "Revealed type: `int | Unknown`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='optional.md - Optional - Typing Extensions'
MDTEST_TEST_FILTER='optional.md - Optional - Typing Extensions' cargo test -p ty_python_semantic --test mdtest -- mdtest__annotations_optional

--------------------------------------------------

test mdtest__annotations_optional ... FAILED

failures:

failures:
    mdtest__annotations_optional

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.19s


--- STDERR:              ty_python_semantic::mdtest mdtest__annotations_optional ---

thread 'mdtest__annotations_optional' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.214s] ty_python_semantic::mdtest mdtest__annotations_literal
        PASS [   0.221s] ty_python_semantic::mdtest mdtest__annotations_any
        PASS [   0.199s] ty_python_semantic::mdtest mdtest__annotations_unsupported_special_types
        PASS [   0.190s] ty_python_semantic::mdtest mdtest__binary_booleans
        FAIL [   0.224s] ty_python_semantic::mdtest mdtest__annotations_self

--- STDOUT:              ty_python_semantic::mdtest mdtest__annotations_self ---

running 1 test

self.md - Self - Generic Classes

  crates/ty_python_semantic/resources/mdtest/annotations/self.md:114 unexpected error: [invalid-return-type] "Return type does not match returned value: Expected `Self`, found `Self`"
  crates/ty_python_semantic/resources/mdtest/annotations/self.md:118 unmatched assertion: revealed: Container[int]
  crates/ty_python_semantic/resources/mdtest/annotations/self.md:118 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='self.md - Self - Generic Classes'
MDTEST_TEST_FILTER='self.md - Self - Generic Classes' cargo test -p ty_python_semantic --test mdtest -- mdtest__annotations_self

--------------------------------------------------

test mdtest__annotations_self ... FAILED

failures:

failures:
    mdtest__annotations_self

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.21s


--- STDERR:              ty_python_semantic::mdtest mdtest__annotations_self ---

thread 'mdtest__annotations_self' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.240s] ty_python_semantic::mdtest mdtest__annotations_invalid
        PASS [   0.224s] ty_python_semantic::mdtest mdtest__annotations_stdlib_typing_aliases
        PASS [   0.243s] ty_python_semantic::mdtest mdtest__annotations_literal_string
        PASS [   0.236s] ty_python_semantic::mdtest mdtest__assignment_augmented
        PASS [   0.172s] ty_python_semantic::mdtest mdtest__binary_tuples
        PASS [   0.233s] ty_python_semantic::mdtest mdtest__binary_integers
        PASS [   0.291s] ty_python_semantic::mdtest mdtest__binary_classes
        PASS [   0.170s] ty_python_semantic::mdtest mdtest__call_annotation
        PASS [   0.185s] ty_python_semantic::mdtest mdtest__boolean_short_circuit
        PASS [   0.304s] ty_python_semantic::mdtest mdtest__binary_custom
        PASS [   0.211s] ty_python_semantic::mdtest mdtest__binary_unions
        PASS [   0.345s] ty_python_semantic::mdtest mdtest__annotations_unsupported_special_forms
        PASS [   0.206s] ty_python_semantic::mdtest mdtest__boundness_declaredness_public
        PASS [   0.358s] ty_python_semantic::mdtest mdtest__annotations_union
        PASS [   0.154s] ty_python_semantic::mdtest mdtest__call_never
        PASS [   0.178s] ty_python_semantic::mdtest mdtest__call_invalid_syntax
        PASS [   0.200s] ty_python_semantic::mdtest mdtest__call_builtins
        PASS [   0.193s] ty_python_semantic::mdtest mdtest__call_str_startswith
        PASS [   0.217s] ty_python_semantic::mdtest mdtest__call_getattr_static
        PASS [   0.163s] ty_python_semantic::mdtest mdtest__comparison_byte_literals
        PASS [   0.459s] ty_python_semantic::mdtest mdtest__annotations_never
        PASS [   0.192s] ty_python_semantic::mdtest mdtest__comparison_identity
        FAIL [   0.496s] ty_python_semantic::mdtest mdtest__annotations_deferred

--- STDOUT:              ty_python_semantic::mdtest mdtest__annotations_deferred ---

running 1 test

deferred.md - Deferred annotations - Non-deferred self-reference annotations in a class definition

  crates/ty_python_semantic/resources/mdtest/annotations/deferred.md:115 unexpected error: 19 [not-iterable] "Object of type `range` is not iterable"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='deferred.md - Deferred annotations - Non-deferred self-reference annotations in a class definition'
MDTEST_TEST_FILTER='deferred.md - Deferred annotations - Non-deferred self-reference annotations in a class definition' cargo test -p ty_python_semantic --test mdtest -- mdtest__annotations_deferred

--------------------------------------------------

test mdtest__annotations_deferred ... FAILED

failures:

failures:
    mdtest__annotations_deferred

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.49s


--- STDERR:              ty_python_semantic::mdtest mdtest__annotations_deferred ---

thread 'mdtest__annotations_deferred' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        FAIL [   0.262s] ty_python_semantic::mdtest mdtest__call_union

--- STDOUT:              ty_python_semantic::mdtest mdtest__call_union ---

running 1 test

union.md - Unions in calls - Any non-callable variant

  crates/ty_python_semantic/resources/mdtest/call/union.md:105 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/call/union.md:105 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/call/union.md:105 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/call/union.md:105 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/call/union.md:105    0: to_overloaded_(Id(4004)) -> (R31, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/call/union.md:105              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/call/union.md:105    1: infer_definition_types(Id(2da5)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/union.md:105              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/call/union.md:105    2: symbol_by_id(Id(1c10)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/union.md:105              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/call/union.md:105    3: infer_scope_types(Id(801)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/union.md:105              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/call/union.md:105    4: check_types(Id(0)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/union.md:105              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/call/union.md:105

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='union.md - Unions in calls - Any non-callable variant'
MDTEST_TEST_FILTER='union.md - Unions in calls - Any non-callable variant' cargo test -p ty_python_semantic --test mdtest -- mdtest__call_union

union.md - Unions in calls - One not-callable, one wrong argument

  crates/ty_python_semantic/resources/mdtest/call/union.md:137 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/call/union.md:137 expected function
  crates/ty_python_semantic/resources/mdtest/call/union.md:137 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/call/union.md:137 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/call/union.md:137    0: to_overloaded_(Id(4000)) -> (R48, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/union.md:137              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/call/union.md:137    1: infer_definition_types(Id(47cc)) -> (R48, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/union.md:137              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/call/union.md:137    2: infer_scope_types(Id(800)) -> (R49, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/union.md:137              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/call/union.md:137    3: check_types(Id(0)) -> (R48, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/union.md:137              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/call/union.md:137

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='union.md - Unions in calls - One not-callable, one wrong argument'
MDTEST_TEST_FILTER='union.md - Unions in calls - One not-callable, one wrong argument' cargo test -p ty_python_semantic --test mdtest -- mdtest__call_union

union.md - Unions in calls - Unions with literals and negations

  crates/ty_python_semantic/resources/mdtest/call/union.md:171 unexpected error: [static-assert-error] "Static assertion error: argument evaluates to `False`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:172 unexpected error: [static-assert-error] "Static assertion error: argument evaluates to `False`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:173 unexpected error: [static-assert-error] "Static assertion error: argument evaluates to `False`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:174 unexpected error: [static-assert-error] "Static assertion error: argument evaluates to `False`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:176 unexpected error: [static-assert-error] "Static assertion error: argument evaluates to `False`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:176 unexpected error: [invalid-syntax-in-forward-annotation] "Syntax error in forward annotation: Expected an expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:177 unexpected error: [static-assert-error] "Static assertion error: argument evaluates to `False`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:177 unexpected error: [invalid-syntax-in-forward-annotation] "Syntax error in forward annotation: Expected an expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:177 unexpected error: [invalid-syntax-in-forward-annotation] "Syntax error in forward annotation: Expected an expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:178 unexpected error: [static-assert-error] "Static assertion error: argument evaluates to `False`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:178 unexpected error: [invalid-syntax-in-forward-annotation] "Syntax error in forward annotation: Expected an expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:179 unexpected error: [static-assert-error] "Static assertion error: argument evaluates to `False`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:179 unexpected error: [invalid-syntax-in-forward-annotation] "Syntax error in forward annotation: Expected an expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:179 unexpected error: [invalid-syntax-in-forward-annotation] "Syntax error in forward annotation: Expected an expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:183 unexpected error: [invalid-syntax-in-forward-annotation] "Syntax error in forward annotation: Expected an expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:184 unexpected error: [invalid-syntax-in-forward-annotation] "Syntax error in forward annotation: Expected an expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:184 unexpected error: [invalid-syntax-in-forward-annotation] "Syntax error in forward annotation: Expected an expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:185 unexpected error: [invalid-syntax-in-forward-annotation] "Syntax error in forward annotation: Expected an expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:185 unexpected error: [invalid-syntax-in-forward-annotation] "Syntax error in forward annotation: Expected an expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:186 unexpected error: [unresolved-reference] "Name `a` used when not defined"
  crates/ty_python_semantic/resources/mdtest/call/union.md:186 unexpected error: [unresolved-reference] "Name `a` used when not defined"
  crates/ty_python_semantic/resources/mdtest/call/union.md:187 unexpected error: [invalid-type-form] "Bytes literals are not allowed in this context in a type expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:187 unexpected error: [invalid-type-form] "Bytes literals are not allowed in this context in a type expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:188 unexpected error: [invalid-type-form] "Bytes literals are not allowed in this context in a type expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:188 unexpected error: [invalid-type-form] "Bytes literals are not allowed in this context in a type expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:189 unexpected error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:189 unexpected error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:190 unexpected error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:190 unexpected error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
  crates/ty_python_semantic/resources/mdtest/call/union.md:192 unmatched assertion: revealed: Literal[""] | ~AlwaysFalsy
  crates/ty_python_semantic/resources/mdtest/call/union.md:192 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:193 unmatched assertion: revealed: object
  crates/ty_python_semantic/resources/mdtest/call/union.md:193 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:194 unmatched assertion: revealed: object
  crates/ty_python_semantic/resources/mdtest/call/union.md:194 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:195 unmatched assertion: revealed: object
  crates/ty_python_semantic/resources/mdtest/call/union.md:195 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:196 unmatched assertion: revealed: object
  crates/ty_python_semantic/resources/mdtest/call/union.md:196 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:197 unmatched assertion: revealed: object
  crates/ty_python_semantic/resources/mdtest/call/union.md:197 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:198 unmatched assertion: revealed: object
  crates/ty_python_semantic/resources/mdtest/call/union.md:198 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:199 unmatched assertion: revealed: object
  crates/ty_python_semantic/resources/mdtest/call/union.md:199 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:200 unmatched assertion: revealed: object
  crates/ty_python_semantic/resources/mdtest/call/union.md:200 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='union.md - Unions in calls - Unions with literals and negations'
MDTEST_TEST_FILTER='union.md - Unions in calls - Unions with literals and negations' cargo test -p ty_python_semantic --test mdtest -- mdtest__call_union

union.md - Unions in calls - Size limit on unions of literals

  crates/ty_python_semantic/resources/mdtest/call/union.md:233 unmatched assertion: revealed: int
  crates/ty_python_semantic/resources/mdtest/call/union.md:233 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:240 unmatched assertion: revealed: int
  crates/ty_python_semantic/resources/mdtest/call/union.md:240 unexpected error: 17 [revealed-type] "Revealed type: `bool | @Todo(unknown type subscript)`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='union.md - Unions in calls - Size limit on unions of literals'
MDTEST_TEST_FILTER='union.md - Unions in calls - Size limit on unions of literals' cargo test -p ty_python_semantic --test mdtest -- mdtest__call_union

union.md - Unions in calls - Simplifying gradually-equivalent types

  crates/ty_python_semantic/resources/mdtest/call/union.md:251 unexpected error: [call-non-callable] "Method `__getitem__` of type `Overload[(index: int) -> _T, (index: slice) -> @Todo(specialized non-generic class)]` is not callable on object of type `MutableSequence`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:251 unexpected error: [call-non-callable] "Method `__getitem__` of type `Overload[(index: int) -> _T, (index: slice) -> @Todo(specialized non-generic class)]` is not callable on object of type `MutableSequence`"
  crates/ty_python_semantic/resources/mdtest/call/union.md:252 unmatched assertion: revealed: Any & ~int
  crates/ty_python_semantic/resources/mdtest/call/union.md:252 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='union.md - Unions in calls - Simplifying gradually-equivalent types'
MDTEST_TEST_FILTER='union.md - Unions in calls - Simplifying gradually-equivalent types' cargo test -p ty_python_semantic --test mdtest -- mdtest__call_union

--------------------------------------------------

test mdtest__call_union ... FAILED

failures:

failures:
    mdtest__call_union

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.26s


--- STDERR:              ty_python_semantic::mdtest mdtest__call_union ---

thread 'mdtest__call_union' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        FAIL [   0.296s] ty_python_semantic::mdtest mdtest__call_dunder

--- STDOUT:              ty_python_semantic::mdtest mdtest__call_dunder ---

running 1 test

dunder.md - Dunder calls - Calling a union of dunder methods

  crates/ty_python_semantic/resources/mdtest/call/dunder.md:188 unexpected error: [missing-argument] "No argument provided for required parameter `name` of bound method `__init__`"
  crates/ty_python_semantic/resources/mdtest/call/dunder.md:201 unexpected error: [missing-argument] "No argument provided for required parameter `name` of bound method `__init__`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='dunder.md - Dunder calls - Calling a union of dunder methods'
MDTEST_TEST_FILTER='dunder.md - Dunder calls - Calling a union of dunder methods' cargo test -p ty_python_semantic --test mdtest -- mdtest__call_dunder

--------------------------------------------------

test mdtest__call_dunder ... FAILED

failures:

failures:
    mdtest__call_dunder

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.29s


--- STDERR:              ty_python_semantic::mdtest mdtest__call_dunder ---

thread 'mdtest__call_dunder' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.169s] ty_python_semantic::mdtest mdtest__comparison_strings
        PASS [   0.177s] ty_python_semantic::mdtest mdtest__comparison_non_bool_returns
        PASS [   0.187s] ty_python_semantic::mdtest mdtest__comparison_integers
        PASS [   0.170s] ty_python_semantic::mdtest mdtest__comparison_unsupported
        FAIL [   0.208s] ty_python_semantic::mdtest mdtest__comparison_intersections

--- STDOUT:              ty_python_semantic::mdtest mdtest__comparison_intersections ---

running 1 test

intersections.md - Comparison: Intersections - Diagnostics - Unsupported operators for negative contributions

  crates/ty_python_semantic/resources/mdtest/comparison/intersections.md:146 unmatched assertion: revealed: Container & ~NonContainer
  crates/ty_python_semantic/resources/mdtest/comparison/intersections.md:149 unmatched assertion: revealed: bool

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersections.md - Comparison: Intersections - Diagnostics - Unsupported operators for negative contributions'
MDTEST_TEST_FILTER='intersections.md - Comparison: Intersections - Diagnostics - Unsupported operators for negative contributions' cargo test -p ty_python_semantic --test mdtest -- mdtest__comparison_intersections

--------------------------------------------------

test mdtest__comparison_intersections ... FAILED

failures:

failures:
    mdtest__comparison_intersections

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.20s


--- STDERR:              ty_python_semantic::mdtest mdtest__comparison_intersections ---

thread 'mdtest__comparison_intersections' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.171s] ty_python_semantic::mdtest mdtest__comprehensions_invalid_syntax
        PASS [   0.576s] ty_python_semantic::mdtest mdtest__annotations_callable
        PASS [   0.544s] ty_python_semantic::mdtest mdtest__assignment_annotations
        PASS [   0.206s] ty_python_semantic::mdtest mdtest__comparison_unions
        PASS [   0.221s] ty_python_semantic::mdtest mdtest__cycle
        PASS [   0.109s] ty_python_semantic::mdtest mdtest__diagnostics_unpacking
        PASS [   0.186s] ty_python_semantic::mdtest mdtest__declaration_error
        FAIL [   0.231s] ty_python_semantic::mdtest mdtest__dataclass_transform

--- STDOUT:              ty_python_semantic::mdtest mdtest__dataclass_transform ---

running 1 test

dataclass_transform.md - `typing.dataclass_transform` - Types of decorators - Decorating a metaclass

  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:96 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:96 expected function
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:96 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:96 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:96    0: to_overloaded_(Id(2800)) -> (R18, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:96              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:96    1: infer_scope_types(Id(801)) -> (R18, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:96              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:96    2: check_types(Id(0)) -> (R19, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:96              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:96

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='dataclass_transform.md - `typing.dataclass_transform` - Types of decorators - Decorating a metaclass'
MDTEST_TEST_FILTER='dataclass_transform.md - `typing.dataclass_transform` - Types of decorators - Decorating a metaclass' cargo test -p ty_python_semantic --test mdtest -- mdtest__dataclass_transform

dataclass_transform.md - `typing.dataclass_transform` - Types of decorators - Decorating a base class

  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116 expected function
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116    0: to_overloaded_(Id(2800)) -> (R24, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116    1: infer_definition_types(Id(43ff)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116    2: symbol_by_id(Id(2017)) -> (R25, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116    3: class_member_with_policy_(Id(6000)) -> (R25, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116    4: try_call_dunder_get_(Id(6801)) -> (R25, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116    5: member_lookup_with_policy_(Id(1804)) -> (R25, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116    6: infer_scope_types(Id(800)) -> (R25, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116    7: check_types(Id(0)) -> (R24, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:116

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='dataclass_transform.md - `typing.dataclass_transform` - Types of decorators - Decorating a base class'
MDTEST_TEST_FILTER='dataclass_transform.md - `typing.dataclass_transform` - Types of decorators - Decorating a base class' cargo test -p ty_python_semantic --test mdtest -- mdtest__dataclass_transform

dataclass_transform.md - `typing.dataclass_transform` - Arguments to `dataclass_transform` - `order_default`

  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:144 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:144 expected class
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:144 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:144 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:144    0: pep695_generic_context_(Id(2c01)) -> (R30, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:144              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:144    1: infer_definition_types(Id(c02)) -> (R31, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:144              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:144    2: infer_scope_types(Id(3fa2)) -> (R31, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:144              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:144    3: check_types(Id(0)) -> (R31, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:144              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:144

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='dataclass_transform.md - `typing.dataclass_transform` - Arguments to `dataclass_transform` - `order_default`'
MDTEST_TEST_FILTER='dataclass_transform.md - `typing.dataclass_transform` - Arguments to `dataclass_transform` - `order_default`' cargo test -p ty_python_semantic --test mdtest -- mdtest__dataclass_transform

dataclass_transform.md - `typing.dataclass_transform` - Overloaded dataclass-like decorators - Applying `dataclass_transform` to the implementation

  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212    0: pep695_generic_context_(Id(2c0a)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212    1: try_mro_(Id(5c0d)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212    2: infer_expression_types(Id(1c0e)) -> (R37, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212    3: infer_expression_type(Id(1c0e)) -> (R1, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212              at crates/ty_python_semantic/src/types/infer.rs:277
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212    4: symbol_by_id(Id(201a)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212    5: member_lookup_with_policy_(Id(1805)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212    6: infer_definition_types(Id(57cf)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212    7: infer_scope_types(Id(800)) -> (R36, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212    8: check_types(Id(0)) -> (R36, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:212

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='dataclass_transform.md - `typing.dataclass_transform` - Overloaded dataclass-like decorators - Applying `dataclass_transform` to the implementation'
MDTEST_TEST_FILTER='dataclass_transform.md - `typing.dataclass_transform` - Overloaded dataclass-like decorators - Applying `dataclass_transform` to the implementation' cargo test -p ty_python_semantic --test mdtest -- mdtest__dataclass_transform

dataclass_transform.md - `typing.dataclass_transform` - Overloaded dataclass-like decorators - Applying `dataclass_transform` to an overload

  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253    0: pep695_generic_context_(Id(2c0a)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253    1: try_mro_(Id(5c0e)) -> (R43, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253    2: infer_expression_types(Id(1c0e)) -> (R43, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253    3: infer_expression_type(Id(1c0e)) -> (R1, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253              at crates/ty_python_semantic/src/types/infer.rs:277
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253    4: symbol_by_id(Id(201a)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253    5: member_lookup_with_policy_(Id(1807)) -> (R43, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253    6: infer_definition_types(Id(57cf)) -> (R43, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253    7: infer_scope_types(Id(800)) -> (R42, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253    8: check_types(Id(0)) -> (R42, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/dataclass_transform.md:253

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='dataclass_transform.md - `typing.dataclass_transform` - Overloaded dataclass-like decorators - Applying `dataclass_transform` to an overload'
MDTEST_TEST_FILTER='dataclass_transform.md - `typing.dataclass_transform` - Overloaded dataclass-like decorators - Applying `dataclass_transform` to an overload' cargo test -p ty_python_semantic --test mdtest -- mdtest__dataclass_transform

--------------------------------------------------

test mdtest__dataclass_transform ... FAILED

failures:

failures:
    mdtest__dataclass_transform

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.22s


--- STDERR:              ty_python_semantic::mdtest mdtest__dataclass_transform ---

thread 'mdtest__dataclass_transform' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.130s] ty_python_semantic::mdtest mdtest__diagnostics_unresolved_import
        PASS [   0.020s] ty_python_semantic::mdtest mdtest__doc_README
        PASS [   0.208s] ty_python_semantic::mdtest mdtest__diagnostics_no_matching_overload
        PASS [   0.181s] ty_python_semantic::mdtest mdtest__diagnostics_shadowing
        FAIL [   0.385s] ty_python_semantic::mdtest mdtest__comparison_tuples

--- STDOUT:              ty_python_semantic::mdtest mdtest__comparison_tuples ---

running 1 test

tuples.md - Comparison: Tuples - Heterogeneous - Value Comparisons - Non Boolean Rich Comparisons

  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:178 unexpected error: 13 [revealed-type] "Revealed type: `bool`"
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:179 unexpected error: 13 [revealed-type] "Revealed type: `bool`"
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:180 unexpected error: 13 [revealed-type] "Revealed type: `LtReturnType | Literal[False]`"
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:181 unexpected error: 13 [revealed-type] "Revealed type: `LeReturnType | Literal[True]`"
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:182 unexpected error: 13 [revealed-type] "Revealed type: `GtReturnType | Literal[False]`"
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:183 unexpected error: 13 [revealed-type] "Revealed type: `GeReturnType | Literal[True]`"
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:189 unexpected error: 13 [revealed-type] "Revealed type: `Literal[False]`"
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:190 unexpected error: 13 [revealed-type] "Revealed type: `Literal[True]`"
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:191 unexpected error: 13 [revealed-type] "Revealed type: `Literal[True]`"
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:192 unexpected error: 13 [revealed-type] "Revealed type: `Literal[True]`"
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:193 unexpected error: 13 [revealed-type] "Revealed type: `Literal[False]`"
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:194 unexpected error: 13 [revealed-type] "Revealed type: `Literal[False]`"
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:202 unexpected error: 13 [revealed-type] "Revealed type: `LtReturnType | LtReturnTypeOnB | Literal[False]`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='tuples.md - Comparison: Tuples - Heterogeneous - Value Comparisons - Non Boolean Rich Comparisons'
MDTEST_TEST_FILTER='tuples.md - Comparison: Tuples - Heterogeneous - Value Comparisons - Non Boolean Rich Comparisons' cargo test -p ty_python_semantic --test mdtest -- mdtest__comparison_tuples

tuples.md - Comparison: Tuples - Heterogeneous - Value Comparisons - Special Handling of Eq and NotEq in Lexicographic Comparisons

  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218    0: explicit_bases_(Id(2803)) -> (R25, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218    1: member_lookup_with_policy_(Id(4417)) -> (R31, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218    2: infer_deferred_types(Id(2ead)) -> (R31, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218    3: explicit_bases_(Id(280f)) -> (R31, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218    4: infer_definition_types(Id(4bcc)) -> (R31, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218    5: infer_scope_types(Id(800)) -> (R30, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218    6: check_types(Id(0)) -> (R30, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:218

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='tuples.md - Comparison: Tuples - Heterogeneous - Value Comparisons - Special Handling of Eq and NotEq in Lexicographic Comparisons'
MDTEST_TEST_FILTER='tuples.md - Comparison: Tuples - Heterogeneous - Value Comparisons - Special Handling of Eq and NotEq in Lexicographic Comparisons' cargo test -p ty_python_semantic --test mdtest -- mdtest__comparison_tuples

tuples.md - Comparison: Tuples - Heterogeneous - Value Comparisons - Error Propagation

  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258    0: explicit_bases_(Id(2803)) -> (R25, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258    1: infer_expression_types(Id(cfd)) -> (R25, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258    2: member_lookup_with_policy_(Id(4417)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258    3: infer_deferred_types(Id(2ead)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258    4: explicit_bases_(Id(280f)) -> (R31, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258    5: infer_definition_types(Id(4bcc)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258    6: infer_scope_types(Id(800)) -> (R36, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258    7: check_types(Id(0)) -> (R36, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:258

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='tuples.md - Comparison: Tuples - Heterogeneous - Value Comparisons - Error Propagation'
MDTEST_TEST_FILTER='tuples.md - Comparison: Tuples - Heterogeneous - Value Comparisons - Error Propagation' cargo test -p ty_python_semantic --test mdtest -- mdtest__comparison_tuples

tuples.md - Comparison: Tuples - Heterogeneous - Membership Test Comparisons

  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:295 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:295 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:295 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:295 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:295    0: explicit_bases_(Id(2803)) -> (R25, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:295              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:295    1: infer_expression_types(Id(ce7)) -> (R43, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:295              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:295    2: infer_scope_types(Id(3b9f)) -> (R43, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:295              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:295    3: check_types(Id(0)) -> (R43, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:295              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:295

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='tuples.md - Comparison: Tuples - Heterogeneous - Membership Test Comparisons'
MDTEST_TEST_FILTER='tuples.md - Comparison: Tuples - Heterogeneous - Membership Test Comparisons' cargo test -p ty_python_semantic --test mdtest -- mdtest__comparison_tuples

tuples.md - Comparison: Tuples - Heterogeneous - Identity Comparisons

  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:316 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:316 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:316 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:316 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:316    0: explicit_bases_(Id(2803)) -> (R25, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:316              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:316    1: infer_expression_types(Id(ce7)) -> (R49, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:316              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:316    2: infer_scope_types(Id(800)) -> (R49, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:316              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:316    3: check_types(Id(0)) -> (R48, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:316              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/comparison/tuples.md:316

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='tuples.md - Comparison: Tuples - Heterogeneous - Identity Comparisons'
MDTEST_TEST_FILTER='tuples.md - Comparison: Tuples - Heterogeneous - Identity Comparisons' cargo test -p ty_python_semantic --test mdtest -- mdtest__comparison_tuples
test mdtest__comparison_tuples ... FAILED

failures:

failures:
    mdtest__comparison_tuples

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.38s


--- STDERR:              ty_python_semantic::mdtest mdtest__comparison_tuples ---

thread 'mdtest__comparison_tuples' panicked at crates/ty_test/src/lib.rs:379:9:
Test `tuples.md - Comparison: Tuples - Chained comparisons with elements that incorrectly implement `__bool__`` requested snapshotting diagnostics but it didn't produce any.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        FAIL [   0.567s] ty_python_semantic::mdtest mdtest__call_subclass_of

--- STDOUT:              ty_python_semantic::mdtest mdtest__call_subclass_of ---

running 1 test

subclass_of.md - Call `type[...]` - Unions of classes

  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47 panicked at /home/ibraheem/dev/astral/salsa/src/function/fetch.rs:129:25
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47 dependency graph cycle when querying try_call_dunder_get_(Id(6804)), set cycle_fn/cycle_initial to fixpoint iterate.
Query stack:
[
    check_types(Id(0)),
    infer_scope_types(Id(801)),
    symbol_by_id(Id(200e)),
    infer_definition_types(Id(2da2)),
    infer_expression_types(Id(1c5c)),
    infer_definition_types(Id(2da1)),
    member_lookup_with_policy_(Id(4005)),
    symbol_by_id(Id(2004)),
    infer_expression_type(Id(1c26)),
    infer_expression_types(Id(1c26)),
    explicit_bases_(Id(1007)),
    infer_deferred_types(Id(2963)),
    infer_expression_types(Id(1cb1)),
    try_call_dunder_get_(Id(6804)),
    symbol_by_id(Id(200b)),
    infer_expression_type(Id(1cf7)),
    infer_expression_types(Id(1cf7)),
    signature_(Id(3403)),
    infer_deferred_types(Id(2940)),
    symbol_by_id(Id(201a)),
    infer_definition_types(Id(f2c)),
    infer_expression_types(Id(1c2b)),
    signature_(Id(3402)),
    infer_deferred_types(Id(292d)),
    symbol_by_id(Id(2021)),
    infer_definition_types(Id(efe)),
    member_lookup_with_policy_(Id(4028)),
    symbol_by_id(Id(2022)),
    dunder_all_names(Id(48)),
    infer_expression_types(Id(1d42)),
    signature_(Id(3404)),
    infer_deferred_types(Id(2943)),
    resolve_module_query(Id(180a)),
]
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47    0: infer_expression_types(Id(1c2b)) -> (R19, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              cycle heads: explicit_bases_(Id(1007)) -> 0, infer_definition_types(Id(2da2)) -> 0, symbol_by_id(Id(200b)) -> 1, explicit_bases_(Id(1010)) -> 0, infer_expression_type(Id(1c26)) -> 0
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47    1: infer_definition_types(Id(f2c)) -> (R19, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47    2: symbol_by_id(Id(201a)) -> (R19, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47    3: infer_deferred_types(Id(2940)) -> (R19, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47    4: signature_(Id(3403)) -> (R19, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47    5: infer_expression_types(Id(1cf7)) -> (R19, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              cycle heads: explicit_bases_(Id(1007)) -> 0, symbol_by_id(Id(200b)) -> 1, infer_definition_types(Id(2da2)) -> 0, explicit_bases_(Id(1010)) -> 0, infer_expression_type(Id(1c26)) -> 0
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47    6: infer_expression_type(Id(1cf7)) -> (R19, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types/infer.rs:277
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47    7: symbol_by_id(Id(200b)) -> (R19, Durability::LOW, iteration = 1)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47    8: try_call_dunder_get_(Id(6804)) -> (R19, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47    9: infer_expression_types(Id(1cb1)) -> (R19, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              cycle heads: explicit_bases_(Id(1007)) -> 0, infer_definition_types(Id(2da2)) -> 0, infer_expression_type(Id(1c26)) -> 0, explicit_bases_(Id(1010)) -> 0
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47   10: infer_deferred_types(Id(2963)) -> (R7, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47   11: explicit_bases_(Id(1007)) -> (R19, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47   12: infer_expression_types(Id(1c26)) -> (R19, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47   13: infer_expression_type(Id(1c26)) -> (R1, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types/infer.rs:277
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47   14: symbol_by_id(Id(2004)) -> (R19, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47   15: member_lookup_with_policy_(Id(4005)) -> (R19, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47   16: infer_definition_types(Id(2da1)) -> (R19, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47   17: infer_expression_types(Id(1c5c)) -> (R1, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47   18: infer_definition_types(Id(2da2)) -> (R1, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47   19: symbol_by_id(Id(200e)) -> (R19, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47   20: infer_scope_types(Id(801)) -> (R19, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47   21: check_types(Id(0)) -> (R19, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/call/subclass_of.md:47

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='subclass_of.md - Call `type[...]` - Unions of classes'
MDTEST_TEST_FILTER='subclass_of.md - Call `type[...]` - Unions of classes' cargo test -p ty_python_semantic --test mdtest -- mdtest__call_subclass_of

--------------------------------------------------

test mdtest__call_subclass_of ... FAILED

failures:

failures:
    mdtest__call_subclass_of

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.56s


--- STDERR:              ty_python_semantic::mdtest mdtest__call_subclass_of ---

thread 'mdtest__call_subclass_of' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        FAIL [   0.283s] ty_python_semantic::mdtest mdtest__diagnostics_invalid_argument_type

--- STDOUT:              ty_python_semantic::mdtest mdtest__diagnostics_invalid_argument_type ---

running 1 test
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ Snapshot Summary ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Snapshot file: crates/ty_python_semantic/resources/mdtest/snapshots/invalid_argument_type.md_-_Invalid_argument_type_diagnostics_-_Many_parameters_across_multiple_lines.snap
Snapshot: invalid_argument_type.md_-_Invalid_argument_type_diagnostics_-_Many_parameters_across_multiple_lines
Source: crates/ty_test/src/lib.rs:394
────────────────────────────────────────────────────────────────────────────────
Expression: snapshot
────────────────────────────────────────────────────────────────────────────────
-old snapshot
+new results
────────────┬───────────────────────────────────────────────────────────────────
   19    19 │
   20    20 │ # Diagnostics
   21    21 │
   22    22 │ ```
         23 │+error: lint:unsupported-operator: Operator `*` is unsupported between objects of type `int` and `int`
         24 │+ --> src/mdtest_snippet.py:6:12
         25 │+  |
         26 │+4 |     z: int,
         27 │+5 | ) -> int:
         28 │+6 |     return x * y * z
         29 │+  |            ^^^^^
         30 │+7 |
         31 │+8 | foo(1, "hello", 3)  # error: [invalid-argument-type]
         32 │+  |
         33 │+info: `lint:unsupported-operator` is enabled by default
         34 │+
         35 │+```
         36 │+
         37 │+```
   23    38 │ error: lint:invalid-argument-type: Argument to this function is incorrect
   24    39 │  --> src/mdtest_snippet.py:8:8
   25    40 │   |
   26    41 │ 6 |     return x * y * z
────────────┴───────────────────────────────────────────────────────────────────
To update snapshots run `cargo insta review`
Stopped on the first failure. Run `cargo insta test` to run all snapshots.
test mdtest__diagnostics_invalid_argument_type ... FAILED

failures:

failures:
    mdtest__diagnostics_invalid_argument_type

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.28s


--- STDERR:              ty_python_semantic::mdtest mdtest__diagnostics_invalid_argument_type ---
stored new snapshot /home/ibraheem/dev/astral/ruff/crates/ty_python_semantic/resources/mdtest/snapshots/invalid_argument_type.md_-_Invalid_argument_type_diagnostics_-_Many_parameters_across_multiple_lines.snap.new

thread 'mdtest__diagnostics_invalid_argument_type' panicked at /home/ibraheem/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/insta-1.42.2/src/runtime.rs:679:13:
snapshot assertion for 'invalid_argument_type.md_-_Invalid_argument_type_diagnostics_-_Many_parameters_across_multiple_lines' failed in line 394
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        FAIL [   0.294s] ty_python_semantic::mdtest mdtest__diagnostics_attribute_assignment

--- STDOUT:              ty_python_semantic::mdtest mdtest__diagnostics_attribute_assignment ---

running 1 test
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ Snapshot Summary ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Snapshot file: crates/ty_python_semantic/resources/mdtest/snapshots/attribute_assignment.md_-_Attribute_assignment_-_Possibly-unbound_attributes.snap
Snapshot: attribute_assignment.md_-_Attribute_assignment_-_Possibly-unbound_attributes
Source: crates/ty_test/src/lib.rs:394
────────────────────────────────────────────────────────────────────────────────
Expression: snapshot
────────────────────────────────────────────────────────────────────────────────
-old snapshot
+new results
────────────┬───────────────────────────────────────────────────────────────────
   35    35 │
   36    36 │ ```
   37    37 │
   38    38 │ ```
         39 │+error: lint:invalid-argument-type: Argument to this function is incorrect
         40 │+ --> src/mdtest_snippet.py:8:16
         41 │+  |
         42 │+6 |     C.attr = 1  # error: [possibly-unbound-attribute]
         43 │+7 |
         44 │+8 |     instance = C()
         45 │+  |                ^^^ Expected `bool`, found `C`
         46 │+9 |     instance.attr = 1  # error: [possibly-unbound-attribute]
         47 │+  |
         48 │+info: Function defined here
         49 │+ --> src/mdtest_snippet.py:1:5
         50 │+  |
         51 │+1 | def _(flag: bool) -> None:
         52 │+  |     ^ ---------- Parameter declared here
         53 │+2 |     class C:
         54 │+3 |         if flag:
         55 │+  |
         56 │+info: `lint:invalid-argument-type` is enabled by default
         57 │+
         58 │+```
         59 │+
         60 │+```
   39    61 │ warning: lint:possibly-unbound-attribute: Attribute `attr` on type `C` is possibly unbound
   40    62 │  --> src/mdtest_snippet.py:9:5
   41    63 │   |
   42    64 │ 8 |     instance = C()
────────────┴───────────────────────────────────────────────────────────────────
To update snapshots run `cargo insta review`
Stopped on the first failure. Run `cargo insta test` to run all snapshots.
test mdtest__diagnostics_attribute_assignment ... FAILED

failures:

failures:
    mdtest__diagnostics_attribute_assignment

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.29s


--- STDERR:              ty_python_semantic::mdtest mdtest__diagnostics_attribute_assignment ---
stored new snapshot /home/ibraheem/dev/astral/ruff/crates/ty_python_semantic/resources/mdtest/snapshots/attribute_assignment.md_-_Attribute_assignment_-_Possibly-unbound_attributes.snap.new

thread 'mdtest__diagnostics_attribute_assignment' panicked at /home/ibraheem/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/insta-1.42.2/src/runtime.rs:679:13:
snapshot assertion for 'attribute_assignment.md_-_Attribute_assignment_-_Possibly-unbound_attributes' failed in line 394
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        FAIL [   0.650s] ty_python_semantic::mdtest mdtest__call_function

--- STDOUT:              ty_python_semantic::mdtest mdtest__call_function ---

running 1 test

function.md - Call expression - Special functions - `reveal_type`

  crates/ty_python_semantic/resources/mdtest/call/function.md:282 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/call/function.md:282 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/call/function.md:282 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/call/function.md:282 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/call/function.md:282    0: to_overloaded_(Id(3401)) -> (R153, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/call/function.md:282              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/call/function.md:282    1: infer_scope_types(Id(800)) -> (R159, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/function.md:282              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/call/function.md:282    2: check_types(Id(0)) -> (R158, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/function.md:282              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/call/function.md:282

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='function.md - Call expression - Special functions - `reveal_type`'
MDTEST_TEST_FILTER='function.md - Call expression - Special functions - `reveal_type`' cargo test -p ty_python_semantic --test mdtest -- mdtest__call_function

function.md - Call expression - Special functions - `len`

  crates/ty_python_semantic/resources/mdtest/call/function.md:306 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/call/function.md:306 expected function
  crates/ty_python_semantic/resources/mdtest/call/function.md:306 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/call/function.md:306 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/call/function.md:306    0: to_overloaded_(Id(3402)) -> (R165, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/call/function.md:306              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/call/function.md:306    1: infer_definition_types(Id(3b07)) -> (R105, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/call/function.md:306              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/call/function.md:306    2: symbol_by_id(Id(1c10)) -> (R171, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/call/function.md:306              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/call/function.md:306    3: member_lookup_with_policy_(Id(4008)) -> (R171, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/call/function.md:306              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/call/function.md:306    4: infer_definition_types(Id(786f)) -> (R171, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/call/function.md:306              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/call/function.md:306    5: symbol_by_id(Id(1c0d)) -> (R171, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/call/function.md:306              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/call/function.md:306    6: symbol_by_id(Id(1c0c)) -> (R171, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/function.md:306              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/call/function.md:306    7: member_lookup_with_policy_(Id(4007)) -> (R171, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/call/function.md:306              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/call/function.md:306    8: infer_definition_types(Id(f01)) -> (R171, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/call/function.md:306              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/call/function.md:306    9: infer_deferred_types(Id(2c03)) -> (R21, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/function.md:306              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/call/function.md:306   10: signature_(Id(3400)) -> (R171, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/function.md:306              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/call/function.md:306   11: infer_scope_types(Id(800)) -> (R171, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/function.md:306              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/call/function.md:306   12: check_types(Id(0)) -> (R170, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/function.md:306              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/call/function.md:306

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='function.md - Call expression - Special functions - `len`'
MDTEST_TEST_FILTER='function.md - Call expression - Special functions - `len`' cargo test -p ty_python_semantic --test mdtest -- mdtest__call_function

function.md - Call expression - Special functions - Type API predicates

  crates/ty_python_semantic/resources/mdtest/call/function.md:316 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/call/function.md:316 expected function
  crates/ty_python_semantic/resources/mdtest/call/function.md:316 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/call/function.md:316 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/call/function.md:316    0: to_overloaded_(Id(3401)) -> (R165, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/call/function.md:316              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/call/function.md:316    1: symbol_by_id(Id(1c14)) -> (R177, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/call/function.md:316              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/call/function.md:316    2: infer_scope_types(Id(800)) -> (R177, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/function.md:316              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/call/function.md:316    3: check_types(Id(0)) -> (R176, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/call/function.md:316              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/call/function.md:316

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='function.md - Call expression - Special functions - Type API predicates'
MDTEST_TEST_FILTER='function.md - Call expression - Special functions - Type API predicates' cargo test -p ty_python_semantic --test mdtest -- mdtest__call_function

--------------------------------------------------

test mdtest__call_function ... FAILED

failures:

failures:
    mdtest__call_function

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.65s


--- STDERR:              ty_python_semantic::mdtest mdtest__call_function ---

thread 'mdtest__call_function' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.184s] ty_python_semantic::mdtest mdtest__directives_cast
        FAIL [   0.855s] ty_python_semantic::mdtest mdtest__attributes

--- STDOUT:              ty_python_semantic::mdtest mdtest__attributes ---

running 1 test

attributes.md - Attributes - Class and instance variables - Pure instance variables - Variable declared in class body and not bound anywhere

  crates/ty_python_semantic/resources/mdtest/attributes.md:113 unexpected error: [missing-argument] "No argument provided for required parameter `self` of function `__init__`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Variable declared in class body and not bound anywhere'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Variable declared in class body and not bound anywhere' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Mixed declarations/bindings in class body and `__init__`

  crates/ty_python_semantic/resources/mdtest/attributes.md:149 unexpected error: [missing-argument] "No argument provided for required parameter `flag` of function `__init__`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Mixed declarations/bindings in class body and `__init__`'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Mixed declarations/bindings in class body and `__init__`' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Variable defined in non-`__init__` method

  crates/ty_python_semantic/resources/mdtest/attributes.md:181 unexpected error: [missing-argument] "No argument provided for required parameter `param` of function `__init__`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Variable defined in non-`__init__` method'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Variable defined in non-`__init__` method' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Variable defined in multiple methods

  crates/ty_python_semantic/resources/mdtest/attributes.md:230 unexpected error: [missing-argument] "No argument provided for required parameter `self` of function `__init__`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Variable defined in multiple methods'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Variable defined in multiple methods' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in multi-target assignments

  crates/ty_python_semantic/resources/mdtest/attributes.md:244 unexpected error: [missing-argument] "No argument provided for required parameter `self` of function `__init__`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in multi-target assignments'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in multi-target assignments' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Augmented assignments

  crates/ty_python_semantic/resources/mdtest/attributes.md:259 unexpected error: [missing-argument] "No argument provided for required parameter `self` of function `__init__`"
  crates/ty_python_semantic/resources/mdtest/attributes.md:264 unexpected error: 13 [missing-argument] "No argument provided for required parameter `self` of function `__init__`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Augmented assignments'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Augmented assignments' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in tuple unpackings

  crates/ty_python_semantic/resources/mdtest/attributes.md:281 unexpected error: [missing-argument] "No argument provided for required parameter `self` of function `__init__`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in tuple unpackings'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in tuple unpackings' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Starred assignments

  crates/ty_python_semantic/resources/mdtest/attributes.md:303 unexpected error: [missing-argument] "No argument provided for required parameter `self` of function `__init__`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Starred assignments'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Starred assignments' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in for-loop (unpacking)

  crates/ty_python_semantic/resources/mdtest/attributes.md:311 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:311 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:311 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:311 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:311    0: to_overloaded_(Id(4401)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:311              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:311    1: infer_definition_types(Id(2c1a)) -> (R61, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:311              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:311    2: symbol_by_id(Id(1c31)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:311              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:311    3: infer_scope_types(Id(33a7)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:311              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:311    4: check_types(Id(0)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:311              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:311

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in for-loop (unpacking)'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in for-loop (unpacking)' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in `with` statements

  crates/ty_python_semantic/resources/mdtest/attributes.md:351 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:351 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:351 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:351 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:351    0: pep695_generic_context_(Id(140d)) -> (R61, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:351              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:351    1: infer_definition_types(Id(c06)) -> (R66, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:351              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:351    2: symbol_by_id(Id(1c09)) -> (R67, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:351              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:351    3: class_member_with_policy_(Id(3810)) -> (R67, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:351              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:351    4: member_lookup_with_policy_(Id(3407)) -> (R67, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:351              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:351    5: infer_expression_types(Id(1001)) -> (R67, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:351              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:351    6: infer_definition_types(Id(5fe0)) -> (R66, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:351              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:351    7: infer_scope_types(Id(800)) -> (R67, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:351              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:351    8: check_types(Id(0)) -> (R66, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:351              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:351

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in `with` statements'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in `with` statements' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in `with` statements, but with unpacking

  crates/ty_python_semantic/resources/mdtest/attributes.md:371 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:371 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:371 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:371 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:371    0: pep695_generic_context_(Id(140d)) -> (R61, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:371              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:371    1: infer_definition_types(Id(c06)) -> (R72, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:371              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:371    2: symbol_by_id(Id(1c0f)) -> (R73, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:371              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:371    3: class_member_with_policy_(Id(3813)) -> (R73, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:371              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:371    4: member_lookup_with_policy_(Id(3408)) -> (R73, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:371              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:371    5: infer_expression_types(Id(1001)) -> (R73, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:371              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:371    6: infer_definition_types(Id(5fe0)) -> (R72, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:371              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:371    7: infer_scope_types(Id(800)) -> (R73, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:371              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:371    8: check_types(Id(0)) -> (R72, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:371              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:371

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in `with` statements, but with unpacking'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in `with` statements, but with unpacking' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in comprehensions

  crates/ty_python_semantic/resources/mdtest/attributes.md:392 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/attributes.md:392 expected class
  crates/ty_python_semantic/resources/mdtest/attributes.md:392 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:392 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:392    0: pep695_generic_context_(Id(140d)) -> (R79, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:392              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:392    1: infer_definition_types(Id(5fe2)) -> (R79, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:392              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:392    2: symbol_by_id(Id(1c13)) -> (R79, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:392              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:392    3: class_member_with_policy_(Id(3814)) -> (R79, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:392              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:392    4: member_lookup_with_policy_(Id(3427)) -> (R79, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:392              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:392    5: infer_expression_types(Id(1001)) -> (R79, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:392              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:392    6: infer_definition_types(Id(5fe4)) -> (R79, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:392              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:392    7: infer_scope_types(Id(800)) -> (R79, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:392              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:392    8: check_types(Id(0)) -> (R78, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:392              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:392

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in comprehensions'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in comprehensions' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Conditionally declared / bound attributes

  crates/ty_python_semantic/resources/mdtest/attributes.md:429 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:429 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:429 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:429 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:429    0: pep695_generic_context_(Id(140d)) -> (R61, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:429              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:429    1: explicit_bases_(Id(1419)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:429              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:429    2: infer_expression_types(Id(1099)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:429              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:429    3: try_call_dunder_get_(Id(6000)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:429              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:429    4: member_lookup_with_policy_(Id(3431)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:429              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:429    5: infer_expression_types(Id(1001)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:429              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:429    6: infer_definition_types(Id(c0c)) -> (R84, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:429              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:429    7: infer_scope_types(Id(800)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:429              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:429    8: check_types(Id(0)) -> (R84, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:429              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:429

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Conditionally declared / bound attributes'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Conditionally declared / bound attributes' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Methods that does not use `self` as a first parameter

  crates/ty_python_semantic/resources/mdtest/attributes.md:457 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:457 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:457 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:457 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:457    0: pep695_generic_context_(Id(140d)) -> (R61, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:457              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:457    1: explicit_bases_(Id(1419)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:457              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:457    2: infer_expression_types(Id(1138)) -> (R91, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:457              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:457    3: infer_scope_types(Id(800)) -> (R91, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:457              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:457    4: check_types(Id(0)) -> (R90, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:457              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:457

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Methods that does not use `self` as a first parameter'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Methods that does not use `self` as a first parameter' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Aliased `self` parameter

  crates/ty_python_semantic/resources/mdtest/attributes.md:469 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:469 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:469 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:469 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:469    0: pep695_generic_context_(Id(140d)) -> (R61, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:469              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:469    1: explicit_bases_(Id(1419)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:469              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:469    2: infer_expression_types(Id(1138)) -> (R97, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:469              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:469    3: infer_scope_types(Id(800)) -> (R96, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:469              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:469    4: check_types(Id(0)) -> (R96, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:469              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:469

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Aliased `self` parameter'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Aliased `self` parameter' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Static methods do not influence implicitly defined attributes

  crates/ty_python_semantic/resources/mdtest/attributes.md:483 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/attributes.md:483 expected class
  crates/ty_python_semantic/resources/mdtest/attributes.md:483 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:483 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:483    0: explicit_bases_(Id(140b)) -> (R103, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:483              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:483    1: explicit_bases_(Id(1419)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:483              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:483    2: infer_expression_types(Id(1138)) -> (R103, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:483              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:483    3: infer_scope_types(Id(800)) -> (R103, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:483              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:483    4: check_types(Id(0)) -> (R102, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:483              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:483

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Static methods do not influence implicitly defined attributes'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Static methods do not influence implicitly defined attributes' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in statically-known-to-be-false branches

  crates/ty_python_semantic/resources/mdtest/attributes.md:550 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:550 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:550 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:550 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:550    0: pep695_generic_context_(Id(140d)) -> (R61, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:550              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:550    1: explicit_bases_(Id(1419)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:550              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:550    2: infer_expression_types(Id(1138)) -> (R109, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:550              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:550    3: infer_scope_types(Id(800)) -> (R108, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:550              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:550    4: check_types(Id(0)) -> (R108, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:550              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:550

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in statically-known-to-be-false branches'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes defined in statically-known-to-be-false branches' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes considered always bound

  crates/ty_python_semantic/resources/mdtest/attributes.md:601 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:601 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:601 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:601 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:601    0: pep695_generic_context_(Id(140d)) -> (R61, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:601              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:601    1: explicit_bases_(Id(1419)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:601              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:601    2: infer_expression_types(Id(1138)) -> (R115, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:601              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:601    3: infer_scope_types(Id(800)) -> (R114, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:601              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:601    4: check_types(Id(0)) -> (R114, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:601              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:601

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes considered always bound'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Attributes considered always bound' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure instance variables - Diagnostics are reported for the right-hand side of attribute assignments

  crates/ty_python_semantic/resources/mdtest/attributes.md:646 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:646 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:646 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:646 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:646    0: pep695_generic_context_(Id(140d)) -> (R61, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:646              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:646    1: explicit_bases_(Id(1419)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:646              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:646    2: infer_expression_types(Id(1099)) -> (R121, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:646              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:646    3: infer_definition_types(Id(5fd6)) -> (R121, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:646              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:646    4: infer_scope_types(Id(801)) -> (R120, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:646              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:646    5: check_types(Id(0)) -> (R121, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:646              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:646

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Diagnostics are reported for the right-hand side of attribute assignments'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure instance variables - Diagnostics are reported for the right-hand side of attribute assignments' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure class variables (`ClassVar`) - Annotated with `ClassVar` type qualifier

  crates/ty_python_semantic/resources/mdtest/attributes.md:663 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:663 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:663 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:663 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:663    0: pep695_generic_context_(Id(140d)) -> (R61, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:663              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:663    1: explicit_bases_(Id(1419)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:663              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:663    2: infer_expression_types(Id(1138)) -> (R127, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:663              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:663    3: infer_scope_types(Id(800)) -> (R127, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:663              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:663    4: check_types(Id(0)) -> (R126, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:663              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:663

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure class variables (`ClassVar`) - Annotated with `ClassVar` type qualifier'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure class variables (`ClassVar`) - Annotated with `ClassVar` type qualifier' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Pure class variables (`ClassVar`) - Variable only mentioned in a class method

  crates/ty_python_semantic/resources/mdtest/attributes.md:706 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:706 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:706 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:706 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:706    0: pep695_generic_context_(Id(140d)) -> (R61, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:706              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:706    1: explicit_bases_(Id(1419)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:706              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:706    2: infer_expression_types(Id(112a)) -> (R121, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:706              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:706    3: symbol_by_id(Id(1c04)) -> (R133, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:706              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:706    4: member_lookup_with_policy_(Id(341f)) -> (R133, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:706              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:706    5: infer_scope_types(Id(800)) -> (R133, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:706              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:706    6: check_types(Id(0)) -> (R132, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:706              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:706

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure class variables (`ClassVar`) - Variable only mentioned in a class method'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Pure class variables (`ClassVar`) - Variable only mentioned in a class method' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Instance variables with class-level default values - Basic

  crates/ty_python_semantic/resources/mdtest/attributes.md:746 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:746 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:746 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:746 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:746    0: pep695_generic_context_(Id(140d)) -> (R61, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:746              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:746    1: explicit_bases_(Id(1419)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:746              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:746    2: infer_expression_types(Id(1138)) -> (R139, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:746              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:746    3: infer_scope_types(Id(800)) -> (R138, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:746              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:746    4: check_types(Id(0)) -> (R138, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:746              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:746

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Instance variables with class-level default values - Basic'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Instance variables with class-level default values - Basic' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Class and instance variables - Inheritance of class/instance attributes - Instance variable defined in a base class

  crates/ty_python_semantic/resources/mdtest/attributes.md:785 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:785 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:785 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:785 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:785    0: pep695_generic_context_(Id(140d)) -> (R61, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:785              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:785    1: explicit_bases_(Id(1419)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:785              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:785    2: infer_expression_types(Id(1138)) -> (R145, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:785              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:785    3: infer_scope_types(Id(800)) -> (R145, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:785              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:785    4: check_types(Id(0)) -> (R144, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:785              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:785

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Inheritance of class/instance attributes - Instance variable defined in a base class'
MDTEST_TEST_FILTER='attributes.md - Attributes - Class and instance variables - Inheritance of class/instance attributes - Instance variable defined in a base class' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Accessing attributes on class objects

  crates/ty_python_semantic/resources/mdtest/attributes.md:833 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:833 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:833 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:833 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:833    0: to_overloaded_(Id(4400)) -> (R150, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:833              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:833    1: infer_definition_types(Id(4367)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:833              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:833    2: member_lookup_with_policy_(Id(3442)) -> (R150, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:833              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:833    3: infer_expression_types(Id(104a)) -> (R150, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:833              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:833    4: infer_deferred_types(Id(27d9)) -> (R151, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:833              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/attributes.md:833    5: explicit_bases_(Id(1419)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:833              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:833    6: infer_expression_types(Id(1138)) -> (R151, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:833              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:833    7: infer_scope_types(Id(800)) -> (R151, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:833              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:833    8: check_types(Id(0)) -> (R150, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:833              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:833

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Accessing attributes on class objects'
MDTEST_TEST_FILTER='attributes.md - Attributes - Accessing attributes on class objects' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Unions of attributes

  crates/ty_python_semantic/resources/mdtest/attributes.md:913 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:913 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:913 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:913 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:913    0: to_overloaded_(Id(4400)) -> (R156, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:913              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:913    1: infer_definition_types(Id(2765)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:913              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:913    2: infer_definition_types(Id(5fe6)) -> (R156, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:913              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:913    3: infer_scope_types(Id(800)) -> (R156, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:913              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:913    4: check_types(Id(0)) -> (R156, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:913              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:913

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Unions of attributes'
MDTEST_TEST_FILTER='attributes.md - Attributes - Unions of attributes' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Unions with possibly unbound paths - Definite boundness within a class

  crates/ty_python_semantic/resources/mdtest/attributes.md:994 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:994 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:994 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:994 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:994    0: to_overloaded_(Id(4400)) -> (R162, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:994              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:994    1: infer_definition_types(Id(2765)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:994              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:994    2: infer_definition_types(Id(5fe6)) -> (R162, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:994              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:994    3: infer_scope_types(Id(800)) -> (R162, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:994              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:994    4: check_types(Id(0)) -> (R162, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:994              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:994

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Unions with possibly unbound paths - Definite boundness within a class'
MDTEST_TEST_FILTER='attributes.md - Attributes - Unions with possibly unbound paths - Definite boundness within a class' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Unions with possibly unbound paths - Possibly-unbound within a class

  crates/ty_python_semantic/resources/mdtest/attributes.md:1024 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:1024 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:1024 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1024 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1024    0: to_overloaded_(Id(4400)) -> (R168, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1024              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:1024    1: infer_definition_types(Id(2765)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1024              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1024    2: infer_definition_types(Id(5fe6)) -> (R168, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1024              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1024    3: infer_scope_types(Id(800)) -> (R168, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1024              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1024    4: check_types(Id(0)) -> (R168, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1024              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1024

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Unions with possibly unbound paths - Possibly-unbound within a class'
MDTEST_TEST_FILTER='attributes.md - Attributes - Unions with possibly unbound paths - Possibly-unbound within a class' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Unions with possibly unbound paths - Possibly-unbound within gradual types

  crates/ty_python_semantic/resources/mdtest/attributes.md:1055 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:1055 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:1055 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1055 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1055    0: to_overloaded_(Id(4400)) -> (R174, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1055              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:1055    1: infer_definition_types(Id(2765)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1055              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1055    2: infer_definition_types(Id(c05)) -> (R175, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1055              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1055    3: infer_scope_types(Id(800)) -> (R175, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1055              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1055    4: check_types(Id(0)) -> (R174, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1055              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1055

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Unions with possibly unbound paths - Possibly-unbound within gradual types'
MDTEST_TEST_FILTER='attributes.md - Attributes - Unions with possibly unbound paths - Possibly-unbound within gradual types' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Unions with possibly unbound paths - Attribute possibly unbound on a subclass but not on a superclass

  crates/ty_python_semantic/resources/mdtest/attributes.md:1075 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:1075 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:1075 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1075 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1075    0: to_overloaded_(Id(4400)) -> (R180, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1075              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:1075    1: infer_definition_types(Id(2765)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1075              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1075    2: infer_definition_types(Id(5fd6)) -> (R181, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1075              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1075    3: infer_scope_types(Id(800)) -> (R180, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1075              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1075    4: check_types(Id(0)) -> (R180, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1075              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1075

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Unions with possibly unbound paths - Attribute possibly unbound on a subclass but not on a superclass'
MDTEST_TEST_FILTER='attributes.md - Attributes - Unions with possibly unbound paths - Attribute possibly unbound on a subclass but not on a superclass' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Unions with possibly unbound paths - Attribute possibly unbound on a subclass and on a superclass

  crates/ty_python_semantic/resources/mdtest/attributes.md:1093 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:1093 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:1093 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1093 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1093    0: to_overloaded_(Id(4400)) -> (R186, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1093              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:1093    1: infer_definition_types(Id(2765)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1093              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1093    2: infer_definition_types(Id(5fd6)) -> (R186, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1093              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1093    3: infer_scope_types(Id(800)) -> (R186, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1093              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1093    4: check_types(Id(0)) -> (R186, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1093              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1093

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Unions with possibly unbound paths - Attribute possibly unbound on a subclass and on a superclass'
MDTEST_TEST_FILTER='attributes.md - Attributes - Unions with possibly unbound paths - Attribute possibly unbound on a subclass and on a superclass' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Unions with possibly unbound paths - Possibly unbound/undeclared instance attribute - Possibly unbound and undeclared

  crates/ty_python_semantic/resources/mdtest/attributes.md:1120 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:1120 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:1120 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1120 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1120    0: to_overloaded_(Id(4400)) -> (R192, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1120              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:1120    1: infer_definition_types(Id(2765)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1120              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1120    2: infer_definition_types(Id(5fd6)) -> (R192, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1120              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1120    3: infer_scope_types(Id(800)) -> (R192, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1120              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1120    4: check_types(Id(0)) -> (R192, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1120              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1120

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Unions with possibly unbound paths - Possibly unbound/undeclared instance attribute - Possibly unbound and undeclared'
MDTEST_TEST_FILTER='attributes.md - Attributes - Unions with possibly unbound paths - Possibly unbound/undeclared instance attribute - Possibly unbound and undeclared' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Unions with possibly unbound paths - Possibly unbound/undeclared instance attribute - Possibly unbound

  crates/ty_python_semantic/resources/mdtest/attributes.md:1139 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:1139 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:1139 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1139 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1139    0: to_overloaded_(Id(4400)) -> (R198, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1139              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:1139    1: infer_definition_types(Id(2765)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1139              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1139    2: infer_definition_types(Id(5fd6)) -> (R198, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1139              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1139    3: infer_scope_types(Id(800)) -> (R198, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1139              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1139    4: check_types(Id(0)) -> (R198, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1139              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1139

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Unions with possibly unbound paths - Possibly unbound/undeclared instance attribute - Possibly unbound'
MDTEST_TEST_FILTER='attributes.md - Attributes - Unions with possibly unbound paths - Possibly unbound/undeclared instance attribute - Possibly unbound' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Unions with possibly unbound paths - Unions with all paths unbound

  crates/ty_python_semantic/resources/mdtest/attributes.md:1163 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:1163 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:1163 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1163 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1163    0: to_overloaded_(Id(4400)) -> (R204, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1163              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:1163    1: infer_definition_types(Id(2765)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1163              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1163    2: infer_definition_types(Id(5fd6)) -> (R204, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1163              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1163    3: infer_scope_types(Id(800)) -> (R204, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1163              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1163    4: check_types(Id(0)) -> (R204, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1163              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1163

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Unions with possibly unbound paths - Unions with all paths unbound'
MDTEST_TEST_FILTER='attributes.md - Attributes - Unions with possibly unbound paths - Unions with all paths unbound' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Inherited class attributes - Basic

  crates/ty_python_semantic/resources/mdtest/attributes.md:1182 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182    0: to_overloaded_(Id(4400)) -> (R210, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182    1: infer_definition_types(Id(4367)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182    2: member_lookup_with_policy_(Id(3442)) -> (R150, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182    3: infer_expression_types(Id(104a)) -> (R211, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182    4: infer_deferred_types(Id(27d9)) -> (R151, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182    5: explicit_bases_(Id(1419)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182    6: infer_expression_types(Id(1138)) -> (R211, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182    7: infer_scope_types(Id(800)) -> (R211, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182    8: check_types(Id(0)) -> (R210, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1182

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Inherited class attributes - Basic'
MDTEST_TEST_FILTER='attributes.md - Attributes - Inherited class attributes - Basic' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Inherited class attributes - Multiple inheritance

  crates/ty_python_semantic/resources/mdtest/attributes.md:1196 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196    0: to_overloaded_(Id(4400)) -> (R216, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196    1: infer_definition_types(Id(4367)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196    2: member_lookup_with_policy_(Id(3442)) -> (R150, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196    3: infer_expression_types(Id(104a)) -> (R217, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196    4: infer_deferred_types(Id(27d9)) -> (R151, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196    5: explicit_bases_(Id(1419)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196    6: infer_expression_types(Id(1138)) -> (R217, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196    7: infer_scope_types(Id(800)) -> (R217, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196    8: check_types(Id(0)) -> (R216, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1196

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Inherited class attributes - Multiple inheritance'
MDTEST_TEST_FILTER='attributes.md - Attributes - Inherited class attributes - Multiple inheritance' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Intersections of attributes - Attribute only available on one element

  crates/ty_python_semantic/resources/mdtest/attributes.md:1223 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223 expected class
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223    0: explicit_bases_(Id(140b)) -> (R222, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223    1: infer_expression_types(Id(104a)) -> (R145, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223    2: infer_deferred_types(Id(27d9)) -> (R151, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223    3: explicit_bases_(Id(1404)) -> (R223, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223    4: infer_expression_types(Id(112a)) -> (R223, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223    5: infer_definition_types(Id(2de4)) -> (R223, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223    6: symbol_by_id(Id(1c04)) -> (R133, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223    7: member_lookup_with_policy_(Id(341c)) -> (R223, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223    8: infer_definition_types(Id(5fea)) -> (R223, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223    9: infer_scope_types(Id(800)) -> (R223, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223   10: check_types(Id(0)) -> (R222, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1223

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Intersections of attributes - Attribute only available on one element'
MDTEST_TEST_FILTER='attributes.md - Attributes - Intersections of attributes - Attribute only available on one element' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Intersections of attributes - Attribute available on both elements

  crates/ty_python_semantic/resources/mdtest/attributes.md:1245 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245    0: to_overloaded_(Id(4400)) -> (R228, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245    1: infer_definition_types(Id(4367)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245    2: class_member_with_policy_(Id(3809)) -> (R223, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245    3: try_call_dunder_get_(Id(6001)) -> (R229, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245    4: member_lookup_with_policy_(Id(343d)) -> (R229, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245              cycle heads: symbol_by_id(Id(1c04)) -> 0
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245    5: infer_expression_types(Id(112a)) -> (R229, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245              cycle heads: symbol_by_id(Id(1c04)) -> 0
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245    6: infer_definition_types(Id(2de4)) -> (R223, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245    7: symbol_by_id(Id(1c04)) -> (R133, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245    8: member_lookup_with_policy_(Id(342a)) -> (R229, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245    9: infer_definition_types(Id(5ffd)) -> (R229, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245   10: infer_scope_types(Id(800)) -> (R229, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245   11: check_types(Id(0)) -> (R228, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1245

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Intersections of attributes - Attribute available on both elements'
MDTEST_TEST_FILTER='attributes.md - Attributes - Intersections of attributes - Attribute available on both elements' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Intersections of attributes - Possible unboundness

  crates/ty_python_semantic/resources/mdtest/attributes.md:1270 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:1270 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:1270 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1270 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1270    0: to_overloaded_(Id(4400)) -> (R234, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1270              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:1270    1: infer_definition_types(Id(2765)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1270              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1270    2: infer_definition_types(Id(5ff9)) -> (R234, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1270              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1270    3: infer_scope_types(Id(800)) -> (R235, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1270              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1270    4: check_types(Id(0)) -> (R234, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1270              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1270

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Intersections of attributes - Possible unboundness'
MDTEST_TEST_FILTER='attributes.md - Attributes - Intersections of attributes - Possible unboundness' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Intersections of attributes - Intersection of implicit instance attributes

  crates/ty_python_semantic/resources/mdtest/attributes.md:1358 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358    0: to_overloaded_(Id(4400)) -> (R240, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358    1: infer_definition_types(Id(4367)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358    2: class_member_with_policy_(Id(3809)) -> (R223, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358    3: try_call_dunder_get_(Id(6002)) -> (R241, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358    4: member_lookup_with_policy_(Id(3443)) -> (R241, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358              cycle heads: explicit_bases_(Id(1404)) -> 0
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358    5: infer_expression_types(Id(112a)) -> (R241, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358              cycle heads: explicit_bases_(Id(1404)) -> 0
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358    6: infer_definition_types(Id(2de4)) -> (R223, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358    7: symbol_by_id(Id(1c04)) -> (R133, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358    8: member_lookup_with_policy_(Id(3418)) -> (R240, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358    9: infer_deferred_types(Id(27d9)) -> (R240, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358   10: infer_expression_types(Id(107a)) -> (R241, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358   11: symbol_by_id(Id(1c31)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358   12: infer_scope_types(Id(801)) -> (R240, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358   13: check_types(Id(0)) -> (R241, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1358

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Intersections of attributes - Intersection of implicit instance attributes'
MDTEST_TEST_FILTER='attributes.md - Attributes - Intersections of attributes - Intersection of implicit instance attributes' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Attribute access on `Any`

  crates/ty_python_semantic/resources/mdtest/attributes.md:1383 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383    0: to_overloaded_(Id(4400)) -> (R246, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383    1: infer_definition_types(Id(4367)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383    2: class_member_with_policy_(Id(3809)) -> (R223, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383    3: try_call_dunder_get_(Id(6004)) -> (R247, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383    4: member_lookup_with_policy_(Id(343d)) -> (R247, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              cycle heads: explicit_bases_(Id(1404)) -> 0
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383    5: infer_expression_types(Id(112a)) -> (R247, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              cycle heads: explicit_bases_(Id(1404)) -> 0
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383    6: infer_definition_types(Id(2de4)) -> (R223, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383    7: symbol_by_id(Id(1c04)) -> (R133, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383    8: member_lookup_with_policy_(Id(3418)) -> (R246, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383    9: infer_deferred_types(Id(27d9)) -> (R240, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383   10: explicit_bases_(Id(1404)) -> (R223, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383   11: infer_expression_types(Id(10f9)) -> (R247, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383   12: member_lookup_with_policy_(Id(3415)) -> (R247, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383   13: infer_definition_types(Id(5eb7)) -> (R247, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383   14: symbol_by_id(Id(1c0c)) -> (R247, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383   15: infer_scope_types(Id(800)) -> (R247, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383   16: check_types(Id(0)) -> (R246, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1383

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Attribute access on `Any`'
MDTEST_TEST_FILTER='attributes.md - Attributes - Attribute access on `Any`' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Classes with custom `__getattr__` methods - Basic

  crates/ty_python_semantic/resources/mdtest/attributes.md:1414 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/attributes.md:1414 expected function
  crates/ty_python_semantic/resources/mdtest/attributes.md:1414 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1414 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1414    0: to_overloaded_(Id(4400)) -> (R252, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1414              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/attributes.md:1414    1: infer_definition_types(Id(2765)) -> (R133, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1414              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1414    2: infer_definition_types(Id(5ff0)) -> (R253, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1414              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1414    3: infer_scope_types(Id(800)) -> (R253, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1414              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1414    4: check_types(Id(0)) -> (R252, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1414              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1414

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Classes with custom `__getattr__` methods - Basic'
MDTEST_TEST_FILTER='attributes.md - Attributes - Classes with custom `__getattr__` methods - Basic' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Classes with custom `__getattr__` methods - Type of the `name` parameter

  crates/ty_python_semantic/resources/mdtest/attributes.md:1471 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471 expected class
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471    0: pep695_generic_context_(Id(140d)) -> (R258, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471    1: infer_deferred_types(Id(27d9)) -> (R235, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471    2: explicit_bases_(Id(1404)) -> (R223, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471    3: infer_expression_types(Id(112a)) -> (R247, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471    4: infer_definition_types(Id(2de4)) -> (R223, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471    5: symbol_by_id(Id(1c04)) -> (R133, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471    6: try_call_dunder_get_(Id(6005)) -> (R259, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471    7: member_lookup_with_policy_(Id(3433)) -> (R259, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471    8: infer_expression_types(Id(1006)) -> (R259, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471    9: infer_definition_types(Id(5ff0)) -> (R258, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471   10: infer_scope_types(Id(800)) -> (R259, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471   11: check_types(Id(0)) -> (R258, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1471

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Classes with custom `__getattr__` methods - Type of the `name` parameter'
MDTEST_TEST_FILTER='attributes.md - Attributes - Classes with custom `__getattr__` methods - Type of the `name` parameter' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Classes with custom `__getattr__` methods - `argparse.Namespace`

  crates/ty_python_semantic/resources/mdtest/attributes.md:1492 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492 expected class
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492    0: pep695_generic_context_(Id(140d)) -> (R265, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492    1: infer_deferred_types(Id(27d9)) -> (R235, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492    2: explicit_bases_(Id(1404)) -> (R223, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492    3: infer_expression_types(Id(10f9)) -> (R265, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492    4: member_lookup_with_policy_(Id(3436)) -> (R265, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492    5: infer_definition_types(Id(5eb7)) -> (R265, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492    6: symbol_by_id(Id(1c0c)) -> (R247, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492    7: infer_scope_types(Id(801)) -> (R265, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492    8: check_types(Id(0)) -> (R265, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1492

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Classes with custom `__getattr__` methods - `argparse.Namespace`'
MDTEST_TEST_FILTER='attributes.md - Attributes - Classes with custom `__getattr__` methods - `argparse.Namespace`' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Classes with custom `__setattr__` methods - Basic

  crates/ty_python_semantic/resources/mdtest/attributes.md:1506 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506 expected class
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506    0: pep695_generic_context_(Id(140d)) -> (R265, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506    1: infer_deferred_types(Id(27d9)) -> (R235, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506    2: explicit_bases_(Id(1404)) -> (R223, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506    3: infer_expression_types(Id(112a)) -> (R247, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506    4: symbol_by_id(Id(1c2d)) -> (R271, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506    5: member_lookup_with_policy_(Id(343e)) -> (R271, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506    6: infer_definition_types(Id(2de4)) -> (R223, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506    7: symbol_by_id(Id(1c04)) -> (R133, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506    8: try_call_dunder_get_(Id(6006)) -> (R271, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506    9: member_lookup_with_policy_(Id(3437)) -> (R271, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506   10: infer_expression_types(Id(1145)) -> (R271, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506   11: infer_definition_types(Id(7c06)) -> (R270, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506   12: infer_scope_types(Id(800)) -> (R271, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506   13: check_types(Id(0)) -> (R270, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1506

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Classes with custom `__setattr__` methods - Basic'
MDTEST_TEST_FILTER='attributes.md - Attributes - Classes with custom `__setattr__` methods - Basic' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Classes with custom `__setattr__` methods - Type of the `name` parameter

  crates/ty_python_semantic/resources/mdtest/attributes.md:1525 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525 expected class
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525    0: pep695_generic_context_(Id(140d)) -> (R265, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525    1: infer_deferred_types(Id(27d9)) -> (R235, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525    2: explicit_bases_(Id(1404)) -> (R223, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525    3: infer_expression_types(Id(112a)) -> (R247, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525    4: symbol_by_id(Id(1c2e)) -> (R277, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525    5: member_lookup_with_policy_(Id(343e)) -> (R277, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525    6: infer_definition_types(Id(2de4)) -> (R223, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525    7: symbol_by_id(Id(1c04)) -> (R133, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525    8: try_call_dunder_get_(Id(6007)) -> (R277, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525    9: member_lookup_with_policy_(Id(3400)) -> (R277, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525   10: infer_expression_types(Id(1145)) -> (R277, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525   11: infer_definition_types(Id(c02)) -> (R277, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525   12: infer_scope_types(Id(800)) -> (R277, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525   13: check_types(Id(0)) -> (R276, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1525

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Classes with custom `__setattr__` methods - Type of the `name` parameter'
MDTEST_TEST_FILTER='attributes.md - Attributes - Classes with custom `__setattr__` methods - Type of the `name` parameter' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Classes with custom `__setattr__` methods - `argparse.Namespace`

  crates/ty_python_semantic/resources/mdtest/attributes.md:1545 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545 expected class
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545    0: pep695_generic_context_(Id(140d)) -> (R265, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545    1: infer_deferred_types(Id(27d9)) -> (R235, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545    2: explicit_bases_(Id(1404)) -> (R223, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545    3: infer_expression_types(Id(112a)) -> (R247, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545    4: symbol_by_id(Id(1c30)) -> (R283, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545    5: member_lookup_with_policy_(Id(343e)) -> (R283, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545    6: infer_definition_types(Id(2de4)) -> (R223, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545    7: symbol_by_id(Id(1c04)) -> (R133, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545    8: try_call_dunder_get_(Id(6008)) -> (R283, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545    9: member_lookup_with_policy_(Id(342c)) -> (R283, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545   10: infer_scope_types(Id(801)) -> (R283, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545   11: check_types(Id(0)) -> (R283, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1545

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Classes with custom `__setattr__` methods - `argparse.Namespace`'
MDTEST_TEST_FILTER='attributes.md - Attributes - Classes with custom `__setattr__` methods - `argparse.Namespace`' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Objects of all types have a `__class__` method

  crates/ty_python_semantic/resources/mdtest/attributes.md:1557 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557 expected class
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557    0: pep695_generic_context_(Id(140d)) -> (R265, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557    1: infer_deferred_types(Id(27d9)) -> (R235, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557    2: explicit_bases_(Id(1404)) -> (R223, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557    3: infer_expression_types(Id(10f9)) -> (R289, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557    4: member_lookup_with_policy_(Id(3444)) -> (R289, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557    5: infer_definition_types(Id(5eb7)) -> (R289, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557    6: symbol_by_id(Id(1c0c)) -> (R247, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557    7: infer_scope_types(Id(800)) -> (R289, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557    8: check_types(Id(0)) -> (R288, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1557

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Objects of all types have a `__class__` method'
MDTEST_TEST_FILTER='attributes.md - Attributes - Objects of all types have a `__class__` method' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Module attributes - Basic

  crates/ty_python_semantic/resources/mdtest/attributes.md:1608 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/attributes.md:1608 expected class
  crates/ty_python_semantic/resources/mdtest/attributes.md:1608 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1608 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1608    0: pep695_generic_context_(Id(140d)) -> (R265, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1608              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1608    1: infer_deferred_types(Id(2520)) -> (R235, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1608              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/attributes.md:1608    2: explicit_bases_(Id(1418)) -> (R295, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1608              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1608    3: infer_definition_types(Id(7c06)) -> (R295, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1608              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1608    4: infer_scope_types(Id(804d)) -> (R295, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1608              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1608    5: check_types(Id(42)) -> (R292, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1608              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1608
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612 expected class
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612    0: pep695_generic_context_(Id(140d)) -> (R265, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612    1: infer_deferred_types(Id(27d9)) -> (R235, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612    2: explicit_bases_(Id(1404)) -> (R223, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612    3: infer_expression_types(Id(10f9)) -> (R295, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612    4: member_lookup_with_policy_(Id(3444)) -> (R289, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612    5: infer_definition_types(Id(5eb7)) -> (R289, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612    6: symbol_by_id(Id(1c0c)) -> (R247, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612    7: infer_scope_types(Id(800)) -> (R295, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612    8: check_types(Id(0)) -> (R294, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1612

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Module attributes - Basic'
MDTEST_TEST_FILTER='attributes.md - Attributes - Module attributes - Basic' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Module attributes - Nested module attributes

  crates/ty_python_semantic/resources/mdtest/attributes.md:1661 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661 expected class
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661    0: decorators_(Id(1405)) -> (R265, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661    1: infer_expression_types(Id(10f9)) -> (R304, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661    2: member_lookup_with_policy_(Id(3444)) -> (R289, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661    3: infer_definition_types(Id(5eb7)) -> (R304, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661    4: symbol_by_id(Id(1c0c)) -> (R247, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661    5: infer_scope_types(Id(800)) -> (R304, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661    6: check_types(Id(0)) -> (R303, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1661

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Module attributes - Nested module attributes'
MDTEST_TEST_FILTER='attributes.md - Attributes - Module attributes - Nested module attributes' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Literal types - Function-literal attributes

  crates/ty_python_semantic/resources/mdtest/attributes.md:1677 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677    0: pep695_generic_context_(Id(1404)) -> (R304, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677    1: infer_expression_types(Id(10f9)) -> (R304, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677    2: member_lookup_with_policy_(Id(3444)) -> (R289, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677    3: infer_definition_types(Id(5eb7)) -> (R304, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677    4: symbol_by_id(Id(1c0c)) -> (R247, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677    5: infer_scope_types(Id(800)) -> (R319, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677    6: check_types(Id(0)) -> (R318, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1677

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Literal types - Function-literal attributes'
MDTEST_TEST_FILTER='attributes.md - Attributes - Literal types - Function-literal attributes' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Literal types - Int-literal attributes

  crates/ty_python_semantic/resources/mdtest/attributes.md:1696 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696    0: pep695_generic_context_(Id(1404)) -> (R304, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696    1: infer_expression_types(Id(10f9)) -> (R304, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696    2: member_lookup_with_policy_(Id(3444)) -> (R289, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696    3: infer_definition_types(Id(5eb7)) -> (R304, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696    4: symbol_by_id(Id(1c0c)) -> (R247, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696    5: infer_scope_types(Id(800)) -> (R324, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696    6: check_types(Id(0)) -> (R324, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1696

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Literal types - Int-literal attributes'
MDTEST_TEST_FILTER='attributes.md - Attributes - Literal types - Int-literal attributes' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Literal types - Bool-literal attributes

  crates/ty_python_semantic/resources/mdtest/attributes.md:1713 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713    0: pep695_generic_context_(Id(1404)) -> (R304, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713    1: infer_expression_types(Id(10f9)) -> (R304, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713    2: member_lookup_with_policy_(Id(3444)) -> (R289, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713    3: infer_definition_types(Id(5eb7)) -> (R304, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713    4: symbol_by_id(Id(1c0c)) -> (R247, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713    5: infer_scope_types(Id(800)) -> (R330, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713    6: check_types(Id(0)) -> (R330, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1713

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Literal types - Bool-literal attributes'
MDTEST_TEST_FILTER='attributes.md - Attributes - Literal types - Bool-literal attributes' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Literal types - Bytes-literal attributes

  crates/ty_python_semantic/resources/mdtest/attributes.md:1731 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731    0: pep695_generic_context_(Id(1404)) -> (R304, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731    1: infer_expression_types(Id(10f9)) -> (R304, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731    2: member_lookup_with_policy_(Id(3444)) -> (R289, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731    3: infer_definition_types(Id(5eb7)) -> (R304, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731    4: symbol_by_id(Id(1c0c)) -> (R247, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731    5: infer_scope_types(Id(800)) -> (R336, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731    6: check_types(Id(0)) -> (R336, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1731

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Literal types - Bytes-literal attributes'
MDTEST_TEST_FILTER='attributes.md - Attributes - Literal types - Bytes-literal attributes' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Instance attribute edge cases - Assignment to attribute that does not correspond to the instance

  crates/ty_python_semantic/resources/mdtest/attributes.md:1742 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742    0: pep695_generic_context_(Id(1404)) -> (R304, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742    1: infer_expression_types(Id(112a)) -> (R343, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742    2: symbol_by_id(Id(1c16)) -> (R343, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742    3: member_lookup_with_policy_(Id(343e)) -> (R343, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742    4: infer_definition_types(Id(2de4)) -> (R223, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742    5: symbol_by_id(Id(1c04)) -> (R133, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742    6: infer_definition_types(Id(7f14)) -> (R343, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742    7: infer_scope_types(Id(804e)) -> (R343, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742    8: check_types(Id(0)) -> (R343, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1742

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Instance attribute edge cases - Assignment to attribute that does not correspond to the instance'
MDTEST_TEST_FILTER='attributes.md - Attributes - Instance attribute edge cases - Assignment to attribute that does not correspond to the instance' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Instance attribute edge cases - Nested classes

  crates/ty_python_semantic/resources/mdtest/attributes.md:1757 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757 expected class
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757    0: pep695_generic_context_(Id(1404)) -> (R349, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757    1: infer_expression_types(Id(10f9)) -> (R343, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757    2: member_lookup_with_policy_(Id(3444)) -> (R289, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757    3: infer_definition_types(Id(5eb7)) -> (R304, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757    4: symbol_by_id(Id(1c0c)) -> (R247, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757    5: infer_scope_types(Id(800)) -> (R349, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757    6: check_types(Id(0)) -> (R348, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1757

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Instance attribute edge cases - Nested classes'
MDTEST_TEST_FILTER='attributes.md - Attributes - Instance attribute edge cases - Nested classes' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Instance attribute edge cases - Shadowing of `self`

  crates/ty_python_semantic/resources/mdtest/attributes.md:1779 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779    0: pep695_generic_context_(Id(1404)) -> (R304, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779    1: infer_expression_types(Id(112a)) -> (R343, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779    2: symbol_by_id(Id(1c17)) -> (R355, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779    3: member_lookup_with_policy_(Id(343e)) -> (R355, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779    4: infer_definition_types(Id(2de4)) -> (R355, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779    5: symbol_by_id(Id(1c04)) -> (R133, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779    6: infer_definition_types(Id(7f0b)) -> (R354, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779    7: symbol_by_id(Id(1c03)) -> (R355, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779    8: class_member_with_policy_(Id(382e)) -> (R355, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779    9: infer_scope_types(Id(800)) -> (R355, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779   10: check_types(Id(0)) -> (R354, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1779

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Instance attribute edge cases - Shadowing of `self`'
MDTEST_TEST_FILTER='attributes.md - Attributes - Instance attribute edge cases - Shadowing of `self`' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Instance attribute edge cases - Assignment to `self` after nested function

  crates/ty_python_semantic/resources/mdtest/attributes.md:1795 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795    0: pep695_generic_context_(Id(1404)) -> (R304, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795    1: infer_expression_types(Id(10f9)) -> (R343, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795    2: member_lookup_with_policy_(Id(3444)) -> (R289, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795    3: infer_definition_types(Id(5eb7)) -> (R304, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795    4: symbol_by_id(Id(1c0c)) -> (R247, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795    5: infer_scope_types(Id(800)) -> (R361, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795    6: check_types(Id(0)) -> (R360, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1795

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Instance attribute edge cases - Assignment to `self` after nested function'
MDTEST_TEST_FILTER='attributes.md - Attributes - Instance attribute edge cases - Assignment to `self` after nested function' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Instance attribute edge cases - Assignment to `self` from nested function

  crates/ty_python_semantic/resources/mdtest/attributes.md:1810 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/attributes.md:1810 expected class
  crates/ty_python_semantic/resources/mdtest/attributes.md:1810 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1810 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1810    0: pep695_generic_context_(Id(140b)) -> (R366, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1810              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1810    1: infer_scope_types(Id(800)) -> (R367, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1810              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1810    2: check_types(Id(0)) -> (R366, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1810              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1810

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Instance attribute edge cases - Assignment to `self` from nested function'
MDTEST_TEST_FILTER='attributes.md - Attributes - Instance attribute edge cases - Assignment to `self` from nested function' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Instance attribute edge cases - Accessing attributes on `Never`

  crates/ty_python_semantic/resources/mdtest/attributes.md:1826 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:1826 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:1826 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1826 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1826    0: pep695_generic_context_(Id(140b)) -> (R355, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1826              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1826    1: symbol_by_id(Id(1c1e)) -> (R373, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1826              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1826    2: member_lookup_with_policy_(Id(3440)) -> (R373, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1826              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1826    3: infer_definition_types(Id(7f14)) -> (R373, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1826              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1826    4: infer_scope_types(Id(800)) -> (R372, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1826              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1826    5: check_types(Id(0)) -> (R372, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1826              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1826

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Instance attribute edge cases - Accessing attributes on `Never`'
MDTEST_TEST_FILTER='attributes.md - Attributes - Instance attribute edge cases - Accessing attributes on `Never`' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Instance attribute edge cases - Cyclic implicit attributes

  crates/ty_python_semantic/resources/mdtest/attributes.md:1840 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840 expected class
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840    0: pep695_generic_context_(Id(1404)) -> (R379, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840    1: infer_expression_types(Id(10f9)) -> (R343, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840    2: member_lookup_with_policy_(Id(3444)) -> (R289, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840    3: infer_definition_types(Id(5eb7)) -> (R304, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840    4: symbol_by_id(Id(1c0c)) -> (R247, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840    5: infer_scope_types(Id(800)) -> (R379, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840    6: check_types(Id(0)) -> (R378, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1840

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Instance attribute edge cases - Cyclic implicit attributes'
MDTEST_TEST_FILTER='attributes.md - Attributes - Instance attribute edge cases - Cyclic implicit attributes' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

attributes.md - Attributes - Instance attribute edge cases - Builtin types attributes

  crates/ty_python_semantic/resources/mdtest/attributes.md:1926 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/attributes.md:1926 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/attributes.md:1926 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/attributes.md:1926 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/attributes.md:1926    0: pep695_generic_context_(Id(140b)) -> (R355, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1926              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/attributes.md:1926    1: infer_scope_types(Id(800)) -> (R385, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1926              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/attributes.md:1926    2: check_types(Id(0)) -> (R384, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/attributes.md:1926              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/attributes.md:1926

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='attributes.md - Attributes - Instance attribute edge cases - Builtin types attributes'
MDTEST_TEST_FILTER='attributes.md - Attributes - Instance attribute edge cases - Builtin types attributes' cargo test -p ty_python_semantic --test mdtest -- mdtest__attributes

--------------------------------------------------

test mdtest__attributes ... FAILED

failures:

failures:
    mdtest__attributes

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.85s


--- STDERR:              ty_python_semantic::mdtest mdtest__attributes ---

thread 'mdtest__attributes' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        FAIL [   0.339s] ty_python_semantic::mdtest mdtest__diagnostics_unsupported_bool_conversion

--- STDOUT:              ty_python_semantic::mdtest mdtest__diagnostics_unsupported_bool_conversion ---

running 1 test
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ Snapshot Summary ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Snapshot file: crates/ty_python_semantic/resources/mdtest/snapshots/unsupported_bool_conversion.md_-_Different_ways_that_`unsupported-bool-conversion`_can_occur_-_Part_of_a_union_where_at_least_one_member_has_incorrect_`__bool__`_method.snap
Snapshot: unsupported_bool_conversion.md_-_Different_ways_that_`unsupported-bool-conversion`_can_occur_-_Part_of_a_union_where_at_least_one_member_has_incorrect_`__bool__`_method
Source: crates/ty_test/src/lib.rs:394
────────────────────────────────────────────────────────────────────────────────
Expression: snapshot
────────────────────────────────────────────────────────────────────────────────
-old snapshot
+new results
────────────┬───────────────────────────────────────────────────────────────────
   26    26 │
   27    27 │ # Diagnostics
   28    28 │
   29    29 │ ```
         30 │+error: lint:too-many-positional-arguments: Too many positional arguments to bound method `get`: expected 0, got 0
         31 │+  --> src/mdtest_snippet.py:12:12
         32 │+   |
         33 │+11 | def get() -> NotBoolable1 | NotBoolable2 | NotBoolable3:
         34 │+12 |     return NotBoolable2()
         35 │+   |            ^^^^^^^^^^^^^^
         36 │+13 |
         37 │+14 | # error: [unsupported-bool-conversion]
         38 │+   |
         39 │+info: `lint:too-many-positional-arguments` is enabled by default
         40 │+
         41 │+```
         42 │+
         43 │+```
   30    44 │ error: lint:unsupported-bool-conversion: Boolean conversion is unsupported for union `NotBoolable1 | NotBoolable2 | NotBoolable3` because `NotBoolable1` doesn't implement `__bool__` correctly
   31    45 │   --> src/mdtest_snippet.py:15:8
   32    46 │    |
   33    47 │ 14 | # error: [unsupported-bool-conversion]
────────────┴───────────────────────────────────────────────────────────────────
To update snapshots run `cargo insta review`
Stopped on the first failure. Run `cargo insta test` to run all snapshots.
test mdtest__diagnostics_unsupported_bool_conversion ... FAILED

failures:

failures:
    mdtest__diagnostics_unsupported_bool_conversion

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.33s


--- STDERR:              ty_python_semantic::mdtest mdtest__diagnostics_unsupported_bool_conversion ---
stored new snapshot /home/ibraheem/dev/astral/ruff/crates/ty_python_semantic/resources/mdtest/snapshots/unsupported_bool_conversion.md_-_Different_ways_that_`unsupported-bool-conversion`_can_occur_-_Part_of_a_union_where_at_least_one_member_has_incorrect_`__bool__`_method.snap.new

thread 'mdtest__diagnostics_unsupported_bool_conversion' panicked at /home/ibraheem/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/insta-1.42.2/src/runtime.rs:679:13:
snapshot assertion for 'unsupported_bool_conversion.md_-_Different_ways_that_`unsupported-bool-conversion`_can_occur_-_Part_of_a_union_where_at_least_one_member_has_incorrect_`__bool__`_method' failed in line 394
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.239s] ty_python_semantic::mdtest mdtest__doc_public_type_undeclared_symbols
        PASS [   0.203s] ty_python_semantic::mdtest mdtest__exception_invalid_syntax
        PASS [   0.208s] ty_python_semantic::mdtest mdtest__exception_except_star
        PASS [   0.014s] ty_python_semantic::mdtest mdtest__generics_builtins
        PASS [   0.171s] ty_python_semantic::mdtest mdtest__expression_assert
        PASS [   0.405s] ty_python_semantic::mdtest mdtest__diagnostics_version_related_syntax_errors
        FAIL [   0.490s] ty_python_semantic::mdtest mdtest__descriptor_protocol

--- STDOUT:              ty_python_semantic::mdtest mdtest__descriptor_protocol ---

running 1 test

descriptor_protocol.md - Descriptor protocol - Descriptors distinguishing between class and instance access

  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:464 unmatched assertion: revealed: Literal["called on instance"]
  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:464 unexpected error: 13 [revealed-type] "Revealed type: `Literal["called on class object"]`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='descriptor_protocol.md - Descriptor protocol - Descriptors distinguishing between class and instance access'
MDTEST_TEST_FILTER='descriptor_protocol.md - Descriptor protocol - Descriptors distinguishing between class and instance access' cargo test -p ty_python_semantic --test mdtest -- mdtest__descriptor_protocol

descriptor_protocol.md - Descriptor protocol - Special descriptors - Built-in `property` descriptor

  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:514 unmatched assertion: revealed: str | None
  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:514 unexpected error: 13 [revealed-type] "Revealed type: `str | Unknown`"
  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:516 unmatched assertion: revealed: property
  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:516 unexpected error: 13 [revealed-type] "Revealed type: `str`"
  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:522 unmatched assertion: error: [invalid-assignment] "Invalid assignment to data descriptor attribute `name` on type `C` with custom `__set__` method"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='descriptor_protocol.md - Descriptor protocol - Special descriptors - Built-in `property` descriptor'
MDTEST_TEST_FILTER='descriptor_protocol.md - Descriptor protocol - Special descriptors - Built-in `property` descriptor' cargo test -p ty_python_semantic --test mdtest -- mdtest__descriptor_protocol

descriptor_protocol.md - Descriptor protocol - Special descriptors - Functions as descriptors

  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:566 unmatched assertion: revealed: def f(x: object) -> str
  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:566 unexpected error: 13 [revealed-type] "Revealed type: `bound method Unknown.f() -> str`"
  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:567 unexpected error: 38 [too-many-positional-arguments] "Too many positional arguments to bound method `f`: expected 0, got 1"
  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:572 unmatched assertion: revealed: def f(x: object) -> str
  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:572 unexpected error: 13 [revealed-type] "Revealed type: `bound method Unknown.f() -> str`"
  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:611 unmatched assertion: error: [no-matching-overload] "No overload of wrapper descriptor `FunctionType.__get__` matches arguments"
  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:622 unmatched assertion: error: [no-matching-overload] "No overload of wrapper descriptor `FunctionType.__get__` matches arguments"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='descriptor_protocol.md - Descriptor protocol - Special descriptors - Functions as descriptors'
MDTEST_TEST_FILTER='descriptor_protocol.md - Descriptor protocol - Special descriptors - Functions as descriptors' cargo test -p ty_python_semantic --test mdtest -- mdtest__descriptor_protocol

descriptor_protocol.md - Descriptor protocol - Error handling and edge cases - `__get__` is called with correct arguments

  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:662 unmatched assertion: revealed: TailoredForClassObjectAccess
  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:662 unexpected error: 13 [revealed-type] "Revealed type: `int`"
  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:663 unmatched assertion: revealed: TailoredForInstanceAccess
  crates/ty_python_semantic/resources/mdtest/descriptor_protocol.md:663 unexpected error: 13 [revealed-type] "Revealed type: `str`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='descriptor_protocol.md - Descriptor protocol - Error handling and edge cases - `__get__` is called with correct arguments'
MDTEST_TEST_FILTER='descriptor_protocol.md - Descriptor protocol - Error handling and edge cases - `__get__` is called with correct arguments' cargo test -p ty_python_semantic --test mdtest -- mdtest__descriptor_protocol

--------------------------------------------------

test mdtest__descriptor_protocol ... FAILED

failures:

failures:
    mdtest__descriptor_protocol

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.48s


--- STDERR:              ty_python_semantic::mdtest mdtest__descriptor_protocol ---

thread 'mdtest__descriptor_protocol' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.180s] ty_python_semantic::mdtest mdtest__expression_attribute
        FAIL [   0.296s] ty_python_semantic::mdtest mdtest__exception_basic

--- STDOUT:              ty_python_semantic::mdtest mdtest__exception_basic ---

running 1 test

basic.md - Exception Handling - Dynamic exception types

  crates/ty_python_semantic/resources/mdtest/exception/basic.md:50 unexpected error: [missing-argument] "No argument provided for required parameter `self` of function `__call__`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='basic.md - Exception Handling - Dynamic exception types'
MDTEST_TEST_FILTER='basic.md - Exception Handling - Dynamic exception types' cargo test -p ty_python_semantic --test mdtest -- mdtest__exception_basic

basic.md - Exception Handling - Invalid exception handlers

  crates/ty_python_semantic/resources/mdtest/exception/basic.md:82 unexpected error: [missing-argument] "No argument provided for required parameter `self` of function `__call__`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='basic.md - Exception Handling - Invalid exception handlers'
MDTEST_TEST_FILTER='basic.md - Exception Handling - Invalid exception handlers' cargo test -p ty_python_semantic --test mdtest -- mdtest__exception_basic

basic.md - Exception Handling - Object raised is not an exception

  crates/ty_python_semantic/resources/mdtest/exception/basic.md:98 unexpected error: [missing-argument] "No argument provided for required parameter `self` of function `__init__`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='basic.md - Exception Handling - Object raised is not an exception'
MDTEST_TEST_FILTER='basic.md - Exception Handling - Object raised is not an exception' cargo test -p ty_python_semantic --test mdtest -- mdtest__exception_basic

basic.md - Exception Handling - Exception cause is not an exception

  crates/ty_python_semantic/resources/mdtest/exception/basic.md:129 unexpected error: [too-many-positional-arguments] "Too many positional arguments to function `_`: expected 0, got 0"
  crates/ty_python_semantic/resources/mdtest/exception/basic.md:135 unexpected error: [too-many-positional-arguments] "Too many positional arguments to function `_`: expected 0, got 0"
  crates/ty_python_semantic/resources/mdtest/exception/basic.md:141 unexpected error: [too-many-positional-arguments] "Too many positional arguments to function `_`: expected 0, got 0"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='basic.md - Exception Handling - Exception cause is not an exception'
MDTEST_TEST_FILTER='basic.md - Exception Handling - Exception cause is not an exception' cargo test -p ty_python_semantic --test mdtest -- mdtest__exception_basic

--------------------------------------------------

test mdtest__exception_basic ... FAILED

failures:

failures:
    mdtest__exception_basic

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.26s


--- STDERR:              ty_python_semantic::mdtest mdtest__exception_basic ---

thread 'mdtest__exception_basic' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        FAIL [   0.278s] ty_python_semantic::mdtest mdtest__exception_control_flow

--- STDOUT:              ty_python_semantic::mdtest mdtest__exception_control_flow ---

running 1 test

control_flow.md - Control flow for exception handlers - Combining an `except` branch with a `finally` branch

  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:249 unexpected error: [missing-argument] "No argument provided for required parameter `self` of function `__init__`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:252 unexpected error: [missing-argument] "No argument provided for required parameter `self` of function `__init__`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:255 unexpected error: [missing-argument] "No argument provided for required parameter `self` of function `__init__`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:311 unexpected error: [missing-argument] "No argument provided for required parameter `self` of function `__init__`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:314 unexpected error: [missing-argument] "No argument provided for required parameter `self` of function `__init__`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='control_flow.md - Control flow for exception handlers - Combining an `except` branch with a `finally` branch'
MDTEST_TEST_FILTER='control_flow.md - Control flow for exception handlers - Combining an `except` branch with a `finally` branch' cargo test -p ty_python_semantic --test mdtest -- mdtest__exception_control_flow

control_flow.md - Control flow for exception handlers - Combining `except`, `else` and `finally` branches

  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348 expected function
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348    0: to_overloaded_(Id(4001)) -> (R36, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348    1: class_member_with_policy_(Id(5c01)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348    2: try_call_dunder_get_(Id(7000)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348    3: member_lookup_with_policy_(Id(3803)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348    4: infer_expression_types(Id(1031)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348    5: infer_definition_types(Id(f30)) -> (R1, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348    6: infer_deferred_types(Id(2967)) -> (R31, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348    7: explicit_bases_(Id(200d)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348    8: infer_expression_types(Id(112f)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348    9: infer_scope_types(Id(800)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348   10: check_types(Id(0)) -> (R36, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:348

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='control_flow.md - Control flow for exception handlers - Combining `except`, `else` and `finally` branches'
MDTEST_TEST_FILTER='control_flow.md - Control flow for exception handlers - Combining `except`, `else` and `finally` branches' cargo test -p ty_python_semantic --test mdtest -- mdtest__exception_control_flow

control_flow.md - Control flow for exception handlers - Nested `try`/`except` blocks

  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:461 unexpected error: [too-many-positional-arguments] "Too many positional arguments to bound method `could_raise_returns_C`: expected 0, got 0"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:464 unexpected error: [too-many-positional-arguments] "Too many positional arguments to bound method `could_raise_returns_C`: expected 0, got 0"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:467 unexpected error: [too-many-positional-arguments] "Too many positional arguments to bound method `could_raise_returns_C`: expected 0, got 0"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:470 unexpected error: [too-many-positional-arguments] "Too many positional arguments to bound method `could_raise_returns_C`: expected 0, got 0"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:473 unexpected error: [too-many-positional-arguments] "Too many positional arguments to bound method `could_raise_returns_C`: expected 0, got 0"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:476 unexpected error: [too-many-positional-arguments] "Too many positional arguments to bound method `could_raise_returns_C`: expected 0, got 0"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:479 unexpected error: [too-many-positional-arguments] "Too many positional arguments to bound method `could_raise_returns_C`: expected 0, got 0"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:482 unexpected error: [too-many-positional-arguments] "Too many positional arguments to bound method `could_raise_returns_C`: expected 0, got 0"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:485 unexpected error: [too-many-positional-arguments] "Too many positional arguments to bound method `could_raise_returns_C`: expected 0, got 0"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:488 unexpected error: [too-many-positional-arguments] "Too many positional arguments to bound method `could_raise_returns_C`: expected 0, got 0"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:491 unexpected error: [too-many-positional-arguments] "Too many positional arguments to bound method `could_raise_returns_C`: expected 0, got 0"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:497 unexpected error: 21 [revealed-type] "Revealed type: `Literal[1]`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:499 unexpected error: 21 [revealed-type] "Revealed type: `A`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:501 unexpected error: 21 [revealed-type] "Revealed type: `Literal[1] | A`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:503 unexpected error: 21 [revealed-type] "Revealed type: `B`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:505 unexpected error: 21 [revealed-type] "Revealed type: `C`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:507 unexpected error: 21 [revealed-type] "Revealed type: `Literal[1] | A`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:509 unexpected error: 21 [revealed-type] "Revealed type: `D`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:511 unexpected error: 21 [revealed-type] "Revealed type: `E`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:513 unexpected error: 21 [revealed-type] "Revealed type: `A`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:515 unexpected error: 21 [revealed-type] "Revealed type: `F`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:517 unexpected error: 21 [revealed-type] "Revealed type: `G`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:520 unexpected error: 21 [revealed-type] "Revealed type: `C | E | G`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:522 unexpected error: 21 [revealed-type] "Revealed type: `Literal[2]`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:523 unexpected error: 17 [revealed-type] "Revealed type: `Literal[2]`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:525 unexpected error: 17 [revealed-type] "Revealed type: `Literal[1, 2] | A | B | C | D | E | F | G`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:527 unexpected error: 17 [revealed-type] "Revealed type: `H`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:529 unexpected error: 17 [revealed-type] "Revealed type: `I`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:531 unexpected error: 17 [revealed-type] "Revealed type: `Literal[2]`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:533 unexpected error: 17 [revealed-type] "Revealed type: `J`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:535 unexpected error: 17 [revealed-type] "Revealed type: `K`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:538 unexpected error: 17 [revealed-type] "Revealed type: `I | K`"
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:542 unexpected error: 13 [revealed-type] "Revealed type: `I | K`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='control_flow.md - Control flow for exception handlers - Nested `try`/`except` blocks'
MDTEST_TEST_FILTER='control_flow.md - Control flow for exception handlers - Nested `try`/`except` blocks' cargo test -p ty_python_semantic --test mdtest -- mdtest__exception_control_flow

control_flow.md - Control flow for exception handlers - Nested scopes inside `try` blocks

  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:551 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:551 expected class
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:551 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:551 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:551    0: pep695_generic_context_(Id(2002)) -> (R48, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:551              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:551    1: infer_expression_types(Id(112f)) -> (R49, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:551              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:551    2: infer_scope_types(Id(800)) -> (R49, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:551              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:551    3: check_types(Id(0)) -> (R48, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:551              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/exception/control_flow.md:551

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='control_flow.md - Control flow for exception handlers - Nested scopes inside `try` blocks'
MDTEST_TEST_FILTER='control_flow.md - Control flow for exception handlers - Nested scopes inside `try` blocks' cargo test -p ty_python_semantic --test mdtest -- mdtest__exception_control_flow

--------------------------------------------------

test mdtest__exception_control_flow ... FAILED

failures:

failures:
    mdtest__exception_control_flow

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.26s


--- STDERR:              ty_python_semantic::mdtest mdtest__exception_control_flow ---

thread 'mdtest__exception_control_flow' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.378s] ty_python_semantic::mdtest mdtest__directives_assert_never
        PASS [   0.130s] ty_python_semantic::mdtest mdtest__final
        PASS [   0.202s] ty_python_semantic::mdtest mdtest__expression_lambda
        FAIL [   0.884s] ty_python_semantic::mdtest mdtest__call_methods

--- STDOUT:              ty_python_semantic::mdtest mdtest__call_methods ---

running 1 test

methods.md - Methods - Method calls on literals - String literals

  crates/ty_python_semantic/resources/mdtest/call/methods.md:142 unmatched assertion: revealed: int
  crates/ty_python_semantic/resources/mdtest/call/methods.md:142 unexpected error: 13 [revealed-type] "Revealed type: `SupportsIndex`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='methods.md - Methods - Method calls on literals - String literals'
MDTEST_TEST_FILTER='methods.md - Methods - Method calls on literals - String literals' cargo test -p ty_python_semantic --test mdtest -- mdtest__call_methods

methods.md - Methods - Method calls on `LiteralString`

  crates/ty_python_semantic/resources/mdtest/call/methods.md:160 unmatched assertion: revealed: int
  crates/ty_python_semantic/resources/mdtest/call/methods.md:160 unexpected error: 17 [revealed-type] "Revealed type: `SupportsIndex`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='methods.md - Methods - Method calls on `LiteralString`'
MDTEST_TEST_FILTER='methods.md - Methods - Method calls on `LiteralString`' cargo test -p ty_python_semantic --test mdtest -- mdtest__call_methods

methods.md - Methods - Method calls on `tuple`

  crates/ty_python_semantic/resources/mdtest/call/methods.md:167 unmatched assertion: revealed: int
  crates/ty_python_semantic/resources/mdtest/call/methods.md:167 unexpected error: 17 [missing-argument] "No argument provided for required parameter `value` of function `index`"
  crates/ty_python_semantic/resources/mdtest/call/methods.md:167 unexpected error: 17 [revealed-type] "Revealed type: `SupportsIndex`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='methods.md - Methods - Method calls on `tuple`'
MDTEST_TEST_FILTER='methods.md - Methods - Method calls on `tuple`' cargo test -p ty_python_semantic --test mdtest -- mdtest__call_methods

methods.md - Methods - Method calls on unions

  crates/ty_python_semantic/resources/mdtest/call/methods.md:184 unmatched assertion: revealed: (bound method A.f() -> int) | (bound method B.f() -> str)
  crates/ty_python_semantic/resources/mdtest/call/methods.md:184 unexpected error: 17 [revealed-type] "Revealed type: `(def f(self) -> SupportsIndex) | (def f(self) -> str)`"
  crates/ty_python_semantic/resources/mdtest/call/methods.md:185 unmatched assertion: revealed: int | str
  crates/ty_python_semantic/resources/mdtest/call/methods.md:185 unexpected error: 17 [missing-argument] "No argument provided for required parameter `self` of function `f`"
  crates/ty_python_semantic/resources/mdtest/call/methods.md:185 unexpected error: 17 [revealed-type] "Revealed type: `SupportsIndex | str`"
  crates/ty_python_semantic/resources/mdtest/call/methods.md:187 unmatched assertion: revealed: Any | (bound method A.f() -> int)
  crates/ty_python_semantic/resources/mdtest/call/methods.md:187 unexpected error: 17 [revealed-type] "Revealed type: `Any | (def f(self) -> SupportsIndex)`"
  crates/ty_python_semantic/resources/mdtest/call/methods.md:188 unmatched assertion: revealed: Any | int
  crates/ty_python_semantic/resources/mdtest/call/methods.md:188 unexpected error: 17 [missing-argument] "No argument provided for required parameter `self` of function `f`"
  crates/ty_python_semantic/resources/mdtest/call/methods.md:188 unexpected error: 17 [revealed-type] "Revealed type: `Any | SupportsIndex`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='methods.md - Methods - Method calls on unions'
MDTEST_TEST_FILTER='methods.md - Methods - Method calls on unions' cargo test -p ty_python_semantic --test mdtest -- mdtest__call_methods

--------------------------------------------------

test mdtest__call_methods ... FAILED

failures:

failures:
    mdtest__call_methods

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.88s


--- STDERR:              ty_python_semantic::mdtest mdtest__call_methods ---

thread 'mdtest__call_methods' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.223s] ty_python_semantic::mdtest mdtest__generics_legacy_variance
        PASS [   0.256s] ty_python_semantic::mdtest mdtest__generics_pep695_variance
        FAIL [   0.317s] ty_python_semantic::mdtest mdtest__generics_legacy_functions

--- STDOUT:              ty_python_semantic::mdtest mdtest__generics_legacy_functions ---

running 1 test
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ Snapshot Summary ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Snapshot file: crates/ty_python_semantic/resources/mdtest/snapshots/functions.md_-_Generic_functions___Legacy_syntax_-_Inferring_a_constrained_typevar.snap
Snapshot: functions.md_-_Generic_functions___Legacy_syntax_-_Inferring_a_constrained_typevar
Source: crates/ty_test/src/lib.rs:394
────────────────────────────────────────────────────────────────────────────────
Expression: snapshot
────────────────────────────────────────────────────────────────────────────────
-old snapshot
+new results
────────────┬───────────────────────────────────────────────────────────────────
   57    57 │    |
   58    58 │  9 | reveal_type(f(1))  # revealed: int
   59    59 │ 10 | reveal_type(f(True))  # revealed: int
   60    60 │ 11 | reveal_type(f(None))  # revealed: None
   61       │-   |             ^^^^^^^ `None`
         61 │+   |             ^^^^^^^ `int`
   62    62 │ 12 | # error: [invalid-argument-type]
   63    63 │ 13 | reveal_type(f("string"))  # revealed: Unknown
   64    64 │    |
   65    65 │
┈┈┈┈┈┈┈┈┈┈┈┈┼┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈
   74    74 │ 13 | reveal_type(f("string"))  # revealed: Unknown
   75    75 │    |             ^^^^^^^^^^^ `Unknown`
   76    76 │    |
   77    77 │
   78       │-```
   79       │-
   80       │-```
   81       │-error: lint:invalid-argument-type: Argument to this function is incorrect
   82       │-  --> src/mdtest_snippet.py:13:15
   83       │-   |
   84       │-11 | reveal_type(f(None))  # revealed: None
   85       │-12 | # error: [invalid-argument-type]
   86       │-13 | reveal_type(f("string"))  # revealed: Unknown
   87       │-   |               ^^^^^^^^ Argument type `Literal["string"]` does not satisfy constraints of type variable `T`
   88       │-   |
   89       │-info: Type variable defined here
   90       │- --> src/mdtest_snippet.py:4:1
   91       │-  |
   92       │-2 | from typing_extensions import reveal_type
   93       │-3 |
   94       │-4 | T = TypeVar("T", int, None)
   95       │-  | ^
   96       │-5 |
   97       │-6 | def f(x: T) -> T:
   98       │-  |
   99       │-info: `lint:invalid-argument-type` is enabled by default
  100       │-
  101    78 │ ```
────────────┴───────────────────────────────────────────────────────────────────
To update snapshots run `cargo insta review`
Stopped on the first failure. Run `cargo insta test` to run all snapshots.
test mdtest__generics_legacy_functions ... FAILED

failures:

failures:
    mdtest__generics_legacy_functions

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.31s


--- STDERR:              ty_python_semantic::mdtest mdtest__generics_legacy_functions ---
stored new snapshot /home/ibraheem/dev/astral/ruff/crates/ty_python_semantic/resources/mdtest/snapshots/functions.md_-_Generic_functions___Legacy_syntax_-_Inferring_a_constrained_typevar.snap.new

thread 'mdtest__generics_legacy_functions' panicked at /home/ibraheem/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/insta-1.42.2/src/runtime.rs:679:13:
snapshot assertion for 'functions.md_-_Generic_functions___Legacy_syntax_-_Inferring_a_constrained_typevar' failed in line 394
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        FAIL [   0.248s] ty_python_semantic::mdtest mdtest__import_basic

--- STDOUT:              ty_python_semantic::mdtest mdtest__import_basic ---

running 1 test

basic.md - Structures - Deeply nested

  crates/ty_python_semantic/resources/mdtest/import/basic.md:55 panicked at crates/ty_python_semantic/src/semantic_index/use_def.rs:450:47
  crates/ty_python_semantic/resources/mdtest/import/basic.md:55 index out of bounds: the len is 0 but the index is 0
  crates/ty_python_semantic/resources/mdtest/import/basic.md:55 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/import/basic.md:55 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/import/basic.md:55    0: symbol_by_id(Id(2003)) -> (R30, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/import/basic.md:55              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/import/basic.md:55    1: infer_scope_types(Id(800)) -> (R40, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/import/basic.md:55              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/import/basic.md:55    2: check_types(Id(0)) -> (R33, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/import/basic.md:55              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/import/basic.md:55
  crates/ty_python_semantic/resources/mdtest/import/basic.md:73 panicked at crates/ty_python_semantic/src/semantic_index/use_def.rs:450:47
  crates/ty_python_semantic/resources/mdtest/import/basic.md:73 index out of bounds: the len is 0 but the index is 0
  crates/ty_python_semantic/resources/mdtest/import/basic.md:73 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/import/basic.md:73 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/import/basic.md:73    0: symbol_by_id(Id(2003)) -> (R30, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/import/basic.md:73              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/import/basic.md:73    1: infer_scope_types(Id(3b46)) -> (R40, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/import/basic.md:73              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/import/basic.md:73    2: check_types(Id(3a)) -> (R40, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/import/basic.md:73              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/import/basic.md:73

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='basic.md - Structures - Deeply nested'
MDTEST_TEST_FILTER='basic.md - Structures - Deeply nested' cargo test -p ty_python_semantic --test mdtest -- mdtest__import_basic

--------------------------------------------------

test mdtest__import_basic ... FAILED

failures:

failures:
    mdtest__import_basic

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.24s


--- STDERR:              ty_python_semantic::mdtest mdtest__import_basic ---

thread 'mdtest__import_basic' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.102s] ty_python_semantic::mdtest mdtest__import_case_sensitive
        PASS [   0.391s] ty_python_semantic::mdtest mdtest__function_parameters
        FAIL [   0.333s] ty_python_semantic::mdtest mdtest__generics_pep695_functions

--- STDOUT:              ty_python_semantic::mdtest mdtest__generics_pep695_functions ---

running 1 test
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ Snapshot Summary ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Snapshot file: crates/ty_python_semantic/resources/mdtest/snapshots/functions.md_-_Generic_functions___PEP_695_syntax_-_Inferring_a_constrained_typevar.snap
Snapshot: functions.md_-_Generic_functions___PEP_695_syntax_-_Inferring_a_constrained_typevar
Source: crates/ty_test/src/lib.rs:394
────────────────────────────────────────────────────────────────────────────────
Expression: snapshot
────────────────────────────────────────────────────────────────────────────────
-old snapshot
+new results
────────────┬───────────────────────────────────────────────────────────────────
   27    27 │   |
   28    28 │ 4 |     return x
   29    29 │ 5 |
   30    30 │ 6 | reveal_type(f(1))  # revealed: int
   31       │-  |             ^^^^ `int`
         31 │+  |             ^^^^ `MutableSequence`
   32    32 │ 7 | reveal_type(f(True))  # revealed: int
   33    33 │ 8 | reveal_type(f(None))  # revealed: None
   34    34 │   |
   35    35 │
┈┈┈┈┈┈┈┈┈┈┈┈┼┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈
   40    40 │  --> src/mdtest_snippet.py:7:13
   41    41 │   |
   42    42 │ 6 | reveal_type(f(1))  # revealed: int
   43    43 │ 7 | reveal_type(f(True))  # revealed: int
   44       │-  |             ^^^^^^^ `int`
         44 │+  |             ^^^^^^^ `MutableSequence`
   45    45 │ 8 | reveal_type(f(None))  # revealed: None
   46    46 │ 9 | # error: [invalid-argument-type]
   47    47 │   |
   48    48 │
────────────┴───────────────────────────────────────────────────────────────────
To update snapshots run `cargo insta review`
Stopped on the first failure. Run `cargo insta test` to run all snapshots.
test mdtest__generics_pep695_functions ... FAILED

failures:

failures:
    mdtest__generics_pep695_functions

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.33s


--- STDERR:              ty_python_semantic::mdtest mdtest__generics_pep695_functions ---
stored new snapshot /home/ibraheem/dev/astral/ruff/crates/ty_python_semantic/resources/mdtest/snapshots/functions.md_-_Generic_functions___PEP_695_syntax_-_Inferring_a_constrained_typevar.snap.new

thread 'mdtest__generics_pep695_functions' panicked at /home/ibraheem/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/insta-1.42.2/src/runtime.rs:679:13:
snapshot assertion for 'functions.md_-_Generic_functions___PEP_695_syntax_-_Inferring_a_constrained_typevar' failed in line 394
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        FAIL [   0.357s] ty_python_semantic::mdtest mdtest__generics_scoping

--- STDOUT:              ty_python_semantic::mdtest mdtest__generics_scoping ---

running 1 test

scoping.md - Scoping rules for type variables - Functions on generic classes are descriptors

  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:98 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:98 expected function
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:98 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:98 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:98    0: to_overloaded_(Id(2405)) -> (R24, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:98              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:98    1: infer_scope_types(Id(800)) -> (R25, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:98              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:98    2: check_types(Id(0)) -> (R24, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:98              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:98

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='scoping.md - Scoping rules for type variables - Functions on generic classes are descriptors'
MDTEST_TEST_FILTER='scoping.md - Scoping rules for type variables - Functions on generic classes are descriptors' cargo test -p ty_python_semantic --test mdtest -- mdtest__generics_scoping

scoping.md - Scoping rules for type variables - Methods can mention other typevars

  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:135 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:135 expected function
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:135 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:135 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:135    0: to_overloaded_(Id(2405)) -> (R30, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:135              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:135    1: infer_scope_types(Id(800)) -> (R31, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:135              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:135    2: check_types(Id(0)) -> (R30, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:135              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/generics/scoping.md:135

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='scoping.md - Scoping rules for type variables - Methods can mention other typevars'
MDTEST_TEST_FILTER='scoping.md - Scoping rules for type variables - Methods can mention other typevars' cargo test -p ty_python_semantic --test mdtest -- mdtest__generics_scoping

--------------------------------------------------

test mdtest__generics_scoping ... FAILED

failures:

failures:
    mdtest__generics_scoping

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.35s


--- STDERR:              ty_python_semantic::mdtest mdtest__generics_scoping ---

thread 'mdtest__generics_scoping' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.305s] ty_python_semantic::mdtest mdtest__import_builtins
        FAIL [   0.448s] ty_python_semantic::mdtest mdtest__generics_legacy_classes

--- STDOUT:              ty_python_semantic::mdtest mdtest__generics_legacy_classes ---

running 1 test

classes.md - Generic classes: Legacy syntax - Inferring generic class parameters from constructors - Both present, `__new__` inherited from a generic base class

  crates/ty_python_semantic/resources/mdtest/generics/legacy/classes.md:322 unmatched assertion: revealed: D[Literal[1]]
  crates/ty_python_semantic/resources/mdtest/generics/legacy/classes.md:322 unexpected error: 13 [revealed-type] "Revealed type: `D[Unknown]`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='classes.md - Generic classes: Legacy syntax - Inferring generic class parameters from constructors - Both present, `__new__` inherited from a generic base class'
MDTEST_TEST_FILTER='classes.md - Generic classes: Legacy syntax - Inferring generic class parameters from constructors - Both present, `__new__` inherited from a generic base class' cargo test -p ty_python_semantic --test mdtest -- mdtest__generics_legacy_classes

classes.md - Generic classes: Legacy syntax - Cyclic class definitions - Cyclic inheritance as a generic parameter

  crates/ty_python_semantic/resources/mdtest/generics/legacy/classes.md:489 unexpected error: [call-non-callable] "Method `__class_getitem__` of type `bound method Unknown.__class_getitem__(item: Any, /) -> GenericAlias` is not callable on object of type `<class 'list'>`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='classes.md - Generic classes: Legacy syntax - Cyclic class definitions - Cyclic inheritance as a generic parameter'
MDTEST_TEST_FILTER='classes.md - Generic classes: Legacy syntax - Cyclic class definitions - Cyclic inheritance as a generic parameter' cargo test -p ty_python_semantic --test mdtest -- mdtest__generics_legacy_classes

--------------------------------------------------

test mdtest__generics_legacy_classes ... FAILED

failures:

failures:
    mdtest__generics_legacy_classes

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.44s


--- STDERR:              ty_python_semantic::mdtest mdtest__generics_legacy_classes ---

thread 'mdtest__generics_legacy_classes' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.172s] ty_python_semantic::mdtest mdtest__import_conflicts
        FAIL [   0.198s] ty_python_semantic::mdtest mdtest__import_conditional

--- STDOUT:              ty_python_semantic::mdtest mdtest__import_conditional ---

running 1 test

conditional.md - Conditional imports - Reimport with stub declaration

  crates/ty_python_semantic/resources/mdtest/import/conditional.md:134 panicked at crates/ty_python_semantic/src/semantic_index/use_def.rs:450:47
  crates/ty_python_semantic/resources/mdtest/import/conditional.md:134 index out of bounds: the len is 0 but the index is 0
  crates/ty_python_semantic/resources/mdtest/import/conditional.md:134 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/import/conditional.md:134 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/import/conditional.md:134    0: symbol_by_id(Id(1c04)) -> (R33, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/import/conditional.md:134              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/import/conditional.md:134    1: infer_scope_types(Id(3341)) -> (R46, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/import/conditional.md:134              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/import/conditional.md:134    2: check_types(Id(1)) -> (R45, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/import/conditional.md:134              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/import/conditional.md:134

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='conditional.md - Conditional imports - Reimport with stub declaration'
MDTEST_TEST_FILTER='conditional.md - Conditional imports - Reimport with stub declaration' cargo test -p ty_python_semantic --test mdtest -- mdtest__import_conditional

--------------------------------------------------

test mdtest__import_conditional ... FAILED

failures:

failures:
    mdtest__import_conditional

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.19s


--- STDERR:              ty_python_semantic::mdtest mdtest__import_conditional ---

thread 'mdtest__import_conditional' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.164s] ty_python_semantic::mdtest mdtest__import_invalid_syntax
        FAIL [   1.040s] ty_python_semantic::mdtest mdtest__dataclasses

--- STDOUT:              ty_python_semantic::mdtest mdtest__dataclasses ---

running 1 test

dataclasses.md - Dataclasses - Other dataclass parameters - `repr`

  crates/ty_python_semantic/resources/mdtest/dataclasses.md:280 unmatched assertion: revealed: bound method WithoutRepr.__repr__() -> str
  crates/ty_python_semantic/resources/mdtest/dataclasses.md:280 unexpected error: 13 [revealed-type] "Revealed type: `def __repr__(self) -> str`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='dataclasses.md - Dataclasses - Other dataclass parameters - `repr`'
MDTEST_TEST_FILTER='dataclasses.md - Dataclasses - Other dataclass parameters - `repr`' cargo test -p ty_python_semantic --test mdtest -- mdtest__dataclasses

dataclasses.md - Dataclasses - Internals

  crates/ty_python_semantic/resources/mdtest/dataclasses.md:726 unmatched assertion: revealed: (name: str, age: int | None = None) -> None
  crates/ty_python_semantic/resources/mdtest/dataclasses.md:726 unexpected error: 13 [revealed-type] "Revealed type: `(name: str, age: MutableMapping[Unknown, Unknown] | None = None) -> None`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='dataclasses.md - Dataclasses - Internals'
MDTEST_TEST_FILTER='dataclasses.md - Dataclasses - Internals' cargo test -p ty_python_semantic --test mdtest -- mdtest__dataclasses

--------------------------------------------------

test mdtest__dataclasses ... FAILED

failures:

failures:
    mdtest__dataclasses

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 1.03s


--- STDERR:              ty_python_semantic::mdtest mdtest__dataclasses ---

thread 'mdtest__dataclasses' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.866s] ty_python_semantic::mdtest mdtest__directives_assert_type
        FAIL [   0.596s] ty_python_semantic::mdtest mdtest__function_return_type

--- STDOUT:              ty_python_semantic::mdtest mdtest__function_return_type ---

running 1 test
test mdtest__function_return_type ... FAILED

failures:

failures:
    mdtest__function_return_type

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.59s


--- STDERR:              ty_python_semantic::mdtest mdtest__function_return_type ---

thread 'mdtest__function_return_type' panicked at crates/ty_test/src/lib.rs:379:9:
Test `return_type.md - Function return type - Invalid return type in stub file` requested snapshotting diagnostics but it didn't produce any.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        FAIL [   0.740s] ty_python_semantic::mdtest mdtest__expression_boolean

--- STDOUT:              ty_python_semantic::mdtest mdtest__expression_boolean ---

running 1 test

boolean.md - Expressions - Falsy values

  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:91 unmatched assertion: revealed: Literal[False]
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:91 unexpected error: 13 [revealed-type] "Revealed type: `bool`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='boolean.md - Expressions - Falsy values'
MDTEST_TEST_FILTER='boolean.md - Expressions - Falsy values' cargo test -p ty_python_semantic --test mdtest -- mdtest__expression_boolean

boolean.md - Expressions - Not callable `__bool__`

  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:123 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:123 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:123 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:123 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:123    0: to_overloaded_(Id(4002)) -> (R52, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:123              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:123    1: infer_scope_types(Id(800)) -> (R58, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:123              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:123    2: check_types(Id(0)) -> (R57, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:123              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:123

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='boolean.md - Expressions - Not callable `__bool__`'
MDTEST_TEST_FILTER='boolean.md - Expressions - Not callable `__bool__`' cargo test -p ty_python_semantic --test mdtest -- mdtest__expression_boolean

boolean.md - Expressions - Not-boolable union

  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:134 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:134 expected function
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:134 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:134 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:134    0: to_overloaded_(Id(4002)) -> (R64, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:134              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:134    1: symbol_by_id(Id(1c11)) -> (R64, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:134              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:134    2: infer_scope_types(Id(33b1)) -> (R64, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:134              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:134    3: check_types(Id(0)) -> (R64, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:134              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:134

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='boolean.md - Expressions - Not-boolable union'
MDTEST_TEST_FILTER='boolean.md - Expressions - Not-boolable union' cargo test -p ty_python_semantic --test mdtest -- mdtest__expression_boolean

boolean.md - Expressions - Union with some variants implementing `__bool__` incorrectly

  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:146 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:146 expected function
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:146 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:146 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:146    0: to_overloaded_(Id(4002)) -> (R69, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:146              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:146    1: symbol_by_id(Id(1c11)) -> (R64, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:146              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:146    2: infer_scope_types(Id(33b1)) -> (R70, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:146              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:146    3: check_types(Id(0)) -> (R70, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:146              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/expression/boolean.md:146

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='boolean.md - Expressions - Union with some variants implementing `__bool__` incorrectly'
MDTEST_TEST_FILTER='boolean.md - Expressions - Union with some variants implementing `__bool__` incorrectly' cargo test -p ty_python_semantic --test mdtest -- mdtest__expression_boolean

--------------------------------------------------

test mdtest__expression_boolean ... FAILED

failures:

failures:
    mdtest__expression_boolean

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.74s


--- STDERR:              ty_python_semantic::mdtest mdtest__expression_boolean ---

thread 'mdtest__expression_boolean' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.586s] ty_python_semantic::mdtest mdtest__generics_legacy_variables
        PASS [   0.207s] ty_python_semantic::mdtest mdtest__import_stub_packages
        PASS [   0.167s] ty_python_semantic::mdtest mdtest__import_stubs
        PASS [   0.164s] ty_python_semantic::mdtest mdtest__import_tracking
        PASS [   0.169s] ty_python_semantic::mdtest mdtest__invalid_syntax
        PASS [   0.160s] ty_python_semantic::mdtest mdtest__literal_boolean
        PASS [   0.169s] ty_python_semantic::mdtest mdtest__literal_bytes
        PASS [   0.208s] ty_python_semantic::mdtest mdtest__known_constants
        PASS [   0.183s] ty_python_semantic::mdtest mdtest__literal_collections_dictionary
        FAIL [   0.288s] ty_python_semantic::mdtest mdtest__intersection_types

--- STDOUT:              ty_python_semantic::mdtest mdtest__intersection_types ---

running 1 test

intersection_types.md - Intersection types - Structural properties - Single-element intersections

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:122 unmatched assertion: revealed: P
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:122 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:123 unmatched assertion: revealed: ~P
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:123 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - Single-element intersections'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - Single-element intersections' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Structural properties - Flattening of nested intersections

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:139 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:140 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:142 unmatched assertion: revealed: P & Q & R
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:142 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:143 unmatched assertion: revealed: P & Q & R
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:143 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:146 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:146 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:146 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:146 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:147 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:147 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:147 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:147 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:149 unmatched assertion: revealed: ~P & ~Q & ~R
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:149 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:150 unmatched assertion: revealed: ~P & ~Q & ~R
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:150 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:153 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:153 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:154 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:154 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:155 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:155 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:155 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:156 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:156 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:156 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:158 unmatched assertion: revealed: P & R & ~Q
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:158 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:159 unmatched assertion: revealed: P & R & ~Q
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:159 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:160 unmatched assertion: revealed: Q & ~P & ~R
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:160 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:161 unmatched assertion: revealed: Q & ~R & ~P
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:161 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:164 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:164 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:166 unmatched assertion: revealed: P & Q & R & S
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:166 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:169 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:169 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:170 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:170 unexpected error: [non-subscriptable] "Cannot subscript object of type `A` with no `__getitem__` method"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:172 unmatched assertion: revealed: P & Q & R & S
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:172 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:173 unmatched assertion: revealed: P & Q & R & S
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:173 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - Flattening of nested intersections'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - Flattening of nested intersections' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Structural properties - Union of intersections

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:190 unexpected error: [unsupported-operator] "Operator `|` is unsupported between objects of type `<class 'Q'>` and `<class 'R'>`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:191 unexpected error: [unsupported-operator] "Operator `|` is unsupported between objects of type `<class 'P'>` and `<class 'Q'>`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:192 unexpected error: [unsupported-operator] "Operator `|` is unsupported between objects of type `<class 'P'>` and `<class 'Q'>`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:192 unexpected error: [unsupported-operator] "Operator `|` is unsupported between objects of type `<class 'R'>` and `<class 'S'>`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:194 unmatched assertion: revealed: (P & Q) | (P & R) | (P & S)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:194 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:195 unmatched assertion: revealed: (P & S) | (Q & S) | (R & S)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:195 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:196 unmatched assertion: revealed: (P & R) | (Q & R) | (P & S) | (Q & S)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:196 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:199 unexpected error: [unsupported-operator] "Operator `|` is unsupported between objects of type `<class 'Q'>` and `<class 'P'>`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:200 unexpected error: [unsupported-operator] "Operator `|` is unsupported between objects of type `<class 'P'>` and `<class 'Q'>`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:201 unexpected error: [unsupported-operator] "Operator `|` is unsupported between objects of type `<class 'P'>` and `<class 'Q'>`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:201 unexpected error: [unsupported-operator] "Operator `|` is unsupported between objects of type `<class 'Q'>` and `<class 'R'>`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:202 unexpected error: [unsupported-operator] "Operator `|` is unsupported between objects of type `<class 'P'>` and `<class 'Q'>`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:202 unexpected error: [unsupported-operator] "Operator `|` is unsupported between objects of type `<class 'P'>` and `<class 'Q'>`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:203 unexpected error: [unsupported-operator] "Operator `|` is unsupported between objects of type `<class 'P'>` and `<class 'Q'>`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:203 unexpected error: [unsupported-operator] "Operator `|` is unsupported between objects of type `<class 'Q'>` and `<class 'P'>`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:210 unmatched assertion: revealed: P
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:210 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:212 unmatched assertion: revealed: Q
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:212 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:219 unmatched assertion: revealed: Q | (P & R)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:219 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:225 unmatched assertion: revealed: P | Q
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:225 unexpected error: 17 [revealed-type] "Revealed type: `@Todo(unknown type subscript)`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - Union of intersections'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - Union of intersections' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Structural properties - Negation distributes over union

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:234 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:234 expected class
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:234 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:234 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:234    0: explicit_bases_(Id(2c0b)) -> (R42, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:234              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:234    1: infer_scope_types(Id(3b4f)) -> (R43, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:234              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:234    2: check_types(Id(0)) -> (R43, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:234              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:234

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - Negation distributes over union'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - Negation distributes over union' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Structural properties - Negation of intersections

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:254 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:254 expected class
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:254 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:254 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:254    0: explicit_bases_(Id(2c0b)) -> (R48, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:254              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:254    1: infer_scope_types(Id(3b4f)) -> (R49, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:254              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:254    2: check_types(Id(0)) -> (R49, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:254              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:254

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - Negation of intersections'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - Negation of intersections' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Structural properties - `Never` is dual to `object`

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275 expected class
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275    0: pep695_generic_context_(Id(2c00)) -> (R54, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275    1: symbol_by_id(Id(200e)) -> (R55, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275    2: member_lookup_with_policy_(Id(1809)) -> (R55, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275    3: infer_definition_types(Id(c00)) -> (R55, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275    4: infer_scope_types(Id(800)) -> (R54, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275    5: check_types(Id(0)) -> (R54, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:275

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - `Never` is dual to `object`'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - `Never` is dual to `object`' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Structural properties - `object & ~T` is equivalent to `~T`

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:293 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:293 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:293 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:293 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:293    0: pep695_generic_context_(Id(2c09)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:293              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:293    1: infer_scope_types(Id(803)) -> (R60, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:293              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:293    2: check_types(Id(0)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:293              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:293

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - `object & ~T` is equivalent to `~T`'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - `object & ~T` is equivalent to `~T`' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Structural properties - Intersection of a type and its negation

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:307 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:307 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:307 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:307 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:307    0: pep695_generic_context_(Id(2c09)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:307              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:307    1: infer_scope_types(Id(803)) -> (R66, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:307              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:307    2: check_types(Id(0)) -> (R67, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:307              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:307

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - Intersection of a type and its negation'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - Intersection of a type and its negation' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Structural properties - Union of a type and its negation

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:334 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:334 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:334 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:334 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:334    0: pep695_generic_context_(Id(2c09)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:334              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:334    1: infer_scope_types(Id(803)) -> (R72, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:334              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:334    2: check_types(Id(0)) -> (R73, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:334              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:334

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - Union of a type and its negation'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - Union of a type and its negation' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Structural properties - Negation is an involution

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:356 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:356 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:356 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:356 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:356    0: pep695_generic_context_(Id(2c09)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:356              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:356    1: infer_scope_types(Id(803)) -> (R78, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:356              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:356    2: check_types(Id(0)) -> (R79, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:356              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:356

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - Negation is an involution'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Structural properties - Negation is an involution' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Simplification strategies - `Never` in intersections

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:383 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:383 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:383 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:383 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:383    0: pep695_generic_context_(Id(2c09)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:383              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:383    1: infer_scope_types(Id(803)) -> (R84, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:383              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:383    2: check_types(Id(0)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:383              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:383

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification strategies - `Never` in intersections'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification strategies - `Never` in intersections' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Simplification strategies - Simplifications using disjointness - Positive contributions

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:408 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:408 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:408 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:408 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:408    0: pep695_generic_context_(Id(2c09)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:408              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:408    1: infer_scope_types(Id(803)) -> (R90, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:408              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:408    2: check_types(Id(0)) -> (R91, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:408              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:408

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification strategies - Simplifications using disjointness - Positive contributions'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification strategies - Simplifications using disjointness - Positive contributions' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Simplification strategies - Simplifications using disjointness - Positive and negative contributions

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446 expected class
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446    0: pep695_generic_context_(Id(2c00)) -> (R96, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446    1: symbol_by_id(Id(2002)) -> (R97, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446    2: member_lookup_with_policy_(Id(180b)) -> (R97, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446    3: infer_definition_types(Id(c00)) -> (R97, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446    4: infer_scope_types(Id(800)) -> (R96, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446    5: check_types(Id(0)) -> (R96, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:446

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification strategies - Simplifications using disjointness - Positive and negative contributions'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification strategies - Simplifications using disjointness - Positive and negative contributions' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Simplification strategies - Simplifications using subtype relationships - Positive type and positive subtype

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:479 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:479 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:479 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:479 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:479    0: pep695_generic_context_(Id(2c09)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:479              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:479    1: infer_scope_types(Id(803)) -> (R102, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:479              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:479    2: check_types(Id(0)) -> (R103, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:479              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:479

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification strategies - Simplifications using subtype relationships - Positive type and positive subtype'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification strategies - Simplifications using subtype relationships - Positive type and positive subtype' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Simplification strategies - Simplifications using subtype relationships - Negative type and negative subtype

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:538 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:538 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:538 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:538 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:538    0: pep695_generic_context_(Id(2c09)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:538              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:538    1: infer_scope_types(Id(803)) -> (R108, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:538              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:538    2: check_types(Id(0)) -> (R109, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:538              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:538

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification strategies - Simplifications using subtype relationships - Negative type and negative subtype'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification strategies - Simplifications using subtype relationships - Negative type and negative subtype' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Simplification strategies - Simplifications using subtype relationships - Negative type and multiple negative subtypes

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:597 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:597 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:597 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:597 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:597    0: pep695_generic_context_(Id(2c09)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:597              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:597    1: infer_scope_types(Id(803)) -> (R114, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:597              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:597    2: check_types(Id(0)) -> (R115, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:597              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:597

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification strategies - Simplifications using subtype relationships - Negative type and multiple negative subtypes'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification strategies - Simplifications using subtype relationships - Negative type and multiple negative subtypes' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Simplification strategies - Simplifications using subtype relationships - Negative type and positive subtype

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:625 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:625 expected class
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:625 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:625 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:625    0: pep695_generic_context_(Id(2c09)) -> (R121, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:625              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:625    1: infer_scope_types(Id(803)) -> (R120, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:625              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:625    2: check_types(Id(0)) -> (R121, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:625              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:625

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification strategies - Simplifications using subtype relationships - Negative type and positive subtype'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification strategies - Simplifications using subtype relationships - Negative type and positive subtype' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Simplification strategies - Simplifications of `bool`, `AlwaysTruthy` and `AlwaysFalsy`

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:665 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:665 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:665 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:665 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:665    0: pep695_generic_context_(Id(2c09)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:665              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:665    1: infer_scope_types(Id(803)) -> (R126, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:665              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:665    2: check_types(Id(0)) -> (R127, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:665              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:665

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification strategies - Simplifications of `bool`, `AlwaysTruthy` and `AlwaysFalsy`'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification strategies - Simplifications of `bool`, `AlwaysTruthy` and `AlwaysFalsy`' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Simplification of `LiteralString`, `AlwaysTruthy` and `AlwaysFalsy`

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712 expected class
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712    0: pep695_generic_context_(Id(2c00)) -> (R132, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712    1: symbol_by_id(Id(200e)) -> (R133, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712    2: member_lookup_with_policy_(Id(1816)) -> (R133, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712    3: infer_definition_types(Id(c00)) -> (R133, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712    4: infer_scope_types(Id(800)) -> (R132, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712    5: check_types(Id(0)) -> (R132, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:712

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification of `LiteralString`, `AlwaysTruthy` and `AlwaysFalsy`'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Simplification of `LiteralString`, `AlwaysTruthy` and `AlwaysFalsy`' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Addition of a type to an intersection with many non-disjoint types

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745 expected class
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745    0: pep695_generic_context_(Id(2c08)) -> (R138, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745    1: symbol_by_id(Id(200d)) -> (R139, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745    2: member_lookup_with_policy_(Id(1817)) -> (R139, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745    3: infer_definition_types(Id(c00)) -> (R139, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745    4: infer_scope_types(Id(800)) -> (R138, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745    5: check_types(Id(0)) -> (R138, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:745

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Addition of a type to an intersection with many non-disjoint types'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Addition of a type to an intersection with many non-disjoint types' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Non fully-static types - Negation of dynamic types

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:761 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:761 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:761 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:761 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:761    0: pep695_generic_context_(Id(2c09)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:761              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:761    1: infer_scope_types(Id(803)) -> (R144, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:761              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:761    2: check_types(Id(0)) -> (R145, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:761              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:761

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Non fully-static types - Negation of dynamic types'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Non fully-static types - Negation of dynamic types' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Non fully-static types - Collapsing of multiple `Any`/`Unknown` contributions

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:791 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:791 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:791 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:791 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:791    0: pep695_generic_context_(Id(2c09)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:791              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:791    1: infer_scope_types(Id(803)) -> (R150, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:791              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:791    2: check_types(Id(0)) -> (R151, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:791              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:791

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Non fully-static types - Collapsing of multiple `Any`/`Unknown` contributions'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Non fully-static types - Collapsing of multiple `Any`/`Unknown` contributions' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Non fully-static types - No self-cancellation

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825 expected class
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825    0: pep695_generic_context_(Id(2c00)) -> (R156, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825    1: symbol_by_id(Id(2006)) -> (R157, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825    2: member_lookup_with_policy_(Id(1810)) -> (R157, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825    3: infer_definition_types(Id(c03)) -> (R157, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825    4: infer_scope_types(Id(800)) -> (R157, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825    5: check_types(Id(0)) -> (R156, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:825

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Non fully-static types - No self-cancellation'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Non fully-static types - No self-cancellation' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Non fully-static types - Mixed dynamic types

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848 expected class
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848    0: pep695_generic_context_(Id(2c00)) -> (R162, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848    1: symbol_by_id(Id(2013)) -> (R163, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848    2: member_lookup_with_policy_(Id(1801)) -> (R163, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848    3: infer_definition_types(Id(c03)) -> (R163, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848    4: infer_scope_types(Id(800)) -> (R162, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848    5: check_types(Id(0)) -> (R162, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:848

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Non fully-static types - Mixed dynamic types'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Non fully-static types - Mixed dynamic types' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

intersection_types.md - Intersection types - Invalid

  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866 expected class
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866    0: pep695_generic_context_(Id(2c00)) -> (R168, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866    1: symbol_by_id(Id(200e)) -> (R169, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866    2: member_lookup_with_policy_(Id(180c)) -> (R169, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866    3: infer_definition_types(Id(c00)) -> (R169, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866    4: infer_scope_types(Id(800)) -> (R168, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866    5: check_types(Id(0)) -> (R168, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/intersection_types.md:866

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Invalid'
MDTEST_TEST_FILTER='intersection_types.md - Intersection types - Invalid' cargo test -p ty_python_semantic --test mdtest -- mdtest__intersection_types

--------------------------------------------------

test mdtest__intersection_types ... FAILED

failures:

failures:
    mdtest__intersection_types

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.28s


--- STDERR:              ty_python_semantic::mdtest mdtest__intersection_types ---

thread 'mdtest__intersection_types' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.174s] ty_python_semantic::mdtest mdtest__literal_collections_list
        FAIL [   0.405s] ty_python_semantic::mdtest mdtest__import_relative

--- STDOUT:              ty_python_semantic::mdtest mdtest__import_relative ---

running 1 test

relative.md - Relative - Dunder init

  crates/ty_python_semantic/resources/mdtest/import/relative.md:93 unexpected error: 13 [revealed-type] "Revealed type: `int`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='relative.md - Relative - Dunder init'
MDTEST_TEST_FILTER='relative.md - Relative - Dunder init' cargo test -p ty_python_semantic --test mdtest -- mdtest__import_relative

relative.md - Relative - Non-existent + dunder init

  crates/ty_python_semantic/resources/mdtest/import/relative.md:109 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='relative.md - Relative - Non-existent + dunder init'
MDTEST_TEST_FILTER='relative.md - Relative - Non-existent + dunder init' cargo test -p ty_python_semantic --test mdtest -- mdtest__import_relative

relative.md - Relative - Long relative import

  crates/ty_python_semantic/resources/mdtest/import/relative.md:130 unexpected error: 13 [revealed-type] "Revealed type: `int`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='relative.md - Relative - Long relative import'
MDTEST_TEST_FILTER='relative.md - Relative - Long relative import' cargo test -p ty_python_semantic --test mdtest -- mdtest__import_relative

relative.md - Relative - Unbound symbol

  crates/ty_python_semantic/resources/mdtest/import/relative.md:151 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='relative.md - Relative - Unbound symbol'
MDTEST_TEST_FILTER='relative.md - Relative - Unbound symbol' cargo test -p ty_python_semantic --test mdtest -- mdtest__import_relative

relative.md - Relative - Bare to module

  crates/ty_python_semantic/resources/mdtest/import/relative.md:172 unexpected error: 13 [revealed-type] "Revealed type: `int`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='relative.md - Relative - Bare to module'
MDTEST_TEST_FILTER='relative.md - Relative - Bare to module' cargo test -p ty_python_semantic --test mdtest -- mdtest__import_relative

relative.md - Relative - Non-existent + bare to module

  crates/ty_python_semantic/resources/mdtest/import/relative.md:190 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='relative.md - Relative - Non-existent + bare to module'
MDTEST_TEST_FILTER='relative.md - Relative - Non-existent + bare to module' cargo test -p ty_python_semantic --test mdtest -- mdtest__import_relative

relative.md - Relative - Import submodule from self

  crates/ty_python_semantic/resources/mdtest/import/relative.md:219 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='relative.md - Relative - Import submodule from self'
MDTEST_TEST_FILTER='relative.md - Relative - Import submodule from self' cargo test -p ty_python_semantic --test mdtest -- mdtest__import_relative

--------------------------------------------------

test mdtest__import_relative ... FAILED

failures:

failures:
    mdtest__import_relative

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.39s


--- STDERR:              ty_python_semantic::mdtest mdtest__import_relative ---

thread 'mdtest__import_relative' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.157s] ty_python_semantic::mdtest mdtest__literal_collections_tuple
        PASS [   0.171s] ty_python_semantic::mdtest mdtest__literal_complex
        PASS [   0.234s] ty_python_semantic::mdtest mdtest__literal_collections_set
        PASS [   0.184s] ty_python_semantic::mdtest mdtest__literal_f_string
        PASS [   1.381s] ty_python_semantic::mdtest mdtest__diagnostics_semantic_syntax_errors
        PASS [   0.185s] ty_python_semantic::mdtest mdtest__literal_float
        PASS [   0.024s] ty_python_semantic::mdtest mdtest__mdtest_custom_typeshed
        PASS [   0.180s] ty_python_semantic::mdtest mdtest__literal_string
        PASS [   0.252s] ty_python_semantic::mdtest mdtest__literal_integer
        PASS [   0.214s] ty_python_semantic::mdtest mdtest__loops_async_for
        PASS [   0.236s] ty_python_semantic::mdtest mdtest__loops_iterators
        PASS [   0.333s] ty_python_semantic::mdtest mdtest__literal_ellipsis
        PASS [   1.944s] ty_python_semantic::mdtest mdtest__call_callable_instance
        PASS [   0.171s] ty_python_semantic::mdtest mdtest__narrow_bool_call
        FAIL [   0.247s] ty_python_semantic::mdtest mdtest__narrow_assert

--- STDOUT:              ty_python_semantic::mdtest mdtest__narrow_assert ---

running 1 test

assert.md - Narrowing with assert statements - Assertions with messages

  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:59 unmatched assertion: revealed: int | None
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:59 unexpected error: 17 [revealed-type] "Revealed type: `int | Unknown`"
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:60 unmatched assertion: revealed: int
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:60 unexpected error: 35 [revealed-type] "Revealed type: `int | Unknown`"
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:61 unmatched assertion: revealed: None
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:61 unexpected error: 17 [revealed-type] "Revealed type: `(int & Unknown) | Unknown`"
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:63 unmatched assertion: revealed: int | None
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:63 unexpected error: 17 [revealed-type] "Revealed type: `int | Unknown`"
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:64 unmatched assertion: revealed: None
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:64 unexpected error: 44 [revealed-type] "Revealed type: `Unknown & ~int`"
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:65 unmatched assertion: revealed: int
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:65 unexpected error: 17 [revealed-type] "Revealed type: `int | (Unknown & int)`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='assert.md - Narrowing with assert statements - Assertions with messages'
MDTEST_TEST_FILTER='assert.md - Narrowing with assert statements - Assertions with messages' cargo test -p ty_python_semantic --test mdtest -- mdtest__narrow_assert

assert.md - Narrowing with assert statements - Assertions with definitions inside the message

  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:72 unmatched assertion: revealed: int
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:72 unexpected error: 51 [revealed-type] "Revealed type: `int | Unknown`"
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:79 unmatched assertion: revealed: int | None
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:79 unexpected error: 17 [revealed-type] "Revealed type: `int | Unknown`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='assert.md - Narrowing with assert statements - Assertions with definitions inside the message'
MDTEST_TEST_FILTER='assert.md - Narrowing with assert statements - Assertions with definitions inside the message' cargo test -p ty_python_semantic --test mdtest -- mdtest__narrow_assert

assert.md - Narrowing with assert statements - Assertions with `test` predicates that are statically known to always be `True`

  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:85 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:85 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:85 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:85 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:85    0: to_overloaded_(Id(4001)) -> (R37, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:85              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:85    1: infer_scope_types(Id(800)) -> (R43, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:85              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:85    2: check_types(Id(0)) -> (R42, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:85              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:85

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='assert.md - Narrowing with assert statements - Assertions with `test` predicates that are statically known to always be `True`'
MDTEST_TEST_FILTER='assert.md - Narrowing with assert statements - Assertions with `test` predicates that are statically known to always be `True`' cargo test -p ty_python_semantic --test mdtest -- mdtest__narrow_assert

assert.md - Narrowing with assert statements - Assertions with messages that reference definitions from the `test`

  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:108 unmatched assertion: revealed: (int & ~AlwaysTruthy) | None
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:108 unexpected error: 34 [revealed-type] "Revealed type: `(int & ~AlwaysTruthy) | (Unknown & ~AlwaysTruthy)`"
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:109 unmatched assertion: revealed: int & ~AlwaysFalsy
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:109 unexpected error: 17 [revealed-type] "Revealed type: `(int & ~AlwaysFalsy) | (Unknown & ~AlwaysFalsy)`"
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:112 unmatched assertion: revealed: None
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:112 unexpected error: 51 [revealed-type] "Revealed type: `Unknown & ~int`"
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:113 unmatched assertion: revealed: int
  crates/ty_python_semantic/resources/mdtest/narrow/assert.md:113 unexpected error: 17 [revealed-type] "Revealed type: `int | (Unknown & int)`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='assert.md - Narrowing with assert statements - Assertions with messages that reference definitions from the `test`'
MDTEST_TEST_FILTER='assert.md - Narrowing with assert statements - Assertions with messages that reference definitions from the `test`' cargo test -p ty_python_semantic --test mdtest -- mdtest__narrow_assert

--------------------------------------------------

test mdtest__narrow_assert ... FAILED

failures:

failures:
    mdtest__narrow_assert

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.24s


--- STDERR:              ty_python_semantic::mdtest mdtest__narrow_assert ---

thread 'mdtest__narrow_assert' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.263s] ty_python_semantic::mdtest mdtest__narrow_boolean
        PASS [   0.291s] ty_python_semantic::mdtest mdtest__narrow_conditionals_elif_else
        PASS [   0.201s] ty_python_semantic::mdtest mdtest__narrow_conditionals_in
        FAIL [   0.190s] ty_python_semantic::mdtest mdtest__narrow_conditionals_is_not

--- STDOUT:              ty_python_semantic::mdtest mdtest__narrow_conditionals_is_not ---

running 1 test

is_not.md - Narrowing for `is not` conditionals - Assignment expressions

  crates/ty_python_semantic/resources/mdtest/narrow/conditionals/is_not.md:92 unmatched assertion: revealed: int | str
  crates/ty_python_semantic/resources/mdtest/narrow/conditionals/is_not.md:92 unexpected error: 17 [revealed-type] "Revealed type: `(int & ~None) | (str & ~None)`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='is_not.md - Narrowing for `is not` conditionals - Assignment expressions'
MDTEST_TEST_FILTER='is_not.md - Narrowing for `is not` conditionals - Assignment expressions' cargo test -p ty_python_semantic --test mdtest -- mdtest__narrow_conditionals_is_not

--------------------------------------------------

test mdtest__narrow_conditionals_is_not ... FAILED

failures:

failures:
    mdtest__narrow_conditionals_is_not

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.19s


--- STDERR:              ty_python_semantic::mdtest mdtest__narrow_conditionals_is_not ---

thread 'mdtest__narrow_conditionals_is_not' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.654s] ty_python_semantic::mdtest mdtest__mdtest_config
        PASS [   0.587s] ty_python_semantic::mdtest mdtest__named_tuple
        PASS [   0.186s] ty_python_semantic::mdtest mdtest__narrow_conditionals_not
        PASS [   0.505s] ty_python_semantic::mdtest mdtest__narrow_conditionals_is
        PASS [   0.188s] ty_python_semantic::mdtest mdtest__narrow_post_if_statement
        PASS [   0.433s] ty_python_semantic::mdtest mdtest__narrow_conditionals_nested
        PASS [   0.322s] ty_python_semantic::mdtest mdtest__narrow_match
        PASS [   0.336s] ty_python_semantic::mdtest mdtest__narrow_truthiness
        PASS [   0.221s] ty_python_semantic::mdtest mdtest__pep695_type_aliases
        FAIL [   1.951s] ty_python_semantic::mdtest mdtest__import_errors

--- STDOUT:              ty_python_semantic::mdtest mdtest__import_errors ---

running 1 test

errors.md - Unresolved Imports - Import cycle

  crates/ty_python_semantic/resources/mdtest/import/errors.md:72 unexpected error: 13 [revealed-type] "Revealed type: `tuple[<class 'A'>, <class 'object'>]`"
  crates/ty_python_semantic/resources/mdtest/import/errors.md:77 unexpected error: 13 [revealed-type] "Revealed type: `tuple[<class 'C'>, <class 'B'>, <class 'A'>, <class 'object'>]`"
  crates/ty_python_semantic/resources/mdtest/import/errors.md:87 unexpected error: 13 [revealed-type] "Revealed type: `tuple[<class 'B'>, <class 'A'>, <class 'object'>]`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='errors.md - Unresolved Imports - Import cycle'
MDTEST_TEST_FILTER='errors.md - Unresolved Imports - Import cycle' cargo test -p ty_python_semantic --test mdtest -- mdtest__import_errors

--------------------------------------------------

test mdtest__import_errors ... FAILED

failures:

failures:
    mdtest__import_errors

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 1.94s


--- STDERR:              ty_python_semantic::mdtest mdtest__import_errors ---

thread 'mdtest__import_errors' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        FAIL [   0.544s] ty_python_semantic::mdtest mdtest__properties

--- STDOUT:              ty_python_semantic::mdtest mdtest__properties ---

running 1 test

properties.md - Properties - Behind the scenes

  crates/ty_python_semantic/resources/mdtest/properties.md:211 unexpected error: [missing-argument] "No argument provided for required parameter `fset` of function `setter`"
  crates/ty_python_semantic/resources/mdtest/properties.md:220 unexpected error: [missing-argument] "No argument provided for required parameter `self` of function `__init__`"
  crates/ty_python_semantic/resources/mdtest/properties.md:222 unmatched assertion: revealed: int
  crates/ty_python_semantic/resources/mdtest/properties.md:222 unexpected error: 13 [revealed-type] "Revealed type: `property`"
  crates/ty_python_semantic/resources/mdtest/properties.md:241 unmatched assertion: revealed: <wrapper-descriptor `__get__` of `property` objects>
  crates/ty_python_semantic/resources/mdtest/properties.md:241 unexpected error: 13 [revealed-type] "Revealed type: `Any`"
  crates/ty_python_semantic/resources/mdtest/properties.md:242 unmatched assertion: revealed: <wrapper-descriptor `__set__` of `property` objects>
  crates/ty_python_semantic/resources/mdtest/properties.md:242 unexpected error: 13 [revealed-type] "Revealed type: `Any`"
  crates/ty_python_semantic/resources/mdtest/properties.md:250 unmatched assertion: revealed: int
  crates/ty_python_semantic/resources/mdtest/properties.md:250 unexpected error: 13 [revealed-type] "Revealed type: `Any`"
  crates/ty_python_semantic/resources/mdtest/properties.md:251 unmatched assertion: revealed: int
  crates/ty_python_semantic/resources/mdtest/properties.md:251 unexpected error: 13 [revealed-type] "Revealed type: `Any`"
  crates/ty_python_semantic/resources/mdtest/properties.md:257 unmatched assertion: revealed: int
  crates/ty_python_semantic/resources/mdtest/properties.md:257 unexpected error: 13 [revealed-type] "Revealed type: `Any`"
  crates/ty_python_semantic/resources/mdtest/properties.md:266 unmatched assertion: revealed: property
  crates/ty_python_semantic/resources/mdtest/properties.md:266 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/properties.md:267 unmatched assertion: revealed: property
  crates/ty_python_semantic/resources/mdtest/properties.md:267 unexpected error: 13 [revealed-type] "Revealed type: `Any`"
  crates/ty_python_semantic/resources/mdtest/properties.md:268 unmatched assertion: revealed: property
  crates/ty_python_semantic/resources/mdtest/properties.md:268 unexpected error: 13 [revealed-type] "Revealed type: `Any`"
  crates/ty_python_semantic/resources/mdtest/properties.md:278 unmatched assertion: error: [call-non-callable] "Call of wrapper descriptor `property.__set__` failed: calling the setter failed"
  crates/ty_python_semantic/resources/mdtest/properties.md:286 unmatched assertion: error: [call-non-callable]
  crates/ty_python_semantic/resources/mdtest/properties.md:290 unmatched assertion: error: [call-non-callable]
  crates/ty_python_semantic/resources/mdtest/properties.md:297 unmatched assertion: revealed: def attr(self) -> int
  crates/ty_python_semantic/resources/mdtest/properties.md:297 unexpected error: 13 [revealed-type] "Revealed type: `((Any, /) -> Any) | None`"
  crates/ty_python_semantic/resources/mdtest/properties.md:298 unmatched assertion: revealed: int
  crates/ty_python_semantic/resources/mdtest/properties.md:298 unexpected error: 13 [call-non-callable] "Object of type `None` is not callable"
  crates/ty_python_semantic/resources/mdtest/properties.md:298 unexpected error: 13 [revealed-type] "Revealed type: `Any`"
  crates/ty_python_semantic/resources/mdtest/properties.md:300 unmatched assertion: revealed: def attr(self, value: str) -> None
  crates/ty_python_semantic/resources/mdtest/properties.md:300 unexpected error: 13 [revealed-type] "Revealed type: `((Any, Any, /) -> None) | None`"
  crates/ty_python_semantic/resources/mdtest/properties.md:301 unmatched assertion: revealed: None
  crates/ty_python_semantic/resources/mdtest/properties.md:301 unexpected error: 13 [call-non-callable] "Object of type `None` is not callable"
  crates/ty_python_semantic/resources/mdtest/properties.md:301 unexpected error: 13 [revealed-type] "Revealed type: `None | Unknown`"
  crates/ty_python_semantic/resources/mdtest/properties.md:304 unmatched assertion: error: [invalid-argument-type]
  crates/ty_python_semantic/resources/mdtest/properties.md:304 unexpected error: 1 [call-non-callable] "Object of type `None` is not callable"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='properties.md - Properties - Behind the scenes'
MDTEST_TEST_FILTER='properties.md - Properties - Behind the scenes' cargo test -p ty_python_semantic --test mdtest -- mdtest__properties

--------------------------------------------------

test mdtest__properties ... FAILED

failures:

failures:
    mdtest__properties

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.54s


--- STDERR:              ty_python_semantic::mdtest mdtest__properties ---

thread 'mdtest__properties' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        FAIL [   2.437s] ty_python_semantic::mdtest mdtest__import_star

--- STDOUT:              ty_python_semantic::mdtest mdtest__import_star ---

running 1 test

star.md - Wildcard (`*`) imports - Esoteric definitions and redefinintions - Definitions in function-like scopes are not global definitions

  crates/ty_python_semantic/resources/mdtest/import/star.md:451 unexpected error: [no-matching-overload] "No overload of bound method `__new__` matches arguments"
  crates/ty_python_semantic/resources/mdtest/import/star.md:451 unexpected error: [no-matching-overload] "No overload of bound method `__new__` matches arguments"
  crates/ty_python_semantic/resources/mdtest/import/star.md:463 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:465 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:467 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:469 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:471 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:473 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:475 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:477 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:479 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:481 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:483 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:485 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:492 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:494 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:496 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:498 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:500 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:502 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='star.md - Wildcard (`*`) imports - Esoteric definitions and redefinintions - Definitions in function-like scopes are not global definitions'
MDTEST_TEST_FILTER='star.md - Wildcard (`*`) imports - Esoteric definitions and redefinintions - Definitions in function-like scopes are not global definitions' cargo test -p ty_python_semantic --test mdtest -- mdtest__import_star

star.md - Wildcard (`*`) imports - Esoteric definitions and redefinintions - An annotation without a value is a definition in a stub but not a `.py` file

  crates/ty_python_semantic/resources/mdtest/import/star.md:525 unexpected error: 13 [revealed-type] "Revealed type: `bool`"
  crates/ty_python_semantic/resources/mdtest/import/star.md:527 unexpected error: 13 [revealed-type] "Revealed type: `Unknown`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='star.md - Wildcard (`*`) imports - Esoteric definitions and redefinintions - An annotation without a value is a definition in a stub but not a `.py` file'
MDTEST_TEST_FILTER='star.md - Wildcard (`*`) imports - Esoteric definitions and redefinintions - An annotation without a value is a definition in a stub but not a `.py` file' cargo test -p ty_python_semantic --test mdtest -- mdtest__import_star

--------------------------------------------------

test mdtest__import_star ... FAILED

failures:

failures:
    mdtest__import_star

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 2.43s


--- STDERR:              ty_python_semantic::mdtest mdtest__import_star ---

thread 'mdtest__import_star' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.127s] ty_python_semantic::mdtest mdtest__regression_14334_diagnostics_in_wrong_file
        PASS [   0.182s] ty_python_semantic::mdtest mdtest__scopes_builtin
        FAIL [   1.267s] ty_python_semantic::mdtest mdtest__overloads

--- STDOUT:              ty_python_semantic::mdtest mdtest__overloads ---

running 1 test
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ Snapshot Summary ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Snapshot file: crates/ty_python_semantic/resources/mdtest/snapshots/overloads.md_-_Overloads_-_Invalid_-_Inconsistent_decorators_-_`@override`.snap
Snapshot: overloads.md_-_Overloads_-_Invalid_-_Inconsistent_decorators_-_`@override`
Source: crates/ty_test/src/lib.rs:394
────────────────────────────────────────────────────────────────────────────────
Expression: snapshot
────────────────────────────────────────────────────────────────────────────────
-old snapshot
+new results
────────────┬───────────────────────────────────────────────────────────────────
   76    76 │
   77    77 │ # Diagnostics
   78    78 │
   79    79 │ ```
   80       │-error: lint:invalid-overload: `@override` decorator should be applied only to the overload implementation
   81       │-  --> src/mdtest_snippet.py:27:9
         80 │+error: lint:invalid-overload: Overloaded function `method` requires at least two overloads
         81 │+  --> src/mdtest_snippet.py:35:9
   82    82 │    |
   83       │-25 |     def method(self, x: str) -> str: ...
   84       │-26 |     # error: [invalid-overload]
   85       │-27 |     def method(self, x: int | str) -> int | str:
   86       │-   |         ------
   87       │-   |         |
   88       │-   |         Implementation defined here
   89       │-28 |         return x
   90       │-   |
   91       │-info: `lint:invalid-overload` is enabled by default
   92       │-
   93       │-```
   94       │-
   95       │-```
   96       │-error: lint:invalid-overload: `@override` decorator should be applied only to the overload implementation
   97       │-  --> src/mdtest_snippet.py:37:9
   98       │-   |
         83 │+33 |     def method(self, x: int) -> int: ...
         84 │+34 |     @overload
   99    85 │ 35 |     def method(self, x: str) -> str: ...
         86 │+   |         ------ Only one overload defined here
  100    87 │ 36 |     # error: [invalid-overload]
  101    88 │ 37 |     def method(self, x: int | str) -> int | str:
  102       │-   |         ------
  103       │-   |         |
  104       │-   |         Implementation defined here
  105       │-38 |         return x
  106       │-   |
  107       │-info: `lint:invalid-overload` is enabled by default
  108       │-
  109       │-```
  110       │-
  111       │-```
  112       │-error: lint:invalid-overload: `@override` decorator should be applied only to the first overload
  113       │-  --> src/mdtest_snippet.pyi:18:9
  114       │-   |
  115       │-16 | class Sub2(Base):
  116       │-17 |     @overload
  117       │-18 |     def method(self, x: int) -> int: ...
  118       │-   |         ------ First overload defined here
  119       │-19 |     @overload
  120       │-20 |     @override
  121       │-21 |     # error: [invalid-overload]
  122       │-22 |     def method(self, x: str) -> str: ...
  123    89 │    |         ^^^^^^
         90 │+38 |         return x
         91 │+   |
         92 │+info: `lint:invalid-overload` is enabled by default
         93 │+
         94 │+```
         95 │+
         96 │+```
         97 │+error: lint:invalid-overload: Overloaded function `method` requires at least two overloads
         98 │+  --> src/mdtest_snippet.pyi:14:9
         99 │+   |
        100 │+12 |     def method(self, x: int) -> int: ...
        101 │+13 |     @overload
        102 │+14 |     def method(self, x: str) -> str: ...
        103 │+   |         ------
        104 │+   |         |
        105 │+   |         Only one overload defined here
        106 │+15 |
        107 │+16 | class Sub2(Base):
  124   108 │    |
  125   109 │ info: `lint:invalid-overload` is enabled by default
  126   110 │
  127   111 │ ```
────────────┴───────────────────────────────────────────────────────────────────
To update snapshots run `cargo insta review`
Stopped on the first failure. Run `cargo insta test` to run all snapshots.
test mdtest__overloads ... FAILED

failures:

failures:
    mdtest__overloads

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 1.26s


--- STDERR:              ty_python_semantic::mdtest mdtest__overloads ---
stored new snapshot /home/ibraheem/dev/astral/ruff/crates/ty_python_semantic/resources/mdtest/snapshots/overloads.md_-_Overloads_-_Invalid_-_Inconsistent_decorators_-_`@override`.snap.new

thread 'mdtest__overloads' panicked at /home/ibraheem/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/insta-1.42.2/src/runtime.rs:679:13:
snapshot assertion for 'overloads.md_-_Overloads_-_Invalid_-_Inconsistent_decorators_-_`@override`' failed in line 394
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        FAIL [   0.280s] ty_python_semantic::mdtest mdtest__scopes_eager

--- STDOUT:              ty_python_semantic::mdtest mdtest__scopes_eager ---

running 1 test

eager.md - Eager scopes - Generator expressions

  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:90 unexpected error: 34 [not-iterable] "Object of type `range` is not iterable"
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:102 unexpected error: 34 [not-iterable] "Object of type `range` is not iterable"
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:119 unexpected error: 21 [not-iterable] "Object of type `list` is not iterable"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='eager.md - Eager scopes - Generator expressions'
MDTEST_TEST_FILTER='eager.md - Eager scopes - Generator expressions' cargo test -p ty_python_semantic --test mdtest -- mdtest__scopes_eager

eager.md - Eager scopes - Top-level eager scopes - Class definitions

  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136    0: to_overloaded_(Id(1401)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136    1: class_member_with_policy_(Id(7002)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136    2: try_call_dunder_get_(Id(6c01)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136    3: member_lookup_with_policy_(Id(4003)) -> (R37, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136              cycle heads: explicit_bases_(Id(2402)) -> 0
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136    4: infer_expression_types(Id(cf9)) -> (R37, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136              cycle heads: explicit_bases_(Id(2402)) -> 0
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136    5: infer_deferred_types(Id(2d63)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136    6: explicit_bases_(Id(2402)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136    7: infer_expression_types(Id(c7c)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136    8: infer_scope_types(Id(800)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136    9: check_types(Id(0)) -> (R36, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:136

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='eager.md - Eager scopes - Top-level eager scopes - Class definitions'
MDTEST_TEST_FILTER='eager.md - Eager scopes - Top-level eager scopes - Class definitions' cargo test -p ty_python_semantic --test mdtest -- mdtest__scopes_eager

eager.md - Eager scopes - Top-level eager scopes - List comprehensions

  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:151 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:151 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:151 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:151 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:151    0: to_overloaded_(Id(1401)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:151              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:151    1: infer_expression_types(Id(d53)) -> (R42, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:151              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:151    2: infer_scope_types(Id(800)) -> (R42, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:151              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:151    3: check_types(Id(0)) -> (R42, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:151              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:151

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='eager.md - Eager scopes - Top-level eager scopes - List comprehensions'
MDTEST_TEST_FILTER='eager.md - Eager scopes - Top-level eager scopes - List comprehensions' cargo test -p ty_python_semantic --test mdtest -- mdtest__scopes_eager

eager.md - Eager scopes - Top-level eager scopes - Set comprehensions

  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:166 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:166 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:166 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:166 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:166    0: to_overloaded_(Id(1401)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:166              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:166    1: infer_expression_types(Id(d53)) -> (R48, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:166              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:166    2: infer_scope_types(Id(800)) -> (R48, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:166              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:166    3: check_types(Id(0)) -> (R48, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:166              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:166

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='eager.md - Eager scopes - Top-level eager scopes - Set comprehensions'
MDTEST_TEST_FILTER='eager.md - Eager scopes - Top-level eager scopes - Set comprehensions' cargo test -p ty_python_semantic --test mdtest -- mdtest__scopes_eager

eager.md - Eager scopes - Top-level eager scopes - Dict comprehensions

  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:181 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:181 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:181 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:181 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:181    0: to_overloaded_(Id(1401)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:181              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:181    1: infer_expression_types(Id(d53)) -> (R54, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:181              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:181    2: infer_scope_types(Id(800)) -> (R54, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:181              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:181    3: check_types(Id(0)) -> (R54, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:181              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:181

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='eager.md - Eager scopes - Top-level eager scopes - Dict comprehensions'
MDTEST_TEST_FILTER='eager.md - Eager scopes - Top-level eager scopes - Dict comprehensions' cargo test -p ty_python_semantic --test mdtest -- mdtest__scopes_eager

eager.md - Eager scopes - Top-level eager scopes - Generator expressions

  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196    0: to_overloaded_(Id(1401)) -> (R31, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196    1: infer_expression_types(Id(c2d)) -> (R31, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196              cycle heads: symbol_by_id(Id(2007)) -> 0
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196    2: infer_definition_types(Id(132c)) -> (R1, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196    3: infer_deferred_types(Id(2d63)) -> (R13, Durability::MEDIUM, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196              cycle heads: explicit_bases_(Id(240c)) -> 0
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196    4: explicit_bases_(Id(2402)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196    5: infer_expression_types(Id(cf9)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196    6: infer_scope_types(Id(800)) -> (R60, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196    7: check_types(Id(0)) -> (R60, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:196
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211 expected function
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211    0: to_overloaded_(Id(1401)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211    1: infer_expression_types(Id(d4b)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211    2: infer_expression_types(Id(d4c)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211    3: infer_definition_types(Id(7479)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211    4: infer_scope_types(Id(37c3)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211    5: check_types(Id(49)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:211
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226 expected function
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226    0: to_overloaded_(Id(1401)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226    1: class_member_with_policy_(Id(7005)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226    2: try_call_dunder_get_(Id(6c02)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226    3: member_lookup_with_policy_(Id(400b)) -> (R61, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              cycle heads: explicit_bases_(Id(2402)) -> 0
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226    4: infer_expression_types(Id(cf9)) -> (R61, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              cycle heads: explicit_bases_(Id(2402)) -> 0
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226    5: symbol_by_id(Id(2007)) -> (R1, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226    6: infer_expression_types(Id(cca)) -> (R1, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226    7: infer_definition_types(Id(3acd)) -> (R1, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226    8: infer_deferred_types(Id(3b0f)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226    9: explicit_bases_(Id(240e)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226   10: infer_deferred_types(Id(3b17)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226   11: explicit_bases_(Id(240d)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226   12: infer_deferred_types(Id(3b85)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226   13: explicit_bases_(Id(240c)) -> (R13, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226   14: infer_deferred_types(Id(2d63)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226   15: explicit_bases_(Id(2402)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226   16: infer_expression_types(Id(c7c)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226   17: infer_expression_types(Id(d4e)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226   18: infer_expression_types(Id(d4f)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226   19: infer_definition_types(Id(747e)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226   20: infer_scope_types(Id(37c5)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226   21: check_types(Id(4a)) -> (R61, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:226

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='eager.md - Eager scopes - Top-level eager scopes - Generator expressions'
MDTEST_TEST_FILTER='eager.md - Eager scopes - Top-level eager scopes - Generator expressions' cargo test -p ty_python_semantic --test mdtest -- mdtest__scopes_eager

eager.md - Eager scopes - Lazy scopes are "sticky" - Eager scope within eager scope

  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:250 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:250 expected function
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:250 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:250 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:250    0: to_overloaded_(Id(1401)) -> (R66, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:250              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:250    1: infer_expression_types(Id(d55)) -> (R73, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:250              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:250    2: infer_scope_types(Id(3741)) -> (R72, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:250              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:250    3: check_types(Id(0)) -> (R73, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:250              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:250

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='eager.md - Eager scopes - Lazy scopes are "sticky" - Eager scope within eager scope'
MDTEST_TEST_FILTER='eager.md - Eager scopes - Lazy scopes are "sticky" - Eager scope within eager scope' cargo test -p ty_python_semantic --test mdtest -- mdtest__scopes_eager

eager.md - Eager scopes - Lazy scopes are "sticky" - Class definition bindings are not visible in nested scopes

  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:267 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:267 expected function
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:267 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:267 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:267    0: to_overloaded_(Id(1401)) -> (R66, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:267              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:267    1: infer_expression_types(Id(d52)) -> (R79, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:267              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:267    2: infer_scope_types(Id(3741)) -> (R79, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:267              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:267    3: check_types(Id(0)) -> (R79, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:267              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:267

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='eager.md - Eager scopes - Lazy scopes are "sticky" - Class definition bindings are not visible in nested scopes'
MDTEST_TEST_FILTER='eager.md - Eager scopes - Lazy scopes are "sticky" - Class definition bindings are not visible in nested scopes' cargo test -p ty_python_semantic --test mdtest -- mdtest__scopes_eager

eager.md - Eager scopes - Lazy scopes are "sticky" - Eager scope within a lazy scope

  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:298 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:298 expected function
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:298 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:298 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:298    0: to_overloaded_(Id(1401)) -> (R66, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:298              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:298    1: infer_expression_types(Id(d55)) -> (R84, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:298              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:298    2: infer_scope_types(Id(3741)) -> (R84, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:298              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:298    3: check_types(Id(0)) -> (R85, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:298              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:298

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='eager.md - Eager scopes - Lazy scopes are "sticky" - Eager scope within a lazy scope'
MDTEST_TEST_FILTER='eager.md - Eager scopes - Lazy scopes are "sticky" - Eager scope within a lazy scope' cargo test -p ty_python_semantic --test mdtest -- mdtest__scopes_eager

eager.md - Eager scopes - Lazy scopes are "sticky" - Lazy scope within an eager scope

  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314 expected function
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314    0: to_overloaded_(Id(1401)) -> (R66, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314    1: class_member_with_policy_(Id(7008)) -> (R91, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314    2: try_call_dunder_get_(Id(6c03)) -> (R91, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314    3: member_lookup_with_policy_(Id(400e)) -> (R91, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              cycle heads: explicit_bases_(Id(2402)) -> 0
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314    4: infer_expression_types(Id(cf9)) -> (R91, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              cycle heads: explicit_bases_(Id(2402)) -> 0
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314    5: symbol_by_id(Id(2007)) -> (R1, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314    6: infer_expression_types(Id(cca)) -> (R1, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314    7: infer_definition_types(Id(3acd)) -> (R1, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314    8: infer_deferred_types(Id(3b0f)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314    9: explicit_bases_(Id(240e)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314   10: infer_deferred_types(Id(3b17)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314   11: explicit_bases_(Id(240d)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314   12: infer_deferred_types(Id(3b85)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314   13: explicit_bases_(Id(240c)) -> (R13, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314   14: infer_deferred_types(Id(2d63)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314   15: explicit_bases_(Id(2402)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314   16: infer_expression_types(Id(c7c)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314   17: infer_scope_types(Id(37c7)) -> (R91, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314   18: check_types(Id(0)) -> (R91, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:314

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='eager.md - Eager scopes - Lazy scopes are "sticky" - Lazy scope within an eager scope'
MDTEST_TEST_FILTER='eager.md - Eager scopes - Lazy scopes are "sticky" - Lazy scope within an eager scope' cargo test -p ty_python_semantic --test mdtest -- mdtest__scopes_eager

eager.md - Eager scopes - Lazy scopes are "sticky" - Lazy scope within a lazy scope

  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331 expected function
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331    0: to_overloaded_(Id(1401)) -> (R66, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331    1: class_member_with_policy_(Id(700b)) -> (R97, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331    2: try_call_dunder_get_(Id(6c00)) -> (R97, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331    3: member_lookup_with_policy_(Id(4013)) -> (R97, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              cycle heads: explicit_bases_(Id(2402)) -> 0
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331    4: infer_expression_types(Id(cf9)) -> (R97, Durability::LOW, iteration = 0)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              cycle heads: explicit_bases_(Id(2402)) -> 0
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331    5: symbol_by_id(Id(2007)) -> (R1, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331    6: infer_expression_types(Id(cca)) -> (R1, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331    7: infer_definition_types(Id(3acd)) -> (R1, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331    8: infer_deferred_types(Id(3b0f)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331    9: explicit_bases_(Id(240e)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331   10: infer_deferred_types(Id(3b17)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331   11: explicit_bases_(Id(240d)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331   12: infer_deferred_types(Id(3b85)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331   13: explicit_bases_(Id(240c)) -> (R13, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331   14: infer_deferred_types(Id(2d63)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331   15: explicit_bases_(Id(2402)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331   16: infer_expression_types(Id(c7c)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331   17: infer_scope_types(Id(37c7)) -> (R97, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331   18: check_types(Id(0)) -> (R97, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:331

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='eager.md - Eager scopes - Lazy scopes are "sticky" - Lazy scope within a lazy scope'
MDTEST_TEST_FILTER='eager.md - Eager scopes - Lazy scopes are "sticky" - Lazy scope within a lazy scope' cargo test -p ty_python_semantic --test mdtest -- mdtest__scopes_eager

eager.md - Eager scopes - Lazy scopes are "sticky" - Eager scope within a lazy scope within another eager scope

  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:349 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:349 expected function
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:349 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:349 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:349    0: to_overloaded_(Id(1401)) -> (R66, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:349              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:349    1: infer_expression_types(Id(d4a)) -> (R103, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:349              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:349    2: infer_scope_types(Id(37c7)) -> (R102, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:349              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:349    3: check_types(Id(0)) -> (R103, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:349              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:349

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='eager.md - Eager scopes - Lazy scopes are "sticky" - Eager scope within a lazy scope within another eager scope'
MDTEST_TEST_FILTER='eager.md - Eager scopes - Lazy scopes are "sticky" - Eager scope within a lazy scope within another eager scope' cargo test -p ty_python_semantic --test mdtest -- mdtest__scopes_eager

eager.md - Eager scopes - Annotations - Eager annotations in a Python file

  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368 expected function
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368    0: to_overloaded_(Id(1401)) -> (R66, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368    1: symbol_by_id(Id(200f)) -> (R109, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368    2: infer_deferred_types(Id(2d40)) -> (R109, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368    3: signature_(Id(1405)) -> (R37, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368    4: infer_definition_types(Id(13f8)) -> (R109, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368    5: symbol_by_id(Id(200a)) -> (R109, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368    6: infer_expression_types(Id(cca)) -> (R109, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368    7: explicit_bases_(Id(240c)) -> (R13, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368    8: infer_deferred_types(Id(2d63)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368    9: explicit_bases_(Id(2402)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368   10: infer_expression_types(Id(c7c)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368   11: infer_scope_types(Id(800)) -> (R109, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368   12: check_types(Id(0)) -> (R108, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:368

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='eager.md - Eager scopes - Annotations - Eager annotations in a Python file'
MDTEST_TEST_FILTER='eager.md - Eager scopes - Annotations - Eager annotations in a Python file' cargo test -p ty_python_semantic --test mdtest -- mdtest__scopes_eager

eager.md - Eager scopes - Annotations - Deferred annotations in a Python file

  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383 panicked at /home/ibraheem/dev/astral/salsa/src/function/fetch.rs:129:25
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383 dependency graph cycle when querying to_overloaded_(Id(1401)), set cycle_fn/cycle_initial to fixpoint iterate.
Query stack:
[
    check_types(Id(0)),
    infer_scope_types(Id(800)),
    symbol_by_id(Id(2001)),
    infer_definition_types(Id(335e)),
    member_lookup_with_policy_(Id(401f)),
    symbol_by_id(Id(2011)),
    infer_expression_type(Id(ce3)),
    infer_expression_types(Id(ce3)),
    explicit_bases_(Id(2402)),
    infer_deferred_types(Id(2d63)),
    explicit_bases_(Id(240c)),
    infer_expression_types(Id(cca)),
    symbol_by_id(Id(2012)),
    infer_definition_types(Id(13f8)),
    signature_(Id(1405)),
    infer_deferred_types(Id(2d40)),
    symbol_by_id(Id(2004)),
    infer_definition_types(Id(2cef)),
    to_overloaded_(Id(1401)),
    infer_definition_types(Id(13f4)),
    symbol_by_id(Id(2003)),
    infer_definition_types(Id(31a4)),
    member_lookup_with_policy_(Id(4004)),
    member_lookup_with_policy_(Id(4022)),
    class_member_with_policy_(Id(700f)),
    try_mro_(Id(6806)),
]
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383    0: infer_definition_types(Id(31a4)) -> (R115, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383    1: symbol_by_id(Id(2003)) -> (R115, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383    2: infer_definition_types(Id(13f4)) -> (R115, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383    3: to_overloaded_(Id(1401)) -> (R115, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383    4: infer_definition_types(Id(2cef)) -> (R115, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383    5: symbol_by_id(Id(2004)) -> (R115, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383    6: infer_deferred_types(Id(2d40)) -> (R115, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383    7: signature_(Id(1405)) -> (R37, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383    8: infer_definition_types(Id(13f8)) -> (R115, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383    9: symbol_by_id(Id(2012)) -> (R115, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383   10: infer_expression_types(Id(cca)) -> (R115, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383   11: explicit_bases_(Id(240c)) -> (R13, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383   12: infer_deferred_types(Id(2d63)) -> (R13, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383   13: explicit_bases_(Id(2402)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383   14: infer_expression_types(Id(ce3)) -> (R115, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383   15: infer_expression_type(Id(ce3)) -> (R1, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types/infer.rs:277
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383   16: symbol_by_id(Id(2011)) -> (R115, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383   17: member_lookup_with_policy_(Id(401f)) -> (R115, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383   18: infer_definition_types(Id(335e)) -> (R115, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383   19: symbol_by_id(Id(2001)) -> (R115, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383   20: infer_scope_types(Id(800)) -> (R115, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383   21: check_types(Id(0)) -> (R114, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:383

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='eager.md - Eager scopes - Annotations - Deferred annotations in a Python file'
MDTEST_TEST_FILTER='eager.md - Eager scopes - Annotations - Deferred annotations in a Python file' cargo test -p ty_python_semantic --test mdtest -- mdtest__scopes_eager

eager.md - Eager scopes - Annotations - Deferred annotations in a stub file

  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400 panicked at /home/ibraheem/dev/astral/salsa/src/function/fetch.rs:129:25
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400 dependency graph cycle when querying to_overloaded_(Id(1401)), set cycle_fn/cycle_initial to fixpoint iterate.
Query stack:
[
    check_types(Id(54)),
    infer_scope_types(Id(37c7)),
    symbol_by_id(Id(2001)),
    infer_definition_types(Id(335e)),
    member_lookup_with_policy_(Id(400f)),
    symbol_by_id(Id(2015)),
    infer_expression_type(Id(ce3)),
    infer_expression_types(Id(ce3)),
    explicit_bases_(Id(2402)),
    infer_deferred_types(Id(2d63)),
    infer_definition_types(Id(132c)),
    infer_expression_types(Id(c2d)),
    symbol_by_id(Id(2016)),
    infer_definition_types(Id(13f8)),
    signature_(Id(1405)),
    infer_deferred_types(Id(2d40)),
    symbol_by_id(Id(2017)),
    infer_definition_types(Id(2cef)),
    to_overloaded_(Id(1401)),
    infer_definition_types(Id(13f4)),
    symbol_by_id(Id(2018)),
    infer_definition_types(Id(31a4)),
    class_member_with_policy_(Id(7011)),
    try_mro_(Id(6808)),
    class_member_with_policy_(Id(700f)),
    try_mro_(Id(6806)),
]
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400    0: infer_definition_types(Id(31a4)) -> (R115, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400    1: symbol_by_id(Id(2018)) -> (R118, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400    2: infer_definition_types(Id(13f4)) -> (R118, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400    3: to_overloaded_(Id(1401)) -> (R115, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400    4: infer_definition_types(Id(2cef)) -> (R115, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400    5: symbol_by_id(Id(2017)) -> (R118, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400    6: infer_deferred_types(Id(2d40)) -> (R118, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400    7: signature_(Id(1405)) -> (R37, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400    8: infer_definition_types(Id(13f8)) -> (R118, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400    9: symbol_by_id(Id(2016)) -> (R118, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400   10: infer_expression_types(Id(c2d)) -> (R118, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400   11: infer_definition_types(Id(132c)) -> (R1, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400   12: infer_deferred_types(Id(2d63)) -> (R118, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400   13: explicit_bases_(Id(2402)) -> (R37, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400   14: infer_expression_types(Id(ce3)) -> (R118, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400   15: infer_expression_type(Id(ce3)) -> (R1, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types/infer.rs:277
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400   16: symbol_by_id(Id(2015)) -> (R118, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400   17: member_lookup_with_policy_(Id(400f)) -> (R118, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400   18: infer_definition_types(Id(335e)) -> (R118, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400   19: symbol_by_id(Id(2001)) -> (R115, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400   20: infer_scope_types(Id(37c7)) -> (R118, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400   21: check_types(Id(54)) -> (R118, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/scopes/eager.md:400

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='eager.md - Eager scopes - Annotations - Deferred annotations in a stub file'
MDTEST_TEST_FILTER='eager.md - Eager scopes - Annotations - Deferred annotations in a stub file' cargo test -p ty_python_semantic --test mdtest -- mdtest__scopes_eager

--------------------------------------------------

test mdtest__scopes_eager ... FAILED

failures:

failures:
    mdtest__scopes_eager

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 0.28s


--- STDERR:              ty_python_semantic::mdtest mdtest__scopes_eager ---

thread 'mdtest__scopes_eager' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.229s] ty_python_semantic::mdtest mdtest__scopes_global
        PASS [   0.226s] ty_python_semantic::mdtest mdtest__scopes_moduletype_attrs
        PASS [   0.178s] ty_python_semantic::mdtest mdtest__scopes_unbound
        PASS [   0.142s] ty_python_semantic::mdtest mdtest__shadowing_class
        PASS [   0.194s] ty_python_semantic::mdtest mdtest__shadowing_function
        PASS [   0.160s] ty_python_semantic::mdtest mdtest__shadowing_variable_declaration
        FAIL [   2.227s] ty_python_semantic::mdtest mdtest__narrow_while

--- STDOUT:              ty_python_semantic::mdtest mdtest__narrow_while ---

running 1 test

while.md - Narrowing in `while` loops - Nested `while` loops

  crates/ty_python_semantic/resources/mdtest/narrow/while.md:52 unexpected error: 17 [revealed-type] "Revealed type: `Literal[2, 3]`"
  crates/ty_python_semantic/resources/mdtest/narrow/while.md:57 unexpected error: 21 [revealed-type] "Revealed type: `Literal[3]`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='while.md - Narrowing in `while` loops - Nested `while` loops'
MDTEST_TEST_FILTER='while.md - Narrowing in `while` loops - Nested `while` loops' cargo test -p ty_python_semantic --test mdtest -- mdtest__narrow_while

--------------------------------------------------

test mdtest__narrow_while ... FAILED

failures:

failures:
    mdtest__narrow_while

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 2.22s


--- STDERR:              ty_python_semantic::mdtest mdtest__narrow_while ---

thread 'mdtest__narrow_while' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   5.233s] ty_python_semantic types::class::tests::known_class_doesnt_fallback_to_unknown_unexpectedly_on_low_python_version
        FAIL [   1.824s] ty_python_semantic::mdtest mdtest__protocols

--- STDOUT:              ty_python_semantic::mdtest mdtest__protocols ---

running 1 test

protocols.md - Protocols - Protocol members in statically known branches

  crates/ty_python_semantic/resources/mdtest/protocols.md:446 unmatched assertion: revealed: tuple[Literal["d"], Literal["e"], Literal["f"]]
  crates/ty_python_semantic/resources/mdtest/protocols.md:446 unexpected error: 13 [revealed-type] "Revealed type: `Unknown | tuple[Literal["d"], Literal["e"], Literal["f"]]`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='protocols.md - Protocols - Protocol members in statically known branches'
MDTEST_TEST_FILTER='protocols.md - Protocols - Protocol members in statically known branches' cargo test -p ty_python_semantic --test mdtest -- mdtest__protocols

protocols.md - Protocols - Subtyping of protocols with attribute members

  crates/ty_python_semantic/resources/mdtest/protocols.md:623 unexpected error: 13 [revealed-type] "Revealed type: `int`"
  crates/ty_python_semantic/resources/mdtest/protocols.md:627 unexpected error: 13 [revealed-type] "Revealed type: `int`"
  crates/ty_python_semantic/resources/mdtest/protocols.md:631 unexpected error: 17 [revealed-type] "Revealed type: `int`"
  crates/ty_python_semantic/resources/mdtest/protocols.md:679 unmatched assertion: revealed: tuple[Literal["Nested"], Literal["NestedProtocol"], Literal["a"], Literal["b"], Literal["c"], Literal["d"], Literal["e"], Literal["f"], Literal["g"], Literal["h"], Literal["i"], Literal["j"], Literal["k"], Literal["l"]]
  crates/ty_python_semantic/resources/mdtest/protocols.md:679 unexpected error: 13 [revealed-type] "Revealed type: `Unknown | tuple[Literal["Nested"], Literal["NestedProtocol"], Literal["a"], Literal["b"], Literal["c"], Literal["d"], Literal["e"], Literal["f"], Literal["g"], Literal["h"], Literal["i"], Literal["j"], Literal["k"], Literal["l"]]`"
  crates/ty_python_semantic/resources/mdtest/protocols.md:679 unexpected error: 13 [revealed-type] "Revealed type: `Unknown | tuple[Literal["Nested"], Literal["NestedProtocol"], Literal["a"], Literal["b"], Literal["c"], Literal["d"], Literal["e"], Literal["f"], Literal["g"], Literal["h"], Literal["i"], Literal["j"], Literal["k"], Literal["l"]]`"
  crates/ty_python_semantic/resources/mdtest/protocols.md:708 unmatched assertion: revealed: tuple[Literal["non_init_method"], Literal["x"], Literal["y"]]
  crates/ty_python_semantic/resources/mdtest/protocols.md:708 unexpected error: 13 [revealed-type] "Revealed type: `Unknown | tuple[Literal["non_init_method"], Literal["x"], Literal["y"]]`"
  crates/ty_python_semantic/resources/mdtest/protocols.md:708 unexpected error: 13 [revealed-type] "Revealed type: `Unknown | tuple[Literal["non_init_method"], Literal["x"], Literal["y"]]`"
  crates/ty_python_semantic/resources/mdtest/protocols.md:722 unmatched assertion: revealed: tuple[Literal["x"]]
  crates/ty_python_semantic/resources/mdtest/protocols.md:722 unexpected error: 13 [revealed-type] "Revealed type: `Unknown | tuple[Literal["x"]]`"
  crates/ty_python_semantic/resources/mdtest/protocols.md:722 unexpected error: 13 [revealed-type] "Revealed type: `Unknown | tuple[Literal["x"]]`"
  crates/ty_python_semantic/resources/mdtest/protocols.md:723 unmatched assertion: revealed: tuple[Literal["x"]]
  crates/ty_python_semantic/resources/mdtest/protocols.md:723 unexpected error: 13 [revealed-type] "Revealed type: `Unknown | tuple[Literal["x"]]`"
  crates/ty_python_semantic/resources/mdtest/protocols.md:723 unexpected error: 13 [revealed-type] "Revealed type: `Unknown | tuple[Literal["x"]]`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='protocols.md - Protocols - Subtyping of protocols with attribute members'
MDTEST_TEST_FILTER='protocols.md - Protocols - Subtyping of protocols with attribute members' cargo test -p ty_python_semantic --test mdtest -- mdtest__protocols

protocols.md - Protocols - Equivalence of protocols

  crates/ty_python_semantic/resources/mdtest/protocols.md:816 unexpected error: [static-assert-error] "Static assertion error: argument evaluates to `False`"
  crates/ty_python_semantic/resources/mdtest/protocols.md:831 unexpected error: [static-assert-error] "Static assertion error: argument evaluates to `False`"
  crates/ty_python_semantic/resources/mdtest/protocols.md:832 unexpected error: [static-assert-error] "Static assertion error: argument evaluates to `False`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='protocols.md - Protocols - Equivalence of protocols'
MDTEST_TEST_FILTER='protocols.md - Protocols - Equivalence of protocols' cargo test -p ty_python_semantic --test mdtest -- mdtest__protocols

protocols.md - Protocols - Fully static protocols; gradual protocols

  crates/ty_python_semantic/resources/mdtest/protocols.md:1360 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:543:18
  crates/ty_python_semantic/resources/mdtest/protocols.md:1360 expected class
  crates/ty_python_semantic/resources/mdtest/protocols.md:1360 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/protocols.md:1360 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/protocols.md:1360    0: pep695_generic_context_(Id(2803)) -> (R86, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1360              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/protocols.md:1360    1: infer_definition_types(Id(47f8)) -> (R81, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1360              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/protocols.md:1360    2: infer_definition_types(Id(760e)) -> (R86, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1360              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/protocols.md:1360    3: infer_scope_types(Id(800)) -> (R87, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1360              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/protocols.md:1360    4: check_types(Id(0)) -> (R86, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1360              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/protocols.md:1360

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='protocols.md - Protocols - Fully static protocols; gradual protocols'
MDTEST_TEST_FILTER='protocols.md - Protocols - Fully static protocols; gradual protocols' cargo test -p ty_python_semantic --test mdtest -- mdtest__protocols

protocols.md - Protocols - Callable protocols

  crates/ty_python_semantic/resources/mdtest/protocols.md:1461 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/protocols.md:1461 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/protocols.md:1461 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/protocols.md:1461 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/protocols.md:1461    0: pep695_generic_context_(Id(2803)) -> (R81, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1461              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/protocols.md:1461    1: infer_definition_types(Id(47f8)) -> (R81, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1461              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/protocols.md:1461    2: infer_definition_types(Id(7605)) -> (R92, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1461              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/protocols.md:1461    3: infer_scope_types(Id(800)) -> (R93, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1461              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/protocols.md:1461    4: check_types(Id(0)) -> (R92, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1461              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/protocols.md:1461

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='protocols.md - Protocols - Callable protocols'
MDTEST_TEST_FILTER='protocols.md - Protocols - Callable protocols' cargo test -p ty_python_semantic --test mdtest -- mdtest__protocols

protocols.md - Protocols - Protocols are never singleton types, and are never single-valued types

  crates/ty_python_semantic/resources/mdtest/protocols.md:1532 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/protocols.md:1532 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/protocols.md:1532 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/protocols.md:1532 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/protocols.md:1532    0: pep695_generic_context_(Id(2803)) -> (R81, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1532              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/protocols.md:1532    1: infer_scope_types(Id(800)) -> (R99, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1532              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/protocols.md:1532    2: check_types(Id(0)) -> (R98, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1532              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/protocols.md:1532

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='protocols.md - Protocols - Protocols are never singleton types, and are never single-valued types'
MDTEST_TEST_FILTER='protocols.md - Protocols - Protocols are never singleton types, and are never single-valued types' cargo test -p ty_python_semantic --test mdtest -- mdtest__protocols

protocols.md - Protocols - Integration test: `typing.SupportsIndex` and `typing.Sized`

  crates/ty_python_semantic/resources/mdtest/protocols.md:1548 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/protocols.md:1548 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/protocols.md:1548 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/protocols.md:1548 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/protocols.md:1548    0: pep695_generic_context_(Id(2803)) -> (R81, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1548              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/protocols.md:1548    1: infer_deferred_types(Id(39db)) -> (R69, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1548              at crates/ty_python_semantic/src/types/infer.rs:185
  crates/ty_python_semantic/resources/mdtest/protocols.md:1548    2: explicit_bases_(Id(281f)) -> (R105, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1548              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/protocols.md:1548    3: infer_definition_types(Id(75f7)) -> (R105, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1548              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/protocols.md:1548    4: infer_scope_types(Id(800)) -> (R105, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1548              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/protocols.md:1548    5: check_types(Id(0)) -> (R104, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1548              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/protocols.md:1548

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='protocols.md - Protocols - Integration test: `typing.SupportsIndex` and `typing.Sized`'
MDTEST_TEST_FILTER='protocols.md - Protocols - Integration test: `typing.SupportsIndex` and `typing.Sized`' cargo test -p ty_python_semantic --test mdtest -- mdtest__protocols

protocols.md - Protocols - Recursive protocols - Properties

  crates/ty_python_semantic/resources/mdtest/protocols.md:1566 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/protocols.md:1566 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/protocols.md:1566 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/protocols.md:1566 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/protocols.md:1566    0: pep695_generic_context_(Id(2803)) -> (R81, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1566              at crates/ty_python_semantic/src/types/class.rs:502
  crates/ty_python_semantic/resources/mdtest/protocols.md:1566    1: infer_definition_types(Id(47f8)) -> (R81, Durability::MEDIUM)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1566              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/protocols.md:1566    2: infer_definition_types(Id(75f9)) -> (R111, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1566              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/protocols.md:1566    3: infer_scope_types(Id(800)) -> (R111, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1566              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/protocols.md:1566    4: check_types(Id(0)) -> (R110, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/protocols.md:1566              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/protocols.md:1566

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='protocols.md - Protocols - Recursive protocols - Properties'
MDTEST_TEST_FILTER='protocols.md - Protocols - Recursive protocols - Properties' cargo test -p ty_python_semantic --test mdtest -- mdtest__protocols

--------------------------------------------------

test mdtest__protocols ... FAILED

failures:

failures:
    mdtest__protocols

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 1.82s


--- STDERR:              ty_python_semantic::mdtest mdtest__protocols ---

thread 'mdtest__protocols' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.113s] ty_python_semantic::mdtest mdtest__stubs_locals
        PASS [   0.187s] ty_python_semantic::mdtest mdtest__stubs_ellipsis
        PASS [   0.317s] ty_python_semantic::mdtest mdtest__stubs_class
        PASS [   0.172s] ty_python_semantic::mdtest mdtest__subscript_bytes
        PASS [   0.198s] ty_python_semantic::mdtest mdtest__subscript_instance
        PASS [   0.182s] ty_python_semantic::mdtest mdtest__subscript_lists
        PASS [   0.160s] ty_python_semantic::mdtest mdtest__subscript_stepsize_zero
        PASS [   0.202s] ty_python_semantic::mdtest mdtest__subscript_string
        PASS [   0.180s] ty_python_semantic::mdtest mdtest__suppressions_no_type_check
        FAIL [   1.446s] ty_python_semantic::mdtest mdtest__statically_known_branches

--- STDOUT:              ty_python_semantic::mdtest mdtest__statically_known_branches ---

running 1 test

statically_known_branches.md - Statically-known branches - `match` statements - Single-valued types, always true

  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1015 unmatched assertion: revealed: Literal[2]
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1015 unexpected error: 13 [revealed-type] "Revealed type: `Literal[1]`"

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='statically_known_branches.md - Statically-known branches - `match` statements - Single-valued types, always true'
MDTEST_TEST_FILTER='statically_known_branches.md - Statically-known branches - `match` statements - Single-valued types, always true' cargo test -p ty_python_semantic --test mdtest -- mdtest__statically_known_branches

statically_known_branches.md - Statically-known branches - `match` statements - Matching on `sys.platform`

  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1146 unexpected error: [unresolved-reference] "Name `darwin` used when not defined"
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1152 unmatched assertion: error: [unresolved-reference]

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='statically_known_branches.md - Statically-known branches - `match` statements - Matching on `sys.platform`'
MDTEST_TEST_FILTER='statically_known_branches.md - Statically-known branches - `match` statements - Matching on `sys.platform`' cargo test -p ty_python_semantic --test mdtest -- mdtest__statically_known_branches

statically_known_branches.md - Statically-known branches - Conditional class definitions

  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1275 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1275 expected function
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1275 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1275 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1275    0: to_overloaded_(Id(4404)) -> (R437, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1275              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1275    1: infer_scope_types(Id(800)) -> (R438, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1275              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1275    2: check_types(Id(0)) -> (R437, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1275              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1275

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='statically_known_branches.md - Statically-known branches - Conditional class definitions'
MDTEST_TEST_FILTER='statically_known_branches.md - Statically-known branches - Conditional class definitions' cargo test -p ty_python_semantic --test mdtest -- mdtest__statically_known_branches

statically_known_branches.md - Statically-known branches - Conditional class attributes

  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1289 panicked at crates/ty_python_semantic/src/semantic_index/symbol.rs:548:28
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1289 expected function
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1289 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1289 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1289    0: to_overloaded_(Id(4404)) -> (R443, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1289              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1289    1: infer_scope_types(Id(800)) -> (R444, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1289              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1289    2: check_types(Id(0)) -> (R443, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1289              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1289

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='statically_known_branches.md - Statically-known branches - Conditional class attributes'
MDTEST_TEST_FILTER='statically_known_branches.md - Statically-known branches - Conditional class attributes' cargo test -p ty_python_semantic --test mdtest -- mdtest__statically_known_branches

statically_known_branches.md - Statically-known branches - (Un)boundness - Ambiguous, possibly unbound

  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1350 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1350 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1350 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1350 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1350    0: to_overloaded_(Id(4404)) -> (R432, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1350              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1350    1: infer_definition_types(Id(43db)) -> (R474, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1350              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1350    2: infer_scope_types(Id(800)) -> (R473, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1350              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1350    3: check_types(Id(0)) -> (R473, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1350              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1350

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='statically_known_branches.md - Statically-known branches - (Un)boundness - Ambiguous, possibly unbound'
MDTEST_TEST_FILTER='statically_known_branches.md - Statically-known branches - (Un)boundness - Ambiguous, possibly unbound' cargo test -p ty_python_semantic --test mdtest -- mdtest__statically_known_branches

statically_known_branches.md - Statically-known branches - (Un)boundness - Nested conditionals

  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1363 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1363 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1363 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1363 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1363    0: to_overloaded_(Id(4404)) -> (R432, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1363              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1363    1: infer_definition_types(Id(43db)) -> (R479, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1363              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1363    2: infer_scope_types(Id(800)) -> (R479, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1363              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1363    3: check_types(Id(0)) -> (R479, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1363              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1363

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='statically_known_branches.md - Statically-known branches - (Un)boundness - Nested conditionals'
MDTEST_TEST_FILTER='statically_known_branches.md - Statically-known branches - (Un)boundness - Nested conditionals' cargo test -p ty_python_semantic --test mdtest -- mdtest__statically_known_branches

statically_known_branches.md - Statically-known branches - (Un)boundness - Imports of conditionally defined symbols - Ambiguous, possibly unbound

  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1469 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1469 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1469 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1469 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1469    0: to_overloaded_(Id(4404)) -> (R432, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1469              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1469    1: infer_definition_types(Id(43d1)) -> (R525, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1469              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1469    2: infer_scope_types(Id(37a0)) -> (R521, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1469              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1469    3: check_types(Id(1)) -> (R521, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1469              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1469
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477    0: to_overloaded_(Id(4404)) -> (R432, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477    1: infer_definition_types(Id(43d1)) -> (R525, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477    2: infer_expression_types(Id(1144)) -> (R521, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477              at crates/ty_python_semantic/src/types/infer.rs:221
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477    3: symbol_by_id(Id(2406)) -> (R525, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477              at crates/ty_python_semantic/src/symbol.rs:576
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477    4: member_lookup_with_policy_(Id(3c0d)) -> (R525, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477              at crates/ty_python_semantic/src/types.rs:536
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477    5: infer_definition_types(Id(c02)) -> (R525, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477              at crates/ty_python_semantic/src/types/infer.rs:147
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477    6: infer_scope_types(Id(800)) -> (R524, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477    7: check_types(Id(0)) -> (R524, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1477

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='statically_known_branches.md - Statically-known branches - (Un)boundness - Imports of conditionally defined symbols - Ambiguous, possibly unbound'
MDTEST_TEST_FILTER='statically_known_branches.md - Statically-known branches - (Un)boundness - Imports of conditionally defined symbols - Ambiguous, possibly unbound' cargo test -p ty_python_semantic --test mdtest -- mdtest__statically_known_branches

statically_known_branches.md - Statically-known branches - (Un)boundness - Imports of conditionally defined symbols - Always false, undeclared

  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1491 panicked at /home/ibraheem/dev/astral/salsa/src/tracked_struct.rs:844:21
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1491 access to field whilst the value is being initialized
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1491 run with `RUST_BACKTRACE=1` environment variable to display a backtrace
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1491 query stacktrace:
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1491    0: to_overloaded_(Id(4404)) -> (R432, Durability::HIGH)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1491              at crates/ty_python_semantic/src/types.rs:6686
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1491    1: infer_scope_types(Id(800)) -> (R537, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1491              at crates/ty_python_semantic/src/types/infer.rs:120
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1491    2: check_types(Id(0)) -> (R536, Durability::LOW)
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1491              at crates/ty_python_semantic/src/types.rs:84
  crates/ty_python_semantic/resources/mdtest/statically_known_branches.md:1491

To rerun this specific test, set the environment variable: MDTEST_TEST_FILTER='statically_known_branches.md - Statically-known branches - (Un)boundness - Imports of conditionally defined symbols - Always false, undeclared'
MDTEST_TEST_FILTER='statically_known_branches.md - Statically-known branches - (Un)boundness - Imports of conditionally defined symbols - Always false, undeclared' cargo test -p ty_python_semantic --test mdtest -- mdtest__statically_known_branches

--------------------------------------------------

test mdtest__statically_known_branches ... FAILED

failures:

failures:
    mdtest__statically_known_branches

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 221 filtered out; finished in 1.44s


--- STDERR:              ty_python_semantic::mdtest mdtest__statically_known_branches ---

thread 'mdtest__statically_known_branches' panicked at crates/ty_test/src/lib.rs:116:5:
Some tests failed.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   0.188s] ty_python_semantic::mdtest mdtest__suppressions_type_ignore
        PASS [   0.185s] ty_python_semantic::mdtest mdtest__sys_platform
        PASS [   0.228s] ty_python_semantic::mdtest mdtest__sys_version_info
        PASS [   0.298s] ty_python_semantic::mdtest mdtest__terminal_statements
   Canceling due to interrupt: 32 tests still running
      SIGINT [   6.769s] ty_python_semantic::mdtest mdtest__conditional_if_statement
      SIGINT [   6.801s] ty_python_semantic::mdtest mdtest__conditional_if_expression
      SIGINT [   6.881s] ty_python_semantic::mdtest mdtest__comparison_instances_membership_test
      SIGINT [   6.859s] ty_python_semantic::mdtest mdtest__comparison_instances_rich_comparison
      SIGINT [   0.213s] ty_python_semantic::mdtest mdtest__type_api
      SIGINT [   5.883s] ty_python_semantic::mdtest mdtest__import_dunder_all
      SIGINT [   1.247s] ty_python_semantic::mdtest mdtest__suppressions_ty_ignore
      SIGINT [   1.534s] ty_python_semantic::mdtest mdtest__subscript_tuple
      SIGINT [   1.913s] ty_python_semantic::mdtest mdtest__subscript_class
      SIGINT [   2.571s] ty_python_semantic::mdtest mdtest__slots
      SIGINT [   6.816s] ty_python_semantic::mdtest mdtest__comprehensions_basic
      SIGINT [   7.000s] ty_python_semantic::mdtest mdtest__call_constructor
      SIGINT [   2.972s] ty_python_semantic::mdtest mdtest__scopes_nonlocal
      SIGINT [   7.145s] ty_python_semantic::mdtest mdtest__binary_instances
      SIGINT [   7.190s] ty_python_semantic::mdtest mdtest__annotations_string
      SIGINT [   5.061s] ty_python_semantic::mdtest mdtest__narrow_conditionals_eq
      SIGINT [   5.273s] ty_python_semantic::mdtest mdtest__mro
      SIGINT [   4.740s] ty_python_semantic::mdtest mdtest__narrow_isinstance
      SIGINT [   4.452s] ty_python_semantic::mdtest mdtest__narrow_type
      SIGINT [   4.688s] ty_python_semantic::mdtest mdtest__narrow_issubclass
      SIGINT [   5.280s] ty_python_semantic::mdtest mdtest__metaclass
      SIGINT [   5.167s] ty_python_semantic::mdtest mdtest__narrow_conditionals_boolean
      SIGINT [   6.707s] ty_python_semantic::mdtest mdtest__decorators
      SIGINT [   6.212s] ty_python_semantic::mdtest mdtest__generics_pep695_classes
      SIGINT [   6.348s] ty_python_semantic::mdtest mdtest__expression_if
      SIGINT [   6.967s] ty_python_semantic::mdtest mdtest__class_super
      SIGINT [   6.313s] ty_python_semantic::mdtest mdtest__expression_len
      SIGINT [   5.899s] ty_python_semantic::mdtest mdtest__import_conventions
      SIGINT [   6.780s] ty_python_semantic::mdtest mdtest__conditional_match
      SIGINT [   6.189s] ty_python_semantic::mdtest mdtest__generics_pep695_variables
      SIGINT [   5.406s] ty_python_semantic::mdtest mdtest__loops_for
      SIGINT [   5.355s] ty_python_semantic::mdtest mdtest__loops_while_loop
------------
     Summary [   7.391s] 394/420 tests run: 323 passed, 71 failed, 36 skipped
error: test run failed
