/// Generate either `field_ref_expr` or `field_ty::default`
///
/// Used when generating an input's builder.
#[macro_export]
macro_rules! maybe_default {
    (
        ($return_mode:ident, default),
        $field_ty:ty,
        $field_ref_expr:expr,
    ) => {
        <$field_ty>::default()
    };

    (
        ($return_mode:ident, required),
        $field_ty:ty,
        $field_ref_expr:expr,
    ) => {
        $field_ref_expr
    };
}

#[macro_export]
macro_rules! maybe_default_tt {
    (($return_mode:ident, default) => $($t:tt)*) => {
        $($t)*
    };

    (($return_mode:ident, required) => $($t:tt)*) => {

    };
}
