pub(crate) use convert_named_tuple_functional_to_class::{
    convert_named_tuple_functional_to_class, ConvertNamedTupleFunctionalToClass,
};
pub(crate) use convert_typed_dict_functional_to_class::{
    convert_typed_dict_functional_to_class, ConvertTypedDictFunctionalToClass,
};
pub(crate) use datetime_utc_alias::{datetime_utc_alias, DatetimeTimezoneUTC};
pub(crate) use deprecated_unittest_alias::{deprecated_unittest_alias, DeprecatedUnittestAlias};
pub(crate) use extraneous_parentheses::{extraneous_parentheses, ExtraneousParentheses};
pub(crate) use f_strings::{f_strings, FString};
pub(crate) use format_literals::{format_literals, FormatLiterals};
pub(crate) use functools_cache::{functools_cache, FunctoolsCache};
pub(crate) use import_replacements::{import_replacements, ImportReplacements};
pub(crate) use lru_cache_without_parameters::{
    lru_cache_without_parameters, LRUCacheWithoutParameters,
};
pub(crate) use native_literals::{native_literals, NativeLiterals};
pub(crate) use open_alias::{open_alias, OpenAlias};
pub(crate) use os_error_alias::{os_error_alias, OSErrorAlias};
pub(crate) use outdated_version_block::{outdated_version_block, OutdatedVersionBlock};
pub(crate) use printf_string_formatting::{printf_string_formatting, PrintfStringFormatting};
pub(crate) use quoted_annotation::{quoted_annotation, QuotedAnnotation};
pub(crate) use redundant_open_modes::{redundant_open_modes, RedundantOpenModes};
pub(crate) use replace_stdout_stderr::{replace_stdout_stderr, ReplaceStdoutStderr};
pub(crate) use replace_universal_newlines::{replace_universal_newlines, ReplaceUniversalNewlines};
pub(crate) use rewrite_c_element_tree::{replace_c_element_tree, RewriteCElementTree};
pub(crate) use rewrite_mock_import::{
    rewrite_mock_attribute, rewrite_mock_import, RewriteMockImport,
};
pub(crate) use rewrite_unicode_literal::{rewrite_unicode_literal, RewriteUnicodeLiteral};
pub(crate) use rewrite_yield_from::{rewrite_yield_from, RewriteYieldFrom};
pub(crate) use super_call_with_parameters::{super_call_with_parameters, SuperCallWithParameters};
pub(crate) use type_of_primitive::{type_of_primitive, TypeOfPrimitive};
pub(crate) use typing_text_str_alias::{typing_text_str_alias, TypingTextStrAlias};
pub(crate) use unnecessary_builtin_import::{unnecessary_builtin_import, UnnecessaryBuiltinImport};
pub(crate) use unnecessary_coding_comment::{
    unnecessary_coding_comment, PEP3120UnnecessaryCodingComment,
};
pub(crate) use unnecessary_encode_utf8::{unnecessary_encode_utf8, UnnecessaryEncodeUTF8};
pub(crate) use unnecessary_future_import::{unnecessary_future_import, UnnecessaryFutureImport};
pub(crate) use unpack_list_comprehension::{unpack_list_comprehension, RewriteListComprehension};
pub(crate) use use_pep585_annotation::{use_pep585_annotation, UsePEP585Annotation};
pub(crate) use use_pep604_annotation::{use_pep604_annotation, UsePEP604Annotation};
pub(crate) use useless_metaclass_type::{useless_metaclass_type, UselessMetaclassType};
pub(crate) use useless_object_inheritance::{useless_object_inheritance, UselessObjectInheritance};

mod convert_named_tuple_functional_to_class;
mod convert_typed_dict_functional_to_class;
mod datetime_utc_alias;
mod deprecated_unittest_alias;
mod extraneous_parentheses;
mod f_strings;
mod format_literals;
mod functools_cache;
mod import_replacements;
mod lru_cache_without_parameters;
mod native_literals;
mod open_alias;
mod os_error_alias;
mod outdated_version_block;
mod printf_string_formatting;
mod quoted_annotation;
mod redundant_open_modes;
mod replace_stdout_stderr;
mod replace_universal_newlines;
mod rewrite_c_element_tree;
mod rewrite_mock_import;
mod rewrite_unicode_literal;
mod rewrite_yield_from;
mod super_call_with_parameters;
mod type_of_primitive;
mod typing_text_str_alias;
mod unnecessary_builtin_import;
mod unnecessary_coding_comment;
mod unnecessary_encode_utf8;
mod unnecessary_future_import;
mod unpack_list_comprehension;
mod use_pep585_annotation;
mod use_pep604_annotation;
mod useless_metaclass_type;
mod useless_object_inheritance;
