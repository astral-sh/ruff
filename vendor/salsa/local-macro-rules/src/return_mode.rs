/// Generate the expression for the return type, depending on the return mode defined in [`salsa-macros::options::Options::returns`]
///
/// Used when generating field getters.
#[macro_export]
macro_rules! return_mode_expression {
    (
        (copy, $maybe_default:ident),
        $field_ty:ty,
        $field_ref_expr:expr,
    ) => {
        *$field_ref_expr
    };

    (
        (clone, $maybe_default:ident),
        $field_ty:ty,
        $field_ref_expr:expr,
    ) => {
        ::core::clone::Clone::clone($field_ref_expr)
    };

    (
        (ref, $maybe_default:ident),
        $field_ty:ty,
        $field_ref_expr:expr,
    ) => {
        $field_ref_expr
    };

    (
        (deref, $maybe_default:ident),
        $field_ty:ty,
        $field_ref_expr:expr,
    ) => {
        ::core::ops::Deref::deref($field_ref_expr)
    };

    (
        (as_ref, $maybe_default:ident),
        $field_ty:ty,
        $field_ref_expr:expr,
    ) => {
        ::salsa::SalsaAsRef::as_ref($field_ref_expr)
    };

    (
        (as_deref, $maybe_default:ident),
        $field_ty:ty,
        $field_ref_expr:expr,
    ) => {
        ::salsa::SalsaAsDeref::as_deref($field_ref_expr)
    };
}

#[macro_export]
macro_rules! return_mode_ty {
    (
        (copy, $maybe_default:ident),
        $db_lt:lifetime,
        $field_ty:ty
    ) => {
        $field_ty
    };

    (
        (clone, $maybe_default:ident),
        $db_lt:lifetime,
        $field_ty:ty
    ) => {
        $field_ty
    };

    (
        (ref, $maybe_default:ident),
        $db_lt:lifetime,
        $field_ty:ty
    ) => {
        & $db_lt $field_ty
    };

    (
        (deref, $maybe_default:ident),
        $db_lt:lifetime,
        $field_ty:ty
    ) => {
        & $db_lt <$field_ty as ::core::ops::Deref>::Target
    };

    (
        (as_ref, $maybe_default:ident),
        $db_lt:lifetime,
        $field_ty:ty
    ) => {
        <$field_ty as ::salsa::SalsaAsRef>::AsRef<$db_lt>
    };

    (
        (as_deref, $maybe_default:ident),
        $db_lt:lifetime,
        $field_ty:ty
    ) => {
        <$field_ty as ::salsa::SalsaAsDeref>::AsDeref<$db_lt>
    };
}

/// Identifies the generated getter conversion for native value quotation.
#[macro_export]
macro_rules! return_mode_field_mode {
    ((copy, $maybe_default:ident)) => {
        ::salsa::plumbing::FieldReturnMode::Copy
    };
    ((clone, $maybe_default:ident)) => {
        ::salsa::plumbing::FieldReturnMode::Clone
    };
    ((ref, $maybe_default:ident)) => {
        ::salsa::plumbing::FieldReturnMode::Ref
    };
    ((deref, $maybe_default:ident)) => {
        ::salsa::plumbing::FieldReturnMode::Deref
    };
    ((as_ref, $maybe_default:ident)) => {
        ::salsa::plumbing::FieldReturnMode::AsRef
    };
    ((as_deref, $maybe_default:ident)) => {
        ::salsa::plumbing::FieldReturnMode::AsDeref
    };
}
