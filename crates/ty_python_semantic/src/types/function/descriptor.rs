use std::convert::Infallible;
use std::iter::{Chain, Copied};
use std::option;
use std::slice;

use super::{FunctionType, OverloadLiteral};
use crate::Db;
use crate::types::callable::CallableTypeKind;

#[cfg(all(test, feature = "experimental-analysis"))]
mod test_support;

pub(in crate::types) type FunctionDefinitionsCursor<'db> =
    Chain<Copied<slice::Iter<'db, OverloadLiteral<'db>>>, option::IntoIter<OverloadLiteral<'db>>>;

pub(super) struct OrdinaryFunctionDescriptor<'db> {
    pub(super) db: &'db dyn Db,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousFunctionTypeDescriptorEffects)]
    pub(in crate::types) trait FunctionTypeDescriptorEffects<'db> {
        type Error;
        type Definitions;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn descriptor_kind(&self, function: FunctionType<'db>) -> Result<Option<CallableTypeKind>, Self::Error>;
        #[operation(child)]
        async fn definitions(&self, function: FunctionType<'db>) -> Result<Self::Definitions, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_definition(&self, definitions: &mut Self::Definitions) -> Result<Option<OverloadLiteral<'db>>, Self::Error>;
        #[operation(child)]
        async fn overload_is_classmethod(&self, overload: OverloadLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn staticmethod_declaration(&self, function: FunctionType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn classmethod(&self, function: FunctionType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn staticmethod(&self, function: FunctionType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn with_kind(&self, function: FunctionType<'db>, kind: CallableTypeKind) -> Result<FunctionType<'db>, Self::Error>;
        /// Interns the original literal and stored signatures with the supplied descriptor override.
        #[operation(child)]
        async fn rebuild(&self, function: FunctionType<'db>, kind: Option<CallableTypeKind>) -> Result<FunctionType<'db>, Self::Error>;
        #[operation(child)]
        async fn callable_kind(&self, function: FunctionType<'db>) -> Result<CallableTypeKind, Self::Error>;
        #[operation(local)]
        async fn same_kind(&self, left: CallableTypeKind, right: CallableTypeKind) -> Result<bool, Self::Error>;
    }

    /// Changes descriptor behavior while retaining stored signatures. A request matching the
    /// declaration's kind restores its canonical declared identity.
    #[synchronous(with_descriptor_kind_sync)]
    #[capabilities(effects = FunctionTypeDescriptorEffects)]
    #[passive_values()]
    pub(in crate::types) async fn with_descriptor_kind_with<'db, E: FunctionTypeDescriptorEffects<'db>>(
        function: FunctionType<'db>, kind: CallableTypeKind, effects: &E,
    ) -> Result<FunctionType<'db>, E::Error> {
        effects.checkpoint().await?;
        // Keep the original representation when wrapping and unwrapping returns to
        // the declaration's kind, so the same function retains a single identity.
        let declared = effects.rebuild(function, None).await?;
        let declared_kind = effects.callable_kind(declared).await?;
        if effects.same_kind(declared_kind, kind).await? {
            return Ok(declared);
        }
        effects.rebuild(function, Some(kind)).await
    }

    #[synchronous(function_is_classmethod_sync)]
    #[capabilities(effects = FunctionTypeDescriptorEffects)]
    #[passive_values()]
    pub(in crate::types) async fn function_is_classmethod_with<'db, E: FunctionTypeDescriptorEffects<'db>>(
        function: FunctionType<'db>, effects: &E,
    ) -> Result<bool, E::Error> {
        effects.checkpoint().await?;
        match effects.descriptor_kind(function).await? {
            Some(CallableTypeKind::ClassMethodLike) => return Ok(true),
            Some(_) => return Ok(false),
            None => {}
        }
        let mut definitions = effects.definitions(function).await?;
        // Overload discovery can return no definitions during cycle recovery.
        let Some(first) = effects.next_definition(&mut definitions).await? else {
            return Ok(false);
        };
        if !effects.overload_is_classmethod(first).await? {
            return Ok(false);
        }
        #[cursor_loop]
        while let Some(overload) = effects.next_definition(&mut definitions).await? {
            if !effects.overload_is_classmethod(overload).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    #[synchronous(function_is_staticmethod_sync)]
    #[capabilities(effects = FunctionTypeDescriptorEffects)]
    #[passive_values()]
    pub(in crate::types) async fn function_is_staticmethod_with<'db, E: FunctionTypeDescriptorEffects<'db>>(
        function: FunctionType<'db>, effects: &E,
    ) -> Result<bool, E::Error> {
        effects.checkpoint().await?;
        match effects.descriptor_kind(function).await? {
            Some(CallableTypeKind::StaticMethodLike) => Ok(true),
            Some(_) => Ok(false),
            None => effects.staticmethod_declaration(function).await,
        }
    }

    #[synchronous(underlying_function_sync)]
    #[capabilities(effects = FunctionTypeDescriptorEffects)]
    #[passive_values(CallableTypeKind::FunctionLike)]
    pub(in crate::types) async fn underlying_function_with<'db, E: FunctionTypeDescriptorEffects<'db>>(
        function: FunctionType<'db>, effects: &E,
    ) -> Result<FunctionType<'db>, E::Error> {
        effects.checkpoint().await?;
        if effects.classmethod(function).await? || effects.staticmethod(function).await? {
            effects.with_kind(function, CallableTypeKind::FunctionLike).await
        } else {
            Ok(function)
        }
    }
}

impl<'db> SynchronousFunctionTypeDescriptorEffects<'db> for OrdinaryFunctionDescriptor<'db> {
    type Error = Infallible;
    type Definitions = FunctionDefinitionsCursor<'db>;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn descriptor_kind(
        &self,
        function: FunctionType<'db>,
    ) -> Result<Option<CallableTypeKind>, Self::Error> {
        Ok(function.descriptor_kind(self.db))
    }

    fn definitions(&self, function: FunctionType<'db>) -> Result<Self::Definitions, Self::Error> {
        let (overloads, implementation) = function.overloads_and_implementation(self.db);
        Ok(overloads.iter().copied().chain(implementation))
    }

    fn next_definition(
        &self,
        definitions: &mut Self::Definitions,
    ) -> Result<Option<OverloadLiteral<'db>>, Self::Error> {
        Ok(definitions.next())
    }

    fn overload_is_classmethod(&self, overload: OverloadLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(overload.is_classmethod(self.db))
    }

    fn staticmethod_declaration(&self, function: FunctionType<'db>) -> Result<bool, Self::Error> {
        Ok(function.has_staticmethod_declaration(self.db))
    }

    fn classmethod(&self, function: FunctionType<'db>) -> Result<bool, Self::Error> {
        Ok(function.is_classmethod(self.db))
    }

    fn staticmethod(&self, function: FunctionType<'db>) -> Result<bool, Self::Error> {
        Ok(function.is_staticmethod(self.db))
    }

    fn with_kind(
        &self,
        function: FunctionType<'db>,
        kind: CallableTypeKind,
    ) -> Result<FunctionType<'db>, Self::Error> {
        Ok(function.with_descriptor_kind(self.db, kind))
    }

    fn rebuild(
        &self,
        function: FunctionType<'db>,
        kind: Option<CallableTypeKind>,
    ) -> Result<FunctionType<'db>, Self::Error> {
        Ok(FunctionType::new_internal(
            self.db,
            function.literal(self.db),
            function.updated_signatures(self.db),
            kind,
        ))
    }

    fn callable_kind(&self, function: FunctionType<'db>) -> Result<CallableTypeKind, Self::Error> {
        Ok(function.callable_type_kind(self.db))
    }

    fn same_kind(&self, left: CallableTypeKind, right: CallableTypeKind) -> Result<bool, Self::Error> {
        Ok(left == right)
    }
}
