#[macro_export]
macro_rules! setup_tracked_assoc_fn_body {
    (
        salsa_tracked_attr: #[$salsa_tracked_attr:meta],
        self_ty: $self_ty:ty,
        db_lt: $($db_lt:lifetime)?,
        db: $db:ident,
        db_ty: ($($db_ty:tt)*),
        input_ids: [$($input_id:ident),*],
        input_tys: [$($input_ty:ty),*],
        output_ty: $output_ty:ty,
        inner_fn_name: $inner_fn_name:ident,
        inner_fn: $inner_fn:item,
        tracked_fn: $tracked_fn:item,

        // Annoyingly macro-rules hygiene does not extend to items defined in the macro.
        // We have the procedural macro generate names for those items that are
        // not used elsewhere in the user's code.
        unused_names: [
            $InnerTrait:ident,
        ]
    ) => {
        {
            trait $InnerTrait<$($db_lt)?> {
                fn $inner_fn_name(db: $($db_ty)*, $($input_id: $input_ty),*) -> $output_ty;
            }

            impl<$($db_lt)?> $InnerTrait<$($db_lt)?> for $self_ty {
                $inner_fn
            }

            $tracked_fn

            $inner_fn_name($db, $($input_id),*)
        }
    };
}
