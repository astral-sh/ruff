//! Tuple mapping preserves contextual fixed elements and variable-before-prefix traversal.

use std::borrow::Cow;
use std::convert::Infallible;
use std::slice;

use super::buffer::{fixed_spec, variable_spec};
use super::{Tuple, TupleLength, TupleSpec, TupleType, VariableSegment};
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, KnownClass, Type, TypeContext, TypeMapping,
};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) enum MappedTupleVariable<'db> {
    Segment(VariableSegment<'db>),
    Splice(Cow<'db, TupleSpec<'db>>),
}

enum TupleMappingParts<'a, 'db> {
    Fixed(&'a [Type<'db>]),
    Variable {
        prefix: &'a [Type<'db>],
        variable: VariableSegment<'db>,
        suffix: &'a [Type<'db>],
    },
}

#[derive(Clone, Copy)]
pub(in crate::types) struct TupleMappingFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTupleMappingEffects)]
    pub(in crate::types) trait TupleMappingEffects<'db> {
        type Error;
        type Buffer;
        type FixedContexts;

        #[operation(local)]
        async fn spec(&self, tuple: TupleType<'db>) -> Result<&'db TupleSpec<'db>, Self::Error>;
        #[operation(child)]
        async fn map_spec(&self, db: &'db dyn Db, spec: &TupleSpec<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<TupleSpec<'db>, Self::Error>;
        #[operation(child)]
        async fn fixed_contexts(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, tcx: TypeContext<'db>, len: usize) -> Result<Self::FixedContexts, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_fixed(&self, db: &'db dyn Db, elements: &mut slice::Iter<'_, Type<'db>>, contexts: &mut Self::FixedContexts) -> Result<Option<(Type<'db>, TypeContext<'db>)>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_element(&self, elements: &mut slice::Iter<'_, Type<'db>>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn new_buffer(&self, capacity: usize) -> Result<Self::Buffer, Self::Error>;
        #[operation(local)]
        async fn push(&self, buffer: &mut Self::Buffer, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn map_type(&self, db: &'db dyn Db, ty: Type<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn classify_variadic(&self, db: &'db dyn Db, original: BoundTypeVarInstance<'db>, mapped: Type<'db>) -> Result<MappedTupleVariable<'db>, Self::Error>;
        #[operation(child)]
        async fn start_variable(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, buffer: Self::Buffer, variable: MappedTupleVariable<'db>) -> Result<Self::Buffer, Self::Error>;
        #[operation(local)]
        async fn finish_buffer(&self, buffer: Self::Buffer) -> Result<TupleSpec<'db>, Self::Error>;
        #[operation(source)]
        async fn intern_tuple(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, spec: TupleSpec<'db>) -> Result<TupleType<'db>, Self::Error>;
        #[operation(source)]
        async fn intern_structural(&self, db: &'db dyn Db, original: TupleType<'db>, spec: TupleSpec<'db>) -> Result<TupleType<'db>, Self::Error>;
    }

    #[finite_capability]
    impl TupleMappingFacts {
        fn parts<'a, 'db>(&self, spec: &'a TupleSpec<'db>) -> TupleMappingParts<'a, 'db> {
            match spec {
                Tuple::Fixed(tuple) => TupleMappingParts::Fixed(tuple.all_elements()),
                Tuple::Variable(tuple) => TupleMappingParts::Variable {
                    prefix: tuple.prefix_elements(),
                    variable: tuple.variable(),
                    suffix: tuple.suffix_elements(),
                },
            }
        }
        fn elements<'a, 'db>(&self, types: &'a [Type<'db>]) -> slice::Iter<'a, Type<'db>> { types.iter() }
        fn len(&self, types: &[Type<'_>]) -> usize { types.len() }
        fn fixed_count(&self, spec: &TupleSpec<'_>) -> usize {
            match spec {
                Tuple::Fixed(tuple) => tuple.len(),
                Tuple::Variable(tuple) => tuple.fixed_elements.len(),
            }
        }
        fn environment<'a, 'db>(&self, visitor: &'a ApplyTypeMappingVisitor<'_, 'db>) -> &'a ProgramEnvironment<'db> { visitor.env }
        fn structural(&self, mapping: &TypeMapping<'_, '_>) -> bool { mapping.is_structural() }
    }

    #[synchronous(map_tuple_spec_sync)]
    #[capabilities(effects = TupleMappingEffects, facts = TupleMappingFacts)]
    #[passive_values(TupleMappingParts::Fixed, TupleMappingParts::Variable, VariableSegment::Homogeneous, VariableSegment::TypeVarTuple, MappedTupleVariable::Segment, Type::TypeVar)]
    pub(in crate::types) async fn map_tuple_spec_with<'db, E: TupleMappingEffects<'db>>(
        db: &'db dyn Db, spec: &TupleSpec<'db>, mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>, effects: &E, facts: TupleMappingFacts,
    ) -> Result<TupleSpec<'db>, E::Error> {
        let env = facts.environment(visitor);
        match facts.parts(spec) {
            TupleMappingParts::Fixed(types) => {
                let mut contexts = effects.fixed_contexts(db, env, tcx, facts.len(types)).await?;
                let mut buffer = effects.new_buffer(facts.len(types)).await?;
                let mut elements = facts.elements(types);
                #[cursor_loop]
                while let Some(next) = effects.next_fixed(db, &mut elements, &mut contexts).await? {
                    let (ty, context) = next;
                    let mapped = effects.map_type(db, ty, mapping, context, visitor).await?;
                    effects.push(&mut buffer, mapped).await?;
                }
                effects.finish_buffer(buffer).await
            }
            TupleMappingParts::Variable { prefix, variable, suffix } => {
                let variable = match variable {
                    VariableSegment::Homogeneous(ty) => {
                        let mapped = effects.map_type(db, ty, mapping, tcx, visitor).await?;
                        MappedTupleVariable::Segment(VariableSegment::Homogeneous(mapped))
                    }
                    VariableSegment::TypeVarTuple(original) => {
                        let mapped = effects.map_type(db, Type::TypeVar(original), mapping, tcx, visitor).await?;
                        effects.classify_variadic(db, original, mapped).await?
                    }
                };
                let mut buffer = effects.new_buffer(facts.fixed_count(spec)).await?;
                let mut prefix = facts.elements(prefix);
                #[cursor_loop]
                while let Some(ty) = effects.next_element(&mut prefix).await? {
                    let mapped = effects.map_type(db, ty, mapping, tcx, visitor).await?;
                    effects.push(&mut buffer, mapped).await?;
                }
                let mut buffer = effects.start_variable(db, env, buffer, variable).await?;
                let mut suffix = facts.elements(suffix);
                #[cursor_loop]
                while let Some(ty) = effects.next_element(&mut suffix).await? {
                    let mapped = effects.map_type(db, ty, mapping, tcx, visitor).await?;
                    effects.push(&mut buffer, mapped).await?;
                }
                effects.finish_buffer(buffer).await
            }
        }
    }

    #[synchronous(map_tuple_sync)]
    #[capabilities(effects = TupleMappingEffects, facts = TupleMappingFacts)]
    #[passive_values()]
    pub(in crate::types) async fn map_tuple_with<'db, E: TupleMappingEffects<'db>>(
        db: &'db dyn Db, tuple: TupleType<'db>, mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>, effects: &E, facts: TupleMappingFacts,
    ) -> Result<TupleType<'db>, E::Error> {
        let spec = effects.spec(tuple).await?;
        let spec = effects.map_spec(db, spec, mapping, tcx, visitor).await?;
        if facts.structural(mapping) {
            effects.intern_structural(db, tuple, spec).await
        } else {
            effects.intern_tuple(db, facts.environment(visitor), spec).await
        }
    }
}

pub(super) struct OrdinaryTupleMapping<'db> {
    pub(super) db: &'db dyn Db,
}

pub(super) struct FixedContexts<'db> {
    tuple: Option<TupleSpec<'db>>,
    index: usize,
}

pub(super) struct MappedElements<'db> {
    elements: Vec<Type<'db>>,
    variable: Option<(usize, VariableSegment<'db>)>,
}

impl<'db> SynchronousTupleMappingEffects<'db> for OrdinaryTupleMapping<'db> {
    type Error = Infallible;
    type Buffer = MappedElements<'db>;
    type FixedContexts = FixedContexts<'db>;

    fn spec(&self, tuple: TupleType<'db>) -> Result<&'db TupleSpec<'db>, Infallible> {
        Ok(tuple.tuple(self.db))
    }

    fn map_spec(
        &self,
        db: &'db dyn Db,
        spec: &TupleSpec<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<TupleSpec<'db>, Infallible> {
        map_tuple_spec_sync(db, spec, mapping, tcx, visitor, self, TupleMappingFacts)
    }

    fn fixed_contexts(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        tcx: TypeContext<'db>,
        len: usize,
    ) -> Result<Self::FixedContexts, Infallible> {
        let tuple = tcx
            .annotation
            .and_then(|annotation| annotation.known_specialization(db, env, KnownClass::Tuple))
            .and_then(|specialization| {
                specialization
                    .tuple(db)
                    .expect("the specialization of `KnownClass::Tuple` must have a tuple spec")
                    .resize(db, env, TupleLength::Fixed(len))
                    .ok()
            });
        Ok(FixedContexts { tuple, index: 0 })
    }

    fn next_fixed(
        &self,
        db: &'db dyn Db,
        elements: &mut slice::Iter<'_, Type<'db>>,
        contexts: &mut Self::FixedContexts,
    ) -> Result<Option<(Type<'db>, TypeContext<'db>)>, Infallible> {
        let Some(ty) = elements.next().copied() else {
            return Ok(None);
        };
        let context = match &contexts.tuple {
            None => TypeContext::default(),
            Some(tuple) => {
                let annotation = match tuple {
                    Tuple::Fixed(tuple) => tuple.all_elements().get(contexts.index).copied(),
                    Tuple::Variable(tuple) => tuple.iter_all_elements(db).nth(contexts.index),
                };
                let Some(annotation) = annotation else {
                    return Ok(None);
                };
                TypeContext::new(Some(annotation))
            }
        };
        contexts.index += 1;
        Ok(Some((ty, context)))
    }

    fn next_element(
        &self,
        elements: &mut slice::Iter<'_, Type<'db>>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(elements.next().copied())
    }
    fn new_buffer(&self, capacity: usize) -> Result<Self::Buffer, Infallible> {
        Ok(MappedElements {
            elements: Vec::with_capacity(capacity),
            variable: None,
        })
    }
    fn push(&self, buffer: &mut Self::Buffer, ty: Type<'db>) -> Result<(), Infallible> {
        buffer.elements.push(ty);
        Ok(())
    }
    fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(ty.apply_type_mapping_impl(db, mapping, tcx, visitor))
    }
    fn classify_variadic(
        &self,
        db: &'db dyn Db,
        original: BoundTypeVarInstance<'db>,
        mapped: Type<'db>,
    ) -> Result<MappedTupleVariable<'db>, Infallible> {
        Ok(if mapped == Type::TypeVar(original) {
            MappedTupleVariable::Segment(VariableSegment::TypeVarTuple(original))
        } else if let Type::TypeVar(variable) = mapped
            && variable.is_typevartuple(db)
        {
            MappedTupleVariable::Segment(VariableSegment::TypeVarTuple(variable))
        } else if let Some(tuple) = mapped.exact_tuple_instance_spec(db) {
            MappedTupleVariable::Splice(tuple)
        } else {
            MappedTupleVariable::Segment(VariableSegment::Homogeneous(mapped))
        })
    }
    fn start_variable(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        mut buffer: Self::Buffer,
        variable: MappedTupleVariable<'db>,
    ) -> Result<Self::Buffer, Infallible> {
        match variable {
            MappedTupleVariable::Segment(segment) => {
                buffer.variable = Some((buffer.elements.len(), segment));
            }
            // Only the fixed prefix has been mapped when the replacement tuple is inserted.
            // Its prefix and suffix therefore keep their positions around its variable segment.
            MappedTupleVariable::Splice(tuple) => match &*tuple {
                Tuple::Fixed(tuple) => buffer.elements.extend_from_slice(tuple.all_elements()),
                Tuple::Variable(tuple) => {
                    buffer.elements.extend_from_slice(tuple.prefix_elements());
                    buffer.variable = Some((buffer.elements.len(), tuple.variable()));
                    buffer.elements.extend_from_slice(tuple.suffix_elements());
                }
            },
        }
        Ok(buffer)
    }
    fn finish_buffer(&self, buffer: Self::Buffer) -> Result<TupleSpec<'db>, Infallible> {
        Ok(match buffer.variable {
            Some((prefix_len, variable)) => variable_spec(buffer.elements, prefix_len, variable),
            None => fixed_spec(buffer.elements),
        })
    }
    fn intern_tuple(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        spec: TupleSpec<'db>,
    ) -> Result<TupleType<'db>, Infallible> {
        Ok(TupleType::new(db, env, &spec))
    }
    fn intern_structural(
        &self,
        db: &'db dyn Db,
        original: TupleType<'db>,
        spec: TupleSpec<'db>,
    ) -> Result<TupleType<'db>, Infallible> {
        Ok(TupleType::new_internal(db, original.program(db), spec))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use ruff_python_ast::name::Name;

    use super::*;
    use crate::db::tests::setup_db;
    use crate::types::typevar::{
        BindingContext, TypeVarIdentity, TypeVarInstance, TypeVarKind, TypeVarNonce,
    };
    use crate::types::{MaterializationKind, TypeVarVariance};

    struct ObservedMapping<'db> {
        ordinary: OrdinaryTupleMapping<'db>,
        children: RefCell<Vec<(Type<'db>, Option<Type<'db>>)>>,
        replacement: Option<(Type<'db>, Type<'db>)>,
    }

    impl<'db> ObservedMapping<'db> {
        fn new(db: &'db dyn Db) -> Self {
            Self {
                ordinary: OrdinaryTupleMapping { db },
                children: RefCell::default(),
                replacement: None,
            }
        }
    }

    impl<'db> SynchronousTupleMappingEffects<'db> for ObservedMapping<'db> {
        type Error = Infallible;
        type Buffer = MappedElements<'db>;
        type FixedContexts = FixedContexts<'db>;

        fn spec(&self, tuple: TupleType<'db>) -> Result<&'db TupleSpec<'db>, Infallible> {
            self.ordinary.spec(tuple)
        }

        fn map_spec(
            &self,
            db: &'db dyn Db,
            spec: &TupleSpec<'db>,
            mapping: &TypeMapping<'_, 'db>,
            tcx: TypeContext<'db>,
            visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        ) -> Result<TupleSpec<'db>, Infallible> {
            map_tuple_spec_sync(db, spec, mapping, tcx, visitor, self, TupleMappingFacts)
        }

        fn fixed_contexts(
            &self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            tcx: TypeContext<'db>,
            len: usize,
        ) -> Result<Self::FixedContexts, Infallible> {
            self.ordinary.fixed_contexts(db, env, tcx, len)
        }

        fn next_fixed(
            &self,
            db: &'db dyn Db,
            elements: &mut slice::Iter<'_, Type<'db>>,
            contexts: &mut Self::FixedContexts,
        ) -> Result<Option<(Type<'db>, TypeContext<'db>)>, Infallible> {
            self.ordinary.next_fixed(db, elements, contexts)
        }

        fn next_element(
            &self,
            elements: &mut slice::Iter<'_, Type<'db>>,
        ) -> Result<Option<Type<'db>>, Infallible> {
            self.ordinary.next_element(elements)
        }

        fn new_buffer(&self, capacity: usize) -> Result<Self::Buffer, Infallible> {
            self.ordinary.new_buffer(capacity)
        }

        fn push(&self, buffer: &mut Self::Buffer, ty: Type<'db>) -> Result<(), Infallible> {
            self.ordinary.push(buffer, ty)
        }

        fn map_type(
            &self,
            _db: &'db dyn Db,
            ty: Type<'db>,
            _mapping: &TypeMapping<'_, 'db>,
            tcx: TypeContext<'db>,
            _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        ) -> Result<Type<'db>, Infallible> {
            self.children.borrow_mut().push((ty, tcx.annotation));
            Ok(match self.replacement {
                Some((original, replacement)) if original == ty => replacement,
                _ => ty,
            })
        }

        fn classify_variadic(
            &self,
            db: &'db dyn Db,
            original: BoundTypeVarInstance<'db>,
            mapped: Type<'db>,
        ) -> Result<MappedTupleVariable<'db>, Infallible> {
            self.ordinary.classify_variadic(db, original, mapped)
        }

        fn start_variable(
            &self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            buffer: Self::Buffer,
            variable: MappedTupleVariable<'db>,
        ) -> Result<Self::Buffer, Infallible> {
            self.ordinary.start_variable(db, env, buffer, variable)
        }

        fn finish_buffer(&self, buffer: Self::Buffer) -> Result<TupleSpec<'db>, Infallible> {
            self.ordinary.finish_buffer(buffer)
        }

        fn intern_tuple(
            &self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            spec: TupleSpec<'db>,
        ) -> Result<TupleType<'db>, Infallible> {
            self.ordinary.intern_tuple(db, env, spec)
        }

        fn intern_structural(
            &self,
            db: &'db dyn Db,
            original: TupleType<'db>,
            spec: TupleSpec<'db>,
        ) -> Result<TupleType<'db>, Infallible> {
            self.ordinary.intern_structural(db, original, spec)
        }
    }

    #[test]
    fn variable_segment_precedes_prefix_and_suffix_with_the_supplied_context() {
        let db = setup_db();
        let env = db.program_environment();
        let visitor = ApplyTypeMappingVisitor::new(&env);
        let elements = [
            Type::int_literal(1),
            Type::int_literal(2),
            Type::int_literal(3),
        ];
        let spec = variable_spec(
            elements.to_vec(),
            2,
            VariableSegment::Homogeneous(Type::any()),
        );
        let effects = ObservedMapping::new(&db);
        let annotation = Some(Type::object());
        let actual = map_tuple_spec_sync(
            &db,
            &spec,
            &TypeMapping::Materialize(MaterializationKind::Top),
            TypeContext::new(annotation),
            &visitor,
            &effects,
            TupleMappingFacts,
        )
        .unwrap();
        assert_eq!(actual, spec);
        assert_eq!(
            *effects.children.borrow(),
            [
                (Type::any(), annotation),
                (elements[0], annotation),
                (elements[1], annotation),
                (elements[2], annotation)
            ]
        );
    }

    #[test]
    fn fixed_elements_receive_resized_contexts_or_defaults() {
        let db = setup_db();
        let env = db.program_environment();
        let visitor = ApplyTypeMappingVisitor::new(&env);
        let elements = [Type::int_literal(1), Type::int_literal(2)];
        let spec = fixed_spec(elements.to_vec());
        let contextual = Type::tuple(TupleType::new(
            &db,
            &env,
            &variable_spec(
                vec![Type::bool_literal(true)],
                1,
                VariableSegment::Homogeneous(Type::object()),
            ),
        ));
        let incompatible = Type::tuple(TupleType::empty(&db, &env));
        for (annotation, contexts) in [
            (
                Some(contextual),
                [Some(Type::bool_literal(true)), Some(Type::object())],
            ),
            (Some(incompatible), [None, None]),
            (None, [None, None]),
        ] {
            let effects = ObservedMapping::new(&db);
            let actual = map_tuple_spec_sync(
                &db,
                &spec,
                &TypeMapping::Materialize(MaterializationKind::Top),
                TypeContext::new(annotation),
                &visitor,
                &effects,
                TupleMappingFacts,
            )
            .unwrap();
            assert_eq!(actual, spec);
            assert_eq!(
                *effects.children.borrow(),
                [(elements[0], contexts[0]), (elements[1], contexts[1])]
            );
        }
    }

    fn variadic<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &'static str,
    ) -> BoundTypeVarInstance<'db> {
        let identity = TypeVarIdentity::new(
            db,
            Name::new_static(name),
            None,
            TypeVarKind::Pep695TypeVarTuple,
        );
        let variable =
            TypeVarInstance::new(db, identity, None, Some(TypeVarVariance::Invariant), None);
        BoundTypeVarInstance::new(
            db,
            variable,
            BindingContext::Synthetic(env.program(db)),
            None,
            TypeVarNonce::default(),
        )
    }

    #[test]
    fn variadic_replacements_preserve_segments_and_splice_between_fixed_elements() {
        let db = setup_db();
        let env = db.program_environment();
        let visitor = ApplyTypeMappingVisitor::new(&env);
        let original = variadic(&db, &env, "Ts");
        let replacement = variadic(&db, &env, "Us");
        let prefix = Type::int_literal(1);
        let suffix = Type::int_literal(2);
        let inserted = Type::int_literal(3);
        let spec = variable_spec(
            vec![prefix, suffix],
            1,
            VariableSegment::TypeVarTuple(original),
        );
        let fixed = fixed_spec(vec![inserted]);
        let variable = variable_spec(
            vec![inserted, inserted],
            1,
            VariableSegment::Homogeneous(Type::object()),
        );
        for (mapped, expected) in [
            (Type::TypeVar(original), spec.clone()),
            (
                Type::TypeVar(replacement),
                variable_spec(
                    vec![prefix, suffix],
                    1,
                    VariableSegment::TypeVarTuple(replacement),
                ),
            ),
            (
                Type::tuple(TupleType::new(&db, &env, &fixed)),
                fixed_spec(vec![prefix, inserted, suffix]),
            ),
            (
                Type::tuple(TupleType::new(&db, &env, &variable)),
                variable_spec(
                    vec![prefix, inserted, inserted, suffix],
                    2,
                    VariableSegment::Homogeneous(Type::object()),
                ),
            ),
            (
                Type::any(),
                variable_spec(
                    vec![prefix, suffix],
                    1,
                    VariableSegment::Homogeneous(Type::any()),
                ),
            ),
        ] {
            let mut effects = ObservedMapping::new(&db);
            effects.replacement = Some((Type::TypeVar(original), mapped));
            let actual = map_tuple_spec_sync(
                &db,
                &spec,
                &TypeMapping::Materialize(MaterializationKind::Top),
                TypeContext::default(),
                &visitor,
                &effects,
                TupleMappingFacts,
            )
            .unwrap();
            assert_eq!(actual, expected);
            assert_eq!(
                *effects.children.borrow(),
                [
                    (Type::TypeVar(original), None),
                    (prefix, None),
                    (suffix, None)
                ]
            );
        }
    }
}
