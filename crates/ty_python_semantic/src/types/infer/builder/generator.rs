use super::TypeInferenceBuilder;
use crate::types::GeneratorTypes;
use crate::types::infer::nearest_enclosing_function;

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(crate) fn enclosing_generator_type_params(&self) -> Option<GeneratorTypes<'db>> {
        let db = self.db();
        let enclosing_function = nearest_enclosing_function(db, self.index, self.scope())?;
        let declared_return_ty = enclosing_function
            .last_definition_raw_signature(db)
            .return_ty;

        let Some(expected_types) = declared_return_ty.generator_types(db) else {
            // If it does not have the required annotation
            // TODO: is this an error or no?
            return None;
        };

        // TODO: Validate that function return type and async matches
        // e.g. async def cannot return Generator
        Some(expected_types)
    }
}
