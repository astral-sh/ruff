use itertools::{Either, EitherOrBoth, Itertools};
use rustc_hash::{FxHashMap, FxHashSet};
use ty_python_core::{definition::Definition, scope::ScopeId};

use super::{
    ApplyTypeMappingVisitor, GenericAlias, InferenceFlags, InvalidTypeExpressionError, KnownClass,
    KnownInstanceType, Type, TypeContext, TypeMapping, UnionBuilder, visitor::TypeVisitor,
};
use crate::{Db, FxOrderSet, Program, ProgramEnvironment};

/// The type of a single Python union object, which can be used as a value or as an annotation.
/// For example, `bool | int` retains both classes as runtime operands and denotes `int` as an
/// annotation.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct UnionTypeInstance<'db> {
    #[returns(ref)]
    kind: RuntimeUnion<'db>,
}

impl get_size2::GetSize for UnionTypeInstance<'_> {}

/// How a runtime union represents the type it denotes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum RuntimeUnion<'db> {
    /// A union built with `|` that stores its operands and computes its denoted type on demand.
    /// In `(A | B) | C`, the outer union can reuse `A | B` without first normalizing it.
    Pep604(Box<[UnionOperand<'db>]>),
    /// A union with a known denoted type and, when available, its original runtime operands.
    /// ty infers the arguments of `typing.Union[A, B]` and `typing.Optional[A]` as type expressions.
    /// Specializing a union can change its denoted type while preserving the operands used by
    /// `isinstance`.
    Evaluated {
        values: Option<Box<[Type<'db>]>>,
        ty: Type<'db>,
    },
}

/// An operand of a runtime `|` union.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum UnionOperand<'db> {
    /// An existing union object used as an operand, such as `A | B` in `(A | B) | C`.
    Nested(UnionTypeInstance<'db>),
    /// An individual operand with its value type and the result of interpreting it as a type
    /// expression. For example, `T` in `Alias = T | int` retains its binding to `Alias` when
    /// another union uses `Alias` as an operand.
    Leaf {
        value: Type<'db>,
        ty: Result<Type<'db>, Box<InvalidTypeExpressionError<'db>>>,
    },
}

impl<'db> UnionOperand<'db> {
    fn value(&self) -> Type<'db> {
        match self {
            Self::Nested(union) => union.into_type(),
            Self::Leaf { value, .. } => *value,
        }
    }

    fn denoted(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
        match self {
            Self::Nested(union) => union.union_type(db, env),
            Self::Leaf { ty, .. } => ty.as_ref().copied().map_err(|error| (**error).clone()),
        }
    }
}

/// Returns the runtime union operand for `value`, replacing `None` with the `NoneType` class.
fn normalize_none_operand<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    value: Type<'db>,
) -> Type<'db> {
    if value.is_none(db) {
        KnownClass::NoneType.to_class_literal(db, env)
    } else {
        value
    }
}

/// Returns whether ty treats two generic aliases as the same operand of a runtime union.
///
/// We approximate runtime equality with type equivalence, so `list[int | str]` matches
/// `list[str | int]`.
fn same_generic_operand<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: GenericAlias<'db>,
    right: GenericAlias<'db>,
) -> bool {
    Type::GenericAlias(left).is_equivalent_to(db, env, Type::GenericAlias(right))
}

#[salsa::tracked]
impl<'db> UnionTypeInstance<'db> {
    fn into_type(self) -> Type<'db> {
        Type::KnownInstance(KnownInstanceType::UnionType(self))
    }

    pub(super) fn from_type_expression(db: &'db dyn Db, ty: Type<'db>) -> Self {
        Self::new_internal(db, RuntimeUnion::Evaluated { values: None, ty })
    }

    pub(crate) fn from_value_expression_types(
        db: &'db dyn Db,
        values: [Type<'db>; 2],
        scope_id: ScopeId<'db>,
        typevar_binding_context: Option<Definition<'db>>,
        inference_flags: InferenceFlags,
    ) -> Type<'db> {
        let [left, right] = values;
        let env = ProgramEnvironment::from_scope(scope_id);
        let is_union = |ty| matches!(ty, Type::KnownInstance(KnownInstanceType::UnionType(_)));
        // Equal class objects and generic aliases collapse to the operand itself. Once
        // either side is a union object, extending it only needs the two operand handles.
        if left == right
            || (!is_union(left) && !is_union(right) && left.is_equivalent_to(db, &env, right))
        {
            return left;
        }
        let operands = values.map(|value| match value {
            Type::KnownInstance(KnownInstanceType::UnionType(union)) => UnionOperand::Nested(union),
            value => UnionOperand::Leaf {
                value,
                ty: value
                    .in_type_expression_impl(db, scope_id, typevar_binding_context, inference_flags)
                    .map_err(Box::new),
            },
        });
        Self::new_internal(db, RuntimeUnion::Pep604(Box::new(operands))).into_type()
    }

    /// Returns the type this union denotes, or the first error from interpreting an operand
    /// as a type expression.
    pub(super) fn union_type(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
        match self.kind(db) {
            RuntimeUnion::Evaluated { ty, .. } => Ok(*ty),
            RuntimeUnion::Pep604(_) => self.normalized_union(db, env.program(db)).clone(),
        }
    }

    /// Flattens a `|` union for value comparisons, retaining the first occurrence of each exact
    /// operand. Operands with different type variable bindings remain distinct. Evaluated unions
    /// keep their stored representation.
    #[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
    fn canonical_value(self, db: &'db dyn Db) -> Self {
        let RuntimeUnion::Pep604(operands) = self.kind(db) else {
            return self;
        };
        let unique: FxOrderSet<_> = Self::leaves(db, operands).cloned().collect();
        Self::new_internal(db, RuntimeUnion::Pep604(unique.into_iter().collect()))
    }

    /// Returns whether ty can treat the two unions as the same value in type relations.
    /// The comparison accounts for the order of distinct operands and the types the unions denote.
    pub(super) fn same_value(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Self,
    ) -> bool {
        let left = self.canonical_value(db);
        let right = other.canonical_value(db);
        if left == right {
            return true;
        }

        // A specialized or legacy union retains a denotation for the whole object. Only
        // compare through these objects when the same ones appear in both expressions.
        let opaque = |union: Self| match union.kind(db) {
            RuntimeUnion::Evaluated { .. } => Either::Left(std::iter::once(union)),
            RuntimeUnion::Pep604(operands) => {
                Either::Right(operands.iter().filter_map(|operand| match operand {
                    UnionOperand::Nested(union) => Some(*union),
                    UnionOperand::Leaf { .. } => None,
                }))
            }
        };
        if !opaque(left).eq(opaque(right)) {
            return false;
        }
        // Extending `Union[int, str]` with `int`, or `list[int | str] | bytes` with
        // `list[str | int]`, preserves both the runtime operands and their denotation.
        // Comparing the denotations alone would also discard `bool` from `bool | int`.
        left.value_expression_types(db, env)
            .zip_longest(right.value_expression_types(db, env))
            .all(|pair| match pair {
                EitherOrBoth::Both(Type::GenericAlias(a), Type::GenericAlias(b)) => {
                    same_generic_operand(db, env, a, b)
                }
                EitherOrBoth::Both(a, b) => a == b,
                EitherOrBoth::Left(_) | EitherOrBoth::Right(_) => false,
            })
            && left.union_type(db, env) == right.union_type(db, env)
    }

    /// Returns whether the operands prove that the unions must be distinct runtime objects.
    pub(super) fn is_disjoint_from(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Self,
    ) -> bool {
        if self == other {
            return false;
        }

        // Classes with metaclass `type` compare by identity, so different sets of these classes
        // imply different unions. For other operands, `typing` can reuse a union object even
        // when ty inferred different representations for the two expressions.
        let classes = |union: Self| {
            let RuntimeUnion::Pep604(operands) = union.kind(db) else {
                return None;
            };
            Self::leaves(db, operands)
                .map(|operand| {
                    let Type::ClassLiteral(class) = normalize_none_operand(db, env, operand.value())
                    else {
                        return None;
                    };
                    matches!(class.metaclass(db), Type::ClassLiteral(meta) if meta.is_known(db, KnownClass::Type))
                        .then_some(class)
                })
                .collect::<Option<FxHashSet<_>>>()
        };
        let Some(left) = classes(self) else {
            return false;
        };
        classes(other).is_some_and(|right| left != right)
    }

    /// Yields the operands of a `|` union from left to right, flattening nested `|` unions.
    /// An evaluated union stays a single operand because its stored type may have been specialized.
    ///
    /// Visiting each shared union once avoids exponential work when assignments reuse a prefix.
    fn leaves(
        db: &'db dyn Db,
        operands: &'db [UnionOperand<'db>],
    ) -> impl Iterator<Item = &'db UnionOperand<'db>> {
        let mut pending: Vec<_> = operands.iter().rev().collect();
        let mut seen = FxHashSet::default();
        std::iter::from_fn(move || {
            loop {
                let operand = pending.pop()?;
                if let UnionOperand::Nested(union) = operand {
                    if !seen.insert(*union) {
                        continue;
                    }
                    if let RuntimeUnion::Pep604(operands) = union.kind(db) {
                        pending.extend(operands.iter().rev());
                        continue;
                    }
                }
                return Some(operand);
            }
        })
    }

    #[salsa::tracked(
        returns(ref),
        cycle_initial=|_, id, _, _| Ok(Type::divergent(id)),
        cycle_fn=|db, cycle, previous: &Result<Type<'db>, _>, result: Result<Type<'db>, _>, _, program| {
            let env = ProgramEnvironment::from_program(program);
            result.map(|ty| match previous {
                Ok(prev) => ty.cycle_normalized(db, &env, *prev, cycle),
                Err(_) => ty.recursive_type_normalized(db, &env, cycle),
            })
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    fn normalized_union(
        self,
        db: &'db dyn Db,
        program: Program<'db>,
    ) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
        let env = ProgramEnvironment::from_program(program);
        self.build_union_type(db, &env, UnionBuilder::new(db, &env))
    }

    fn build_union_type(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut builder: UnionBuilder<'db>,
    ) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
        match self.kind(db) {
            RuntimeUnion::Pep604(operands) => {
                for operand in Self::leaves(db, operands) {
                    builder.add_in_place(operand.denoted(db, env)?);
                }
                Ok(builder.build())
            }
            RuntimeUnion::Evaluated { ty, .. } => Ok(*ty),
        }
    }

    /// Returns the types of this union's immediate operand values, if they were recorded.
    fn value_operands(
        self,
        db: &'db dyn Db,
    ) -> Option<impl DoubleEndedIterator<Item = Type<'db>> + 'db> {
        match self.kind(db) {
            RuntimeUnion::Pep604(operands) => {
                Some(Either::Left(operands.iter().map(UnionOperand::value)))
            }
            RuntimeUnion::Evaluated { values, .. } => {
                Some(Either::Right(values.as_deref()?.iter().copied()))
            }
        }
    }

    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        let map = |ty: Type<'db>| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        if type_mapping.is_structural()
            && let RuntimeUnion::Pep604(operands) = self.kind(db)
        {
            // Structural substitutions can introduce open recursive variables; a builder
            // must not compare them until the recursive type has been closed.
            let mut changed = false;
            let mapped = Self::leaves(db, operands)
                .map(|operand| {
                    let mapped = match operand {
                        UnionOperand::Nested(union) => UnionOperand::Nested(
                            union.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                        ),
                        UnionOperand::Leaf { value, ty } => UnionOperand::Leaf {
                            value: map(*value),
                            ty: ty.as_ref().map(|ty| map(*ty)).map_err(Clone::clone),
                        },
                    };
                    changed |= &mapped != operand;
                    mapped
                })
                .collect();
            return if changed {
                Self::new_internal(db, RuntimeUnion::Pep604(mapped))
            } else {
                self
            };
        }

        let Ok(ty) = self.union_type(db, visitor.env) else {
            return self;
        };
        let mapped = map(ty);
        if !type_mapping.is_structural() && mapped == ty {
            return self;
        }
        let values = self.value_operands(db).map(|values| {
            values
                .map(|value| {
                    if type_mapping.is_structural() {
                        map(value)
                    } else {
                        value
                    }
                })
                .collect()
        });
        Self::new_internal(db, RuntimeUnion::Evaluated { values, ty: mapped })
    }

    /// Yields the operand types used for runtime checks such as `isinstance`.
    /// Recorded operands appear in first-occurrence order, counting equivalent specializations
    /// of the same class once. For `typing.Union[…]`, only the denoted type is available, so we
    /// recover class objects from its members where possible.
    pub(crate) fn value_expression_types(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> impl Iterator<Item = Type<'db>> {
        let mut pending = vec![self.into_type()];
        let mut seen = FxHashSet::default();
        let mut aliases_by_origin = FxHashMap::default();
        std::iter::from_fn(move || {
            loop {
                let value = normalize_none_operand(db, env, pending.pop()?);
                if !seen.insert(value) {
                    continue;
                }
                let Type::KnownInstance(KnownInstanceType::UnionType(union)) = value else {
                    // Group aliases by origin to avoid comparing specializations of unrelated classes.
                    if let Type::GenericAlias(alias) = value {
                        let aliases: &mut Vec<_> =
                            aliases_by_origin.entry(alias.origin(db)).or_default();
                        if aliases
                            .iter()
                            .any(|other| same_generic_operand(db, env, alias, *other))
                        {
                            continue;
                        }
                        aliases.push(alias);
                    }
                    return Some(value);
                };
                if let Some(operands) = union.value_operands(db) {
                    pending.extend(operands.rev());
                } else if let RuntimeUnion::Evaluated { ty, .. } = union.kind(db) {
                    let to_class_literal = |ty: Type<'db>| {
                        ty.as_nominal_instance()
                            .and_then(|instance| instance.class(db, env).static_class_literal(db))
                            .map(|(lit, _)| Type::ClassLiteral(lit.into()))
                            .unwrap_or_else(Type::unknown)
                    };
                    match ty {
                        Type::Union(union) => pending.extend(
                            union
                                .elements(db)
                                .iter()
                                .rev()
                                .copied()
                                .map(to_class_literal),
                        ),
                        ty => pending.push(to_class_literal(*ty)),
                    }
                }
            }
        })
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let normalize = |ty: Type<'db>| {
            let normalized = ty.recursive_type_normalized_impl(db, env, div, nested);
            if nested {
                normalized
            } else {
                Some(normalized.unwrap_or(div))
            }
        };
        match self.kind(db) {
            RuntimeUnion::Evaluated { values, ty } => {
                let values = match values {
                    Some(values) => Some(
                        values
                            .iter()
                            .copied()
                            .map(normalize)
                            .collect::<Option<_>>()?,
                    ),
                    None => None,
                };
                Some(Self::new_internal(
                    db,
                    RuntimeUnion::Evaluated {
                        values,
                        ty: normalize(*ty)?,
                    },
                ))
            }
            RuntimeUnion::Pep604(operands) => {
                // A loop such as `alias |= B` can build a deeper tree on every iteration.
                // Canonicalize by operand identity during cycle recovery. Both the runtime
                // value and its bound type matter; semantic subtyping would discard `bool`
                // from `bool | int`.
                let mut leaves = FxOrderSet::default();
                for operand in Self::leaves(db, operands) {
                    leaves.insert(match operand {
                        UnionOperand::Leaf { value, ty } => UnionOperand::Leaf {
                            value: normalize(*value)?,
                            ty: match ty {
                                Ok(ty) => Ok(normalize(*ty)?),
                                Err(error) => Err(error.clone()),
                            },
                        },
                        UnionOperand::Nested(union) => UnionOperand::Nested(
                            union.recursive_type_normalized_impl(db, env, div, nested)?,
                        ),
                    });
                }
                let canonical =
                    Self::new_internal(db, RuntimeUnion::Pep604(leaves.into_iter().collect()));
                // A denoted union can drop a divergent member even if normalizing the raw
                // operands left that marker in place. Use the recovery builder here because
                // ordinary union simplification can introduce new query cycles.
                if let Ok(ty) =
                    self.build_union_type(db, env, UnionBuilder::new(db, env).cycle_recovery(true))
                    && let normalized = normalize(ty)?
                    && normalized != ty
                {
                    Some(Self::new_internal(
                        db,
                        RuntimeUnion::Evaluated {
                            values: canonical.value_operands(db).map(Iterator::collect),
                            ty: normalized,
                        },
                    ))
                } else {
                    Some(canonical)
                }
            }
        }
    }
}

pub(super) fn walk_union_type_instance<'db, V: TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    instance: UnionTypeInstance<'db>,
    visitor: &V,
) {
    match instance.kind(db) {
        RuntimeUnion::Evaluated { ty, .. } => visitor.visit_type(db, *ty),
        RuntimeUnion::Pep604(operands) => {
            for operand in UnionTypeInstance::leaves(db, operands) {
                match operand {
                    UnionOperand::Leaf { ty: Ok(ty), .. } => visitor.visit_type(db, *ty),
                    UnionOperand::Nested(union) => visitor.visit_type(db, union.into_type()),
                    UnionOperand::Leaf { ty: Err(_), .. } => {}
                }
            }
        }
    }
}
