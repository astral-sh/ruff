use super::TypeInferenceBuilder;
use crate::types::infer::nearest_enclosing_function;
use crate::types::{GeneratorTypes, Type};

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(crate) fn enclosing_generator_type_params(&self) -> Option<GeneratorTypes<'db>> {
        let db = self.db();
        let enclosing_function = nearest_enclosing_function(db, self.index, self.scope())?;
        let declared_return_ty = enclosing_function
            .last_definition_raw_signature(db)
            .return_ty;

        let expected_types = declared_return_ty.generator_types(db).unwrap_or_default();

        let send_ty = expected_types.sent.or(Some(Type::none(db)));
        let yield_ty = expected_types.yielded.or(Some(Type::none(db)));

        // TODO: Validate that function return type and async matches
        let return_ty = expected_types.returned.or(Some(Type::none(db)));

        let types = GeneratorTypes {
            yielded: yield_ty,
            sent: send_ty,
            returned: return_ty,
        };

        Some(types)
    }
}
