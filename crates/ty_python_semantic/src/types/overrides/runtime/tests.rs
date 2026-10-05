use super::*;

/// Key inspection grows with the UTF-8 byte length and rejects arithmetic overflow.
#[test]
fn effective_kind_key_work_counts_name_bytes_with_checked_arithmetic() {
    for name in [
        Name::new_inline("field").unwrap(),
        Name::new_heap("field"),
        Name::new_heap("long_field_".repeat(128)),
        Name::new_heap("résultat"),
    ] {
        assert_eq!(
            EffectiveVariableKindProfile::input_work_for_name_length(name.len()),
            Some(4 + name.as_str().len()),
        );
    }
    assert_eq!(
        EffectiveVariableKindProfile::input_work_for_name_length(usize::MAX - 4),
        Some(usize::MAX),
    );
    assert_eq!(
        EffectiveVariableKindProfile::input_work_for_name_length(usize::MAX - 3),
        None,
    );
}

/// The native quotation accounts for the fixed tuple, shallow Name clone, and shared Name cleanup.
#[test]
fn effective_kind_native_quote_covers_tuple_and_name_cleanup() {
    assert_eq!(
        EffectiveVariableKindProfile::input_conversion_quote(),
        NativeValueQuote {
            work: 3,
            requested_bytes: size_of::<(ClassType<'_>, Name)>(),
            cleanup_work: 1,
        },
    );
    assert_eq!(
        EffectiveVariableKindProfile::output_comparison_quote(),
        NativeValueQuote {
            work: 2,
            requested_bytes: size_of::<bool>(),
            cleanup_work: 0,
        },
    );
}

/// The scalar boolean query accounts for its tuple copy and comparison result representation.
#[test]
fn function_definition_native_quote_covers_scalar_representations() {
    assert_eq!(
        FunctionDefinitionProfile::input_conversion_quote(),
        NativeValueQuote {
            work: 3,
            requested_bytes: size_of::<(ScopeId<'_>, ScopedSymbolId)>(),
            cleanup_work: 0,
        },
    );
    assert_eq!(
        FunctionDefinitionProfile::output_comparison_quote(),
        NativeValueQuote {
            work: 1,
            requested_bytes: size_of::<bool>(),
            cleanup_work: 0,
        },
    );
}
