use std::convert::Infallible;
use std::slice;

use super::buffer::{fixed_spec, variable_spec};
use super::{Tuple, TupleSpec, TupleType, VariableSegment};
use crate::types::Type;
use crate::types::normalization::{
    OrdinaryNormalizationEffects, RecursiveNormalizationFacts, RecursiveNormalizationRequest,
    recursive_normalize_sync,
};
use crate::{Program, ProgramEnvironment};

pub(in crate::types) struct TupleNormalizationFacts;

enum TupleNormalizationParts<'a, 'db> {
    Fixed(&'a [Type<'db>]),
    Variable {
        prefix: &'a [Type<'db>],
        variable: VariableSegment<'db>,
        suffix: &'a [Type<'db>],
    },
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTupleNormalizationEffects)]
    pub(in crate::types) trait TupleNormalizationEffects<'db> {
        type Error;
        type Buffer;

        #[operation(source)]
        async fn program(&self, env: &ProgramEnvironment<'db>) -> Result<Program<'db>, Self::Error>;
        #[operation(local)]
        async fn spec(&self, tuple: TupleType<'db>) -> Result<&'db TupleSpec<'db>, Self::Error>;
        #[operation(child)]
        async fn normalize_spec(&self, spec: &TupleSpec<'db>, env: &ProgramEnvironment<'db>, program: Program<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<TupleSpec<'db>>, Self::Error>;
        #[operation(child)]
        async fn normalize_child(&self, request: RecursiveNormalizationRequest<'db>, env: &ProgramEnvironment<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_element(&self, elements: &mut slice::Iter<'_, Type<'db>>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn new_buffer(&self, capacity: usize) -> Result<Self::Buffer, Self::Error>;
        #[operation(local)]
        async fn push(&self, buffer: &mut Self::Buffer, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn set_variable(&self, buffer: &mut Self::Buffer, variable: VariableSegment<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish_buffer(&self, buffer: &mut Self::Buffer) -> Result<TupleSpec<'db>, Self::Error>;
        #[operation(source)]
        async fn intern_tuple(&self, program: Program<'db>, spec: TupleSpec<'db>) -> Result<TupleType<'db>, Self::Error>;
    }

    #[finite_capability]
    impl TupleNormalizationFacts {
        fn parts<'a, 'db>(&self, spec: &'a TupleSpec<'db>) -> TupleNormalizationParts<'a, 'db> {
            match spec {
                Tuple::Fixed(tuple) => TupleNormalizationParts::Fixed(tuple.all_elements()),
                Tuple::Variable(tuple) => TupleNormalizationParts::Variable {
                    prefix: tuple.prefix_elements(),
                    variable: tuple.variable(),
                    suffix: tuple.suffix_elements(),
                },
            }
        }

        fn fixed_count(&self, spec: &TupleSpec<'_>) -> usize {
            match spec {
                Tuple::Fixed(tuple) => tuple.len(),
                Tuple::Variable(tuple) => tuple.fixed_elements.len(),
            }
        }

        fn elements<'a, 'db>(&self, types: &'a [Type<'db>]) -> slice::Iter<'a, Type<'db>> {
            types.iter()
        }

        fn child<'db>(&self, ty: Type<'db>, divergent: Type<'db>) -> RecursiveNormalizationRequest<'db> {
            RecursiveNormalizationRequest { ty, divergent, nested: true }
        }
    }

    #[synchronous(tuple_normalize_sync)]
    #[capabilities(effects = TupleNormalizationEffects)]
    #[passive_values()]
    pub(in crate::types) async fn tuple_normalize_with<'db, E: TupleNormalizationEffects<'db>>(
        tuple: TupleType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
        effects: &E,
    ) -> Result<Option<TupleType<'db>>, E::Error> {
        let program = effects.program(env).await?;
        let spec = effects.spec(tuple).await?;
        match effects.normalize_spec(spec, env, program, divergent, nested).await? {
            Some(spec) => Ok(Some(effects.intern_tuple(program, spec).await?)),
            None => Ok(None),
        }
    }

    #[synchronous(tuple_spec_normalize_sync)]
    #[capabilities(effects = TupleNormalizationEffects, facts = TupleNormalizationFacts)]
    #[passive_values(VariableSegment::Homogeneous, VariableSegment::TypeVarTuple)]
    pub(in crate::types) async fn tuple_spec_normalize_with<'db, E: TupleNormalizationEffects<'db>>(
        spec: &TupleSpec<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
        effects: &E,
        facts: TupleNormalizationFacts,
    ) -> Result<Option<TupleSpec<'db>>, E::Error> {
        match facts.parts(spec) {
            TupleNormalizationParts::Fixed(types) => {
                let mut buffer = effects.new_buffer(facts.fixed_count(spec)).await?;
                let mut elements = facts.elements(types);
                #[cursor_loop]
                while let Some(ty) = effects.next_element(&mut elements).await? {
                    let normalized = effects.normalize_child(facts.child(ty, divergent), env).await?;
                    let normalized = match normalized {
                        Some(ty) => ty,
                        None if nested => return Ok(None),
                        None => divergent,
                    };
                    effects.push(&mut buffer, normalized).await?;
                }
                Ok(Some(effects.finish_buffer(&mut buffer).await?))
            }
            TupleNormalizationParts::Variable { prefix, variable, suffix } => {
                let variable = match variable {
                    VariableSegment::Homogeneous(ty) => {
                        let normalized = effects.normalize_child(facts.child(ty, divergent), env).await?;
                        let normalized = match normalized {
                            Some(ty) => ty,
                            None if nested => return Ok(None),
                            None => divergent,
                        };
                        VariableSegment::Homogeneous(normalized)
                    }
                    VariableSegment::TypeVarTuple(typevartuple) => VariableSegment::TypeVarTuple(typevartuple),
                };
                let mut buffer = effects.new_buffer(facts.fixed_count(spec)).await?;
                let mut prefix = facts.elements(prefix);
                #[cursor_loop]
                while let Some(ty) = effects.next_element(&mut prefix).await? {
                    let normalized = effects.normalize_child(facts.child(ty, divergent), env).await?;
                    let normalized = match normalized {
                        Some(ty) => ty,
                        None if nested => return Ok(None),
                        None => divergent,
                    };
                    effects.push(&mut buffer, normalized).await?;
                }
                effects.set_variable(&mut buffer, variable).await?;
                let mut suffix = facts.elements(suffix);
                #[cursor_loop]
                while let Some(ty) = effects.next_element(&mut suffix).await? {
                    let normalized = effects.normalize_child(facts.child(ty, divergent), env).await?;
                    let normalized = match normalized {
                        Some(ty) => ty,
                        None if nested => return Ok(None),
                        None => divergent,
                    };
                    effects.push(&mut buffer, normalized).await?;
                }
                Ok(Some(effects.finish_buffer(&mut buffer).await?))
            }
        }
    }
}

pub(in crate::types) struct NormalizedTupleElements<'db> {
    elements: Vec<Type<'db>>,
    variable: Option<(usize, VariableSegment<'db>)>,
}

impl<'db> SynchronousTupleNormalizationEffects<'db> for OrdinaryNormalizationEffects<'db> {
    type Error = Infallible;
    type Buffer = NormalizedTupleElements<'db>;

    fn program(&self, env: &ProgramEnvironment<'db>) -> Result<Program<'db>, Infallible> {
        Ok(env.program(self.db))
    }

    fn spec(&self, tuple: TupleType<'db>) -> Result<&'db TupleSpec<'db>, Infallible> {
        Ok(tuple.tuple(self.db))
    }

    fn normalize_spec(
        &self,
        spec: &TupleSpec<'db>,
        env: &ProgramEnvironment<'db>,
        _program: Program<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<TupleSpec<'db>>, Infallible> {
        tuple_spec_normalize_sync(spec, env, divergent, nested, self, TupleNormalizationFacts)
    }

    fn normalize_child(
        &self,
        request: RecursiveNormalizationRequest<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        recursive_normalize_sync(request, env, self, RecursiveNormalizationFacts)
    }

    fn next_element(
        &self,
        elements: &mut slice::Iter<'_, Type<'db>>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(elements.next().copied())
    }

    fn new_buffer(&self, capacity: usize) -> Result<Self::Buffer, Infallible> {
        Ok(NormalizedTupleElements {
            elements: Vec::with_capacity(capacity),
            variable: None,
        })
    }

    fn push(&self, buffer: &mut Self::Buffer, ty: Type<'db>) -> Result<(), Infallible> {
        buffer.elements.push(ty);
        Ok(())
    }

    fn set_variable(
        &self,
        buffer: &mut Self::Buffer,
        variable: VariableSegment<'db>,
    ) -> Result<(), Infallible> {
        buffer.variable = Some((buffer.elements.len(), variable));
        Ok(())
    }

    fn finish_buffer(&self, buffer: &mut Self::Buffer) -> Result<TupleSpec<'db>, Infallible> {
        let elements = std::mem::take(&mut buffer.elements);
        Ok(match buffer.variable {
            None => fixed_spec(elements),
            Some((prefix_len, variable)) => variable_spec(elements, prefix_len, variable),
        })
    }

    fn intern_tuple(
        &self,
        program: Program<'db>,
        spec: TupleSpec<'db>,
    ) -> Result<TupleType<'db>, Infallible> {
        Ok(TupleType::new_internal(self.db, program, spec))
    }
}
