use std::convert::Infallible;

use ruff_python_ast::name::Name;
use smallvec::SmallVec;
use ty_python_core::definition::Definition;
use ty_python_core::reachability_constraints::ScopedReachabilityConstraintId;
use ty_python_core::scope::ScopeId;
use ty_python_core::{BindingWithConstraintsIterator, UseDefMap, place_table, use_def_map};

use super::Member;
use crate::Db;
use crate::reachability::ReachabilityConstraintsExtension;
use crate::types::function::{FunctionType, OverloadLiteral};
use crate::types::storage_quote::{StorageQuote, buffer_push_quote, buffer_retirement};
use crate::types::{
    BoundMethodType, PropertyInstanceType, Type, UnionType, infer_definition_types, legacy_inline,
};

pub(in crate::types) type LocalFunctions<'db> = SmallVec<[FunctionType<'db>; 1]>;
pub(in crate::types) type LocalDefinitions<'db> = SmallVec<[Definition<'db>; 1]>;

pub(in crate::types) struct FunctionBindings<'db> {
    pub(in crate::types) uses: &'db UseDefMap<'db>,
    pub(in crate::types) bindings: BindingWithConstraintsIterator<'db, 'db>,
}

impl<'db> FunctionBindings<'db> {
    pub(in crate::types) fn next(
        &mut self,
    ) -> Option<(Option<Definition<'db>>, ScopedReachabilityConstraintId)> {
        self.bindings.next().map(|binding| {
            (
                binding.binding.definition(),
                binding.reachability_constraint,
            )
        })
    }
}

pub(in crate::types) trait LocalFunctionEffects<'db> {
    type Error;

    async fn local_quoted<T>(
        &self,
        quote: Option<StorageQuote>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;
    async fn initialize_value<T>(&self, action: impl FnOnce() -> T) -> Result<T, Self::Error>;
    async fn underlying(&self, ty: Type<'db>) -> Result<LocalFunctions<'db>, Self::Error>;
    async fn from_type(
        &self,
        ty: Type<'db>,
        scope: ScopeId<'db>,
    ) -> Result<LocalFunctions<'db>, Self::Error>;
    async fn end_scope(
        &self,
        scope: ScopeId<'db>,
        name: &Name,
    ) -> Result<LocalDefinitions<'db>, Self::Error>;
    async fn contains_definition(
        &self,
        function: FunctionType<'db>,
        definition: Definition<'db>,
    ) -> Result<bool, Self::Error>;
    async fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Self::Error>;
    async fn accessors(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> Result<[Option<Type<'db>>; 3], Self::Error>;
    async fn getter(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;
    async fn bound_callable(&self, method: BoundMethodType<'db>) -> Result<Type<'db>, Self::Error>;
    async fn function_scope(
        &self,
        function: FunctionType<'db>,
    ) -> Result<ScopeId<'db>, Self::Error>;
    async fn function_name(&self, function: FunctionType<'db>) -> Result<&'db Name, Self::Error>;
    async fn names_equal(&self, left: &Name, right: &Name) -> Result<bool, Self::Error>;
    async fn bindings(
        &self,
        scope: ScopeId<'db>,
        name: &Name,
    ) -> Result<Option<FunctionBindings<'db>>, Self::Error>;
    async fn reachable(
        &self,
        bindings: &FunctionBindings<'db>,
        constraint: ScopedReachabilityConstraintId,
    ) -> Result<bool, Self::Error>;
    async fn is_function(&self, definition: Definition<'db>) -> Result<bool, Self::Error>;
    async fn inferred_function(
        &self,
        definition: Definition<'db>,
    ) -> Result<Option<FunctionType<'db>>, Self::Error>;
    async fn overloads(
        &self,
        function: FunctionType<'db>,
    ) -> Result<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>), Self::Error>;
    async fn overload_definition(
        &self,
        overload: OverloadLiteral<'db>,
    ) -> Result<Definition<'db>, Self::Error>;
}

fn fixed<T>(work: usize) -> Option<StorageQuote> {
    Some(StorageQuote {
        work,
        bytes: size_of::<T>(),
    })
}

async fn buffer<'db, T: Copy, E: LocalFunctionEffects<'db>>(
    effects: &E,
) -> Result<SmallVec<[T; 1]>, E::Error> {
    effects
        .local_quoted(
            buffer_retirement::<T>((0, 1, false)).map(|work| StorageQuote { work, bytes: 0 }),
            || (),
        )
        .await?;
    effects.initialize_value(SmallVec::new).await
}

async fn append<'db, T: Copy, E: LocalFunctionEffects<'db>>(
    values: &mut SmallVec<[T; 1]>,
    value: T,
    effects: &E,
) -> Result<(), E::Error> {
    let storage = effects
        .local_quoted(fixed::<(usize, usize, bool)>(3), || {
            (values.len(), values.capacity(), values.spilled())
        })
        .await?;
    // A single push doubles a full SmallVec backing. The quote prepays backing retirement on
    // growth and each entry's retirement on insertion. This also covers dropping a partially built
    // buffer if a later child query refuses work or is cancelled.
    effects
        .local_quoted(buffer_push_quote::<T>(storage), || values.push(value))
        .await
}

async fn next<'db, T: Copy, E: LocalFunctionEffects<'db>>(
    values: &[T],
    cursor: &mut usize,
    effects: &E,
) -> Result<Option<T>, E::Error> {
    effects
        .local_quoted(fixed::<Option<T>>(4), || {
            let value = values.get(*cursor).copied();
            *cursor += usize::from(value.is_some());
            value
        })
        .await
}

async fn extend<'db, T: Copy, E: LocalFunctionEffects<'db>>(
    values: &mut SmallVec<[T; 1]>,
    source: &[T],
    effects: &E,
) -> Result<(), E::Error> {
    let mut cursor = 0;
    while let Some(value) = next(source, &mut cursor, effects).await? {
        append(values, value, effects).await?;
    }
    Ok(())
}

async fn finish<'db, T: Copy, E: LocalFunctionEffects<'db>>(
    values: &mut SmallVec<[T; 1]>,
    effects: &E,
) -> Result<SmallVec<[T; 1]>, E::Error> {
    effects.local_quoted(fixed::<()>(4), || ()).await?;
    // `initialize_value` runs the move only after its work and byte charges succeed, so refusal
    // leaves `values` owning its prepaid backing. The returned buffer carries that cleanup
    // obligation; the replacement empty buffer's retirement is paid above.
    effects.initialize_value(|| std::mem::take(values)).await
}

#[derive(Clone, Copy)]
enum ExtractionFrame<'db> {
    Type(Type<'db>),
    Union {
        elements: &'db [Type<'db>],
        cursor: usize,
    },
}

pub(in crate::types) async fn underlying_functions_with<'db, E: LocalFunctionEffects<'db>>(
    ty: Type<'db>,
    effects: &E,
) -> Result<LocalFunctions<'db>, E::Error> {
    let mut functions = buffer(effects).await?;
    let mut frames = buffer(effects).await?;
    append(&mut frames, ExtractionFrame::Type(ty), effects).await?;
    while let Some(frame) = effects
        .local_quoted(fixed::<Option<ExtractionFrame<'db>>>(2), || frames.pop())
        .await?
    {
        match frame {
            ExtractionFrame::Type(Type::FunctionLiteral(function)) => {
                append(&mut functions, function, effects).await?;
            }
            ExtractionFrame::Type(Type::BoundMethod(method)) => {
                let ty = effects.bound_callable(method).await?;
                append(&mut frames, ExtractionFrame::Type(ty), effects).await?;
            }
            ExtractionFrame::Type(Type::PropertyInstance(property)) => {
                if let Some(ty) = effects.getter(property).await? {
                    append(&mut frames, ExtractionFrame::Type(ty), effects).await?;
                }
            }
            ExtractionFrame::Type(Type::Union(union)) => {
                let elements = effects.union_elements(union).await?;
                append(
                    &mut frames,
                    ExtractionFrame::Union {
                        elements,
                        cursor: 0,
                    },
                    effects,
                )
                .await?;
            }
            ExtractionFrame::Type(_) => {}
            ExtractionFrame::Union {
                elements,
                mut cursor,
            } => {
                if let Some(ty) = next(elements, &mut cursor, effects).await? {
                    append(
                        &mut frames,
                        ExtractionFrame::Union { elements, cursor },
                        effects,
                    )
                    .await?;
                    append(&mut frames, ExtractionFrame::Type(ty), effects).await?;
                }
            }
        }
    }
    finish(&mut functions, effects).await
}

pub(in crate::types) async fn local_functions_from_type_with<'db, E: LocalFunctionEffects<'db>>(
    ty: Type<'db>,
    scope: ScopeId<'db>,
    effects: &E,
) -> Result<LocalFunctions<'db>, E::Error> {
    let mut functions = buffer(effects).await?;
    let mut types = buffer(effects).await?;
    append(&mut types, ty, effects).await?;
    let mut cursor = 0;
    while let Some(ty) = next(&types, &mut cursor, effects).await? {
        match ty {
            Type::PropertyInstance(property) => {
                let accessors = effects.accessors(property).await?;
                let mut accessor_cursor = 0;
                while let Some(accessor) = next(&accessors, &mut accessor_cursor, effects).await? {
                    if let Some(accessor) = accessor {
                        let extracted = effects.underlying(accessor).await?;
                        extend(&mut functions, &extracted, effects).await?;
                    }
                }
            }
            Type::Union(union) => {
                let elements = effects.union_elements(union).await?;
                extend(&mut types, elements, effects).await?;
            }
            _ => {
                let extracted = effects.underlying(ty).await?;
                extend(&mut functions, &extracted, effects).await?;
            }
        }
    }
    let mut filtered = buffer(effects).await?;
    let mut cursor = 0;
    while let Some(function) = next(&functions, &mut cursor, effects).await? {
        let function_scope = effects.function_scope(function).await?;
        if effects
            .local_quoted(fixed::<bool>(1), || function_scope == scope)
            .await?
        {
            append(&mut filtered, function, effects).await?;
        }
    }
    finish(&mut filtered, effects).await
}

pub(in crate::types) async fn end_scope_functions_with<'db, E: LocalFunctionEffects<'db>>(
    scope: ScopeId<'db>,
    name: &Name,
    effects: &E,
) -> Result<LocalDefinitions<'db>, E::Error> {
    let mut definitions = buffer(effects).await?;
    if let Some(mut bindings) = effects.bindings(scope, name).await? {
        while let Some((definition, constraint)) = effects
            .local_quoted(
                fixed::<Option<(Option<Definition<'db>>, ScopedReachabilityConstraintId)>>(4),
                || bindings.next(),
            )
            .await?
        {
            if let Some(definition) = definition
                && effects.reachable(&bindings, constraint).await?
                && effects.is_function(definition).await?
            {
                append(&mut definitions, definition, effects).await?;
            }
        }
    }
    finish(&mut definitions, effects).await
}

pub(in crate::types) async fn contains_definition_with<'db, E: LocalFunctionEffects<'db>>(
    function: FunctionType<'db>,
    definition: Definition<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    let (overloads, implementation) = effects.overloads(function).await?;
    let mut cursor = 0;
    while let Some(overload) = next(overloads, &mut cursor, effects).await? {
        let candidate = effects.overload_definition(overload).await?;
        if effects
            .local_quoted(fixed::<bool>(1), || candidate == definition)
            .await?
        {
            return effects.initialize_value(|| true).await;
        }
    }
    if let Some(implementation) = implementation {
        let candidate = effects.overload_definition(implementation).await?;
        return effects
            .local_quoted(fixed::<bool>(1), || candidate == definition)
            .await;
    }
    effects.initialize_value(|| false).await
}

async fn append_unique<'db, E: LocalFunctionEffects<'db>>(
    functions: &mut LocalFunctions<'db>,
    function: FunctionType<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    let mut cursor = 0;
    while let Some(candidate) = next(functions, &mut cursor, effects).await? {
        if effects
            .local_quoted(fixed::<bool>(1), || candidate == function)
            .await?
        {
            return Ok(());
        }
    }
    append(functions, function, effects).await
}

pub(in crate::types) async fn local_member_functions_with<'db, E: LocalFunctionEffects<'db>>(
    member: &Member<'db>,
    scope: ScopeId<'db>,
    effects: &E,
) -> Result<LocalFunctions<'db>, E::Error> {
    let mut member_functions = effects.from_type(member.ty, scope).await?;
    let mut cursor = 0;
    let mut retained = 0;
    while let Some(function) = next(&member_functions, &mut cursor, effects).await? {
        let name = effects.function_name(function).await?;
        if effects.names_equal(name, &member.name).await? {
            effects
                .local_quoted(fixed::<(FunctionType<'db>, usize)>(2), || {
                    member_functions[retained] = function;
                    retained += 1;
                })
                .await?;
        }
    }
    let removed = effects
        .local_quoted(fixed::<Option<usize>>(2), || {
            member_functions.len().checked_sub(retained)
        })
        .await?;
    effects
        .local_quoted(
            removed
                .and_then(|removed| removed.checked_add(1))
                .map(|work| StorageQuote { work, bytes: 0 }),
            || member_functions.truncate(retained),
        )
        .await?;
    let mut functions = buffer(effects).await?;
    let definitions = effects.end_scope(scope, &member.name).await?;
    let mut cursor = 0;
    while let Some(definition) = next(&definitions, &mut cursor, effects).await? {
        let mut found = None;
        let mut candidates = 0;
        while let Some(function) = next(&member_functions, &mut candidates, effects).await? {
            if effects.contains_definition(function, definition).await? {
                found = Some(function);
                break;
            }
        }
        let function = match found {
            Some(function) => Some(function),
            None => effects.inferred_function(definition).await?,
        };
        if let Some(function) = function {
            append_unique(&mut functions, function, effects).await?;
        }
    }
    // A property can retain a getter even though only its setter is an end-of-scope binding.
    let mut cursor = 0;
    while let Some(function) = next(&member_functions, &mut cursor, effects).await? {
        append_unique(&mut functions, function, effects).await?;
    }
    finish(&mut functions, effects).await
}

pub(super) struct OrdinaryLocalFunctionEffects<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> LocalFunctionEffects<'db> for OrdinaryLocalFunctionEffects<'db> {
    type Error = Infallible;

    async fn local_quoted<T>(
        &self,
        _quote: Option<StorageQuote>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn initialize_value<T>(&self, action: impl FnOnce() -> T) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn underlying(&self, ty: Type<'db>) -> Result<LocalFunctions<'db>, Self::Error> {
        Ok(legacy_inline(underlying_functions_with(ty, self)))
    }

    async fn from_type(
        &self,
        ty: Type<'db>,
        scope: ScopeId<'db>,
    ) -> Result<LocalFunctions<'db>, Self::Error> {
        Ok(legacy_inline(local_functions_from_type_with(
            ty, scope, self,
        )))
    }

    async fn end_scope(
        &self,
        scope: ScopeId<'db>,
        name: &Name,
    ) -> Result<LocalDefinitions<'db>, Self::Error> {
        Ok(super::end_of_scope_function_definitions(
            self.db, scope, name,
        ))
    }

    async fn contains_definition(
        &self,
        function: FunctionType<'db>,
        definition: Definition<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(legacy_inline(contains_definition_with(
            function, definition, self,
        )))
    }

    async fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(union.elements(self.db))
    }

    async fn accessors(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> Result<[Option<Type<'db>>; 3], Self::Error> {
        Ok([
            property.getter(self.db),
            property.setter(self.db),
            property.deleter(self.db),
        ])
    }

    async fn getter(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(property.getter(self.db))
    }

    async fn bound_callable(&self, method: BoundMethodType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(method.func(self.db))
    }

    async fn function_scope(
        &self,
        function: FunctionType<'db>,
    ) -> Result<ScopeId<'db>, Self::Error> {
        Ok(function.definition(self.db).scope(self.db))
    }

    async fn function_name(&self, function: FunctionType<'db>) -> Result<&'db Name, Self::Error> {
        Ok(function.name(self.db))
    }

    async fn names_equal(&self, left: &Name, right: &Name) -> Result<bool, Self::Error> {
        Ok(left == right)
    }

    async fn bindings(
        &self,
        scope: ScopeId<'db>,
        name: &Name,
    ) -> Result<Option<FunctionBindings<'db>>, Self::Error> {
        let table = place_table(self.db, scope);
        let Some(symbol) = table.symbol_id(name) else {
            return Ok(None);
        };
        let uses = use_def_map(self.db, scope);
        Ok(Some(FunctionBindings {
            uses,
            bindings: uses.end_of_scope_symbol_bindings(symbol),
        }))
    }

    async fn reachable(
        &self,
        bindings: &FunctionBindings<'db>,
        constraint: ScopedReachabilityConstraintId,
    ) -> Result<bool, Self::Error> {
        Ok(!bindings
            .uses
            .reachability_constraints()
            .evaluate(self.db, bindings.uses.predicates(), constraint)
            .is_always_false())
    }

    async fn is_function(&self, definition: Definition<'db>) -> Result<bool, Self::Error> {
        Ok(definition.kind(self.db).is_function_def())
    }

    async fn inferred_function(
        &self,
        definition: Definition<'db>,
    ) -> Result<Option<FunctionType<'db>>, Self::Error> {
        Ok(infer_definition_types(self.db, definition).function_type(definition))
    }

    async fn overloads(
        &self,
        function: FunctionType<'db>,
    ) -> Result<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>), Self::Error> {
        Ok(function.overloads_and_implementation(self.db))
    }

    async fn overload_definition(
        &self,
        overload: OverloadLiteral<'db>,
    ) -> Result<Definition<'db>, Self::Error> {
        Ok(overload.definition(self.db))
    }
}
