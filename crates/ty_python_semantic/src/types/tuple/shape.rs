//! Tuple constructors retain unpack operands until their finite shape is observed.
//!
//! An ordinary element guards the shape of a tuple, even when its type is recursive. An unpack
//! does not: `tuple[int, *Alias]` needs Alias's sequence before it can determine its own sequence.
//! Analyze those dependencies on declaration bodies before applying substitutions, so growing
//! applications cannot turn shape observation into an infinite sequence of larger tuples.

use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use ty_python_core::definition::Definition;

use super::{
    Tuple, TupleBuilder, TupleSpec, TupleSpecBuilder, TupleType, VariableLengthTuple,
    VariableSegment,
};
use crate::types::instance::NominalInstanceType;
use crate::types::set_theoretic::TypeNormalization;
use crate::types::visitor::{TypeCollector, TypeVisitor, walk_type_with_recursion_guard};
use crate::types::{BoundTypeVarInstance, Type, UnionBuilder};
use crate::{Db, FxOrderSet, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum TupleElementExpression<'db> {
    Element(Type<'db>),
    Unpack(Type<'db>),
    /// A specialized pack is either a precise tuple or its homogeneous element approximation.
    UnpackSpecialization(Type<'db>),
    /// A sequence recovered from an operand whose syntax has already been diagnosed.
    UnpackRecovery(Type<'db>),
}

impl<'db> TupleElementExpression<'db> {
    pub(super) fn ty(self) -> Type<'db> {
        match self {
            Self::Element(ty)
            | Self::Unpack(ty)
            | Self::UnpackSpecialization(ty)
            | Self::UnpackRecovery(ty) => ty,
        }
    }

    pub(super) fn with_type(self, ty: Type<'db>) -> Self {
        match self {
            Self::Element(_) => Self::Element(ty),
            Self::Unpack(_) => Self::Unpack(ty),
            Self::UnpackSpecialization(_) => Self::UnpackSpecialization(ty),
            Self::UnpackRecovery(_) => Self::UnpackRecovery(ty),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum TupleElements<'db> {
    Resolved(TupleSpec<'db>),
    Expression(Box<[TupleElementExpression<'db>]>),
}

impl<'db> From<TupleSpec<'db>> for TupleElements<'db> {
    fn from(value: TupleSpec<'db>) -> Self {
        Self::Resolved(value)
    }
}

impl<'db> From<&TupleSpec<'db>> for TupleElements<'db> {
    fn from(value: &TupleSpec<'db>) -> Self {
        Self::Resolved(value.clone())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(in crate::types) enum TupleShapeError {
    Recursive,
    NotTuple,
    MultipleVariadic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(in crate::types) enum TupleShapePosition {
    Element(usize),
    Suffix(usize),
    Variable,
}

/// A source occurrence contributing an observed tuple position. Unpack positions are relative
/// to that operand, so changing its length never renumbers the surrounding expression parts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(in crate::types) struct TupleShapePath {
    pub(in crate::types) part: Option<usize>,
    pub(in crate::types) position: Option<TupleShapePosition>,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct TupleElementObservation<'db> {
    pub(in crate::types) ty: Type<'db>,
    pub(in crate::types) source: TupleShapePath,
}

/// Annotation locations refer to the stored parts of the tuple whose shape was observed.
#[derive(Clone)]
pub(in crate::types) struct TupleShapeDiagnostic {
    pub(in crate::types) error: TupleShapeError,
    pub(in crate::types) multiple_variadic_parts: Box<[(usize, usize)]>,
}

#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct TupleShapeObservation<'db> {
    pub(super) tuple: TupleSpec<'db>,
    pub(super) error: Option<TupleShapeError>,
    multiple_variadic_parts: Box<[(usize, usize)]>,
    origins: Tuple<Option<TupleShapePath>>,
}

impl<'db> TupleShapeObservation<'db> {
    pub(super) fn element(
        &self,
        position: TupleShapePosition,
    ) -> Option<TupleElementObservation<'db>> {
        let ty = match (&self.tuple, position) {
            (Tuple::Fixed(tuple), TupleShapePosition::Element(index)) => {
                *tuple.elements_slice().get(index)?
            }
            (Tuple::Fixed(tuple), TupleShapePosition::Suffix(index)) => {
                *tuple.elements_slice().iter().rev().nth(index)?
            }
            (Tuple::Variable(tuple), TupleShapePosition::Element(index)) => {
                *tuple.prefix_elements().get(index)?
            }
            (Tuple::Variable(tuple), TupleShapePosition::Suffix(index)) => {
                *tuple.suffix_elements().iter().rev().nth(index)?
            }
            (Tuple::Variable(tuple), TupleShapePosition::Variable) => {
                tuple.variable().tuple_class_type()
            }
            _ => return None,
        };
        let source = match (&self.origins, position) {
            (Tuple::Fixed(tuple), TupleShapePosition::Element(index)) => {
                *tuple.elements_slice().get(index)?
            }
            (Tuple::Fixed(tuple), TupleShapePosition::Suffix(index)) => {
                *tuple.elements_slice().iter().rev().nth(index)?
            }
            (Tuple::Variable(tuple), TupleShapePosition::Element(index)) => {
                *tuple.prefix_elements().get(index)?
            }
            (Tuple::Variable(tuple), TupleShapePosition::Suffix(index)) => {
                *tuple.suffix_elements().iter().rev().nth(index)?
            }
            (Tuple::Variable(tuple), TupleShapePosition::Variable) => tuple.variable(),
            _ => return None,
        }?;
        Some(TupleElementObservation { ty, source })
    }
}

fn source_paths(tuple: &TupleSpec<'_>, part: Option<usize>) -> Tuple<Option<TupleShapePath>> {
    let path = |position| {
        Some(TupleShapePath {
            part,
            position: Some(position),
        })
    };
    match tuple {
        Tuple::Fixed(tuple) => Tuple::heterogeneous(
            (0..tuple.elements_slice().len()).map(|index| path(TupleShapePosition::Element(index))),
        ),
        Tuple::Variable(tuple) => VariableLengthTuple::mixed(
            (0..tuple.prefix_elements().len())
                .map(|index| path(TupleShapePosition::Element(index))),
            path(TupleShapePosition::Variable),
            (0..tuple.suffix_elements().len())
                .rev()
                .map(|index| path(TupleShapePosition::Suffix(index))),
        ),
    }
}

impl TupleShapeObservation<'_> {
    pub(super) fn diagnostic(&self) -> Option<TupleShapeDiagnostic> {
        Some(TupleShapeDiagnostic {
            error: self.error?,
            multiple_variadic_parts: self.multiple_variadic_parts.clone(),
        })
    }
}

/// The parameters that contribute unpacked sequences, rather than ordinary element types.
#[derive(Clone, Default)]
struct ShapeDependencies<'db> {
    parameters: FxOrderSet<BoundTypeVarInstance<'db>>,
    error: Option<TupleShapeError>,
}

struct ShapeObserver<'db> {
    env: ProgramEnvironment<'db>,
    active: FxHashSet<Definition<'db>>,
    declarations: FxHashMap<Definition<'db>, ShapeDependencies<'db>>,
    declaration_body: Option<(Definition<'db>, Type<'db>)>,
}

pub(super) fn observe<'db>(db: &'db dyn Db, tuple: TupleType<'db>) -> TupleShapeObservation<'db> {
    ShapeObserver {
        env: ProgramEnvironment::from_program(tuple.program(db)),
        active: FxHashSet::default(),
        declarations: FxHashMap::default(),
        declaration_body: None,
    }
    .tuple(db, tuple)
}

pub(in crate::types) fn alias_shape_diagnostic<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    definition: Definition<'db>,
    body: Type<'db>,
) -> Option<TupleShapeDiagnostic> {
    struct Validator<'a, 'db> {
        env: &'a ProgramEnvironment<'db>,
        definition: Definition<'db>,
        body: Type<'db>,
        diagnostic: RefCell<Option<TupleShapeDiagnostic>>,
        visited: TypeCollector<'db>,
    }
    impl<'db> TypeVisitor<'db> for Validator<'_, 'db> {
        fn program_environment(&self) -> &ProgramEnvironment<'db> {
            self.env
        }
        fn should_visit_lazy_type_attributes(&self) -> bool {
            false
        }
        fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
            // Constructor bodies still contain open references. Only an unpack edge asks for
            // their sequence shape; an ordinary recursive element does not affect outer arity.
            if self.diagnostic.borrow().is_some() || matches!(ty, Type::RecursiveVar(_)) {
                return;
            }
            if let Some(tuple) = ty
                .as_nominal_instance()
                .and_then(NominalInstanceType::own_tuple_type)
            {
                let observed = ShapeObserver {
                    env: self.env.clone(),
                    active: FxHashSet::default(),
                    declarations: FxHashMap::default(),
                    declaration_body: Some((self.definition, self.body)),
                }
                .tuple(db, tuple);
                let mut diagnostic = observed.diagnostic();
                // A nested constructor's part indices do not describe the outer annotation.
                if ty != self.body
                    && let Some(diagnostic) = &mut diagnostic
                {
                    diagnostic.multiple_variadic_parts = Box::new([]);
                }
                *self.diagnostic.borrow_mut() = diagnostic;
            }
            walk_type_with_recursion_guard(db, ty, self, &self.visited);
        }
    }
    let validator = Validator {
        env,
        definition,
        body,
        diagnostic: RefCell::new(None),
        visited: TypeCollector::default(),
    };
    validator.visit_type(db, body);
    validator.diagnostic.into_inner()
}

impl<'db> ShapeObserver<'db> {
    fn dependencies(&mut self, db: &'db dyn Db, ty: Type<'db>) -> ShapeDependencies<'db> {
        let mut result = ShapeDependencies::default();
        match ty {
            Type::TypeAlias(alias) => {
                let definition = alias.definition(db);
                let body = self.declaration(db, definition, |db| alias.raw_value_type(db));
                result.error = body.error;
                for parameter in body.parameters {
                    let mapped = alias.apply_to_node_structural(db, Type::TypeVar(parameter));
                    if mapped == Type::TypeVar(parameter) {
                        result.parameters.insert(parameter);
                    } else {
                        let dependency = self.dependencies(db, mapped);
                        result.error = result.error.or(dependency.error);
                        result.parameters.extend(dependency.parameters);
                    }
                }
            }
            Type::Recursive(recursive) => {
                let definition = recursive.definition(db);
                let body = self.declaration(db, definition, |db| {
                    recursive.observation_body(db).unwrap_or(Type::object())
                });
                result.error = body.error;
                for parameter in body.parameters {
                    let mapped = recursive.apply_to_node_structural(db, Type::TypeVar(parameter));
                    if mapped == Type::TypeVar(parameter) {
                        result.parameters.insert(parameter);
                    } else {
                        let dependency = self.dependencies(db, mapped);
                        result.error = result.error.or(dependency.error);
                        result.parameters.extend(dependency.parameters);
                    }
                }
            }
            Type::RecursiveVar(_) | Type::Divergent(_) => {
                result.error = Some(TupleShapeError::Recursive);
            }
            Type::TypeVar(variable) => {
                result.parameters.insert(variable);
            }
            Type::NominalInstance(instance) if let Some(tuple) = instance.own_tuple_type() => {
                if let TupleElements::Expression(elements) = tuple.elements(db) {
                    for element in elements {
                        if let TupleElementExpression::Unpack(ty)
                        | TupleElementExpression::UnpackSpecialization(ty) = element
                        {
                            let mut dependency = self.dependencies(db, *ty);
                            if matches!(element, TupleElementExpression::UnpackSpecialization(_))
                                && dependency.error == Some(TupleShapeError::NotTuple)
                            {
                                dependency.error = None;
                            }
                            result.error = result.error.or(dependency.error);
                            result.parameters.extend(dependency.parameters);
                        }
                    }
                } else if let TupleElements::Resolved(Tuple::Variable(tuple)) = tuple.elements(db)
                    && let VariableSegment::TypeVarTuple(typevar) = tuple.variable()
                {
                    result.parameters.insert(typevar);
                }
            }
            _ => result.error = Some(TupleShapeError::NotTuple),
        }
        result
    }

    fn declaration(
        &mut self,
        db: &'db dyn Db,
        definition: Definition<'db>,
        body: impl FnOnce(&'db dyn Db) -> Type<'db>,
    ) -> ShapeDependencies<'db> {
        if let Some(result) = self.declarations.get(&definition) {
            return result.clone();
        }
        if !self.active.insert(definition) {
            return ShapeDependencies {
                error: Some(TupleShapeError::Recursive),
                ..ShapeDependencies::default()
            };
        }
        let body = match self.declaration_body {
            Some((current, value)) if current == definition => value,
            _ => body(db),
        };
        let result = self.dependencies(db, body);
        self.active.remove(&definition);
        self.declarations.insert(definition, result.clone());
        result
    }

    fn unpack(
        &mut self,
        db: &'db dyn Db,
        ty: Type<'db>,
    ) -> Result<TupleSpec<'db>, TupleShapeError> {
        if let Some(error) = self.dependencies(db, ty).error {
            return Err(error);
        }
        match ty {
            Type::TypeAlias(alias) => {
                let body = alias.apply_to_node_structural(db, alias.raw_value_type(db));
                self.unpack(db, body)
            }
            Type::Recursive(recursive) => {
                let body = recursive
                    .observation_body(db)
                    .ok_or(TupleShapeError::NotTuple)?;
                self.unpack(db, recursive.apply_to_node_structural(db, body))
            }
            Type::NominalInstance(instance) if let Some(tuple) = instance.own_tuple_type() => {
                let observed = self.tuple(db, tuple);
                match observed.error {
                    Some(error) => Err(error),
                    None => Ok(observed.tuple),
                }
            }
            Type::TypeVar(typevar) if typevar.is_typevartuple(db) => Ok(
                VariableLengthTuple::mixed([], VariableSegment::TypeVarTuple(typevar), []),
            ),
            _ => Err(TupleShapeError::NotTuple),
        }
    }

    fn tuple(&mut self, db: &'db dyn Db, tuple: TupleType<'db>) -> TupleShapeObservation<'db> {
        let TupleElements::Expression(elements) = tuple.elements(db) else {
            let tuple = tuple.tuple(db).clone();
            let origins = source_paths(&tuple, None);
            return TupleShapeObservation {
                tuple,
                error: None,
                multiple_variadic_parts: Box::new([]),
                origins,
            };
        };
        let mut builder = TupleSpecBuilder::with_capacity(elements.len());
        let mut origins = TupleBuilder::with_capacity(elements.len());
        let mut error = None;
        let mut first_variadic = None;
        let mut multiple_variadic_parts = Vec::new();
        for (part, element) in elements.iter().enumerate() {
            match element {
                TupleElementExpression::Element(ty) => {
                    builder.push(*ty);
                    origins.push(Some(TupleShapePath {
                        part: Some(part),
                        position: None,
                    }));
                }
                TupleElementExpression::Unpack(ty)
                | TupleElementExpression::UnpackSpecialization(ty)
                | TupleElementExpression::UnpackRecovery(ty) => {
                    let (unpacked, unpacked_origins) = match self.unpack(db, *ty) {
                        Ok(unpacked) => {
                            let origins =
                                if matches!(element, TupleElementExpression::UnpackRecovery(_)) {
                                    VariableLengthTuple::mixed([], None, [])
                                } else {
                                    source_paths(&unpacked, Some(part))
                                };
                            (unpacked, origins)
                        }
                        Err(TupleShapeError::NotTuple)
                            if matches!(
                                element,
                                TupleElementExpression::UnpackSpecialization(_)
                            ) =>
                        {
                            let unpacked =
                                TupleType::homogeneous(db, &self.env, *ty).tuple(db).clone();
                            let origins = if *ty == Type::Never {
                                Tuple::Fixed(super::FixedLengthTuple::empty())
                            } else {
                                VariableLengthTuple::mixed(
                                    [],
                                    Some(TupleShapePath {
                                        part: Some(part),
                                        position: None,
                                    }),
                                    [],
                                )
                            };
                            (unpacked, origins)
                        }
                        Err(invalid) => {
                            error = error.or(Some(invalid));
                            // Recovery has no source position within an invalid sequence. Following
                            // that fictitious position would re-enter the same recursive unpack.
                            (
                                TupleSpec::homogeneous(Type::unknown()),
                                VariableLengthTuple::mixed([], None, []),
                            )
                        }
                    };
                    if unpacked.is_variadic()
                        && matches!(
                            element,
                            TupleElementExpression::Unpack(_)
                                | TupleElementExpression::UnpackSpecialization(_)
                        )
                    {
                        if let Some(first) = first_variadic {
                            error = error.or(Some(TupleShapeError::MultipleVariadic));
                            multiple_variadic_parts.push((first, part));
                        } else {
                            first_variadic = Some(part);
                        }
                    }
                    builder = builder.concat_with(&unpacked, |suffix, left, right, prefix| {
                        let mut union = UnionBuilder::new(db, &self.env)
                            .normalization(TypeNormalization::Structural);
                        for ty in suffix
                            .iter()
                            .copied()
                            .chain([left.element_type(db), right.element_type(db)])
                            .chain(prefix.iter().copied())
                        {
                            union.add_in_place(ty);
                        }
                        *left = VariableSegment::Homogeneous(union.build());
                    });
                    origins = origins.concat_with(&unpacked_origins, |_, variable, _, _| {
                        // A recovery segment merged from multiple packs has no single source.
                        *variable = None;
                    });
                }
            }
        }
        TupleShapeObservation {
            tuple: builder.build(),
            error,
            multiple_variadic_parts: multiple_variadic_parts.into_boxed_slice(),
            origins: origins.build(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TupleElementExpression, TupleShapePath, TupleShapePosition};
    use crate::db::tests::setup_db;
    use crate::types::Type;
    use crate::types::tuple::TupleType;

    #[test]
    fn changing_pack_length_preserves_equal_element_occurrences() {
        let db = setup_db();
        let env = db.program_environment();
        for length in [1, 2] {
            let pack = Type::tuple(TupleType::heterogeneous(
                &db,
                &env,
                std::iter::repeat_n(Type::object(), length),
            ));
            let tuple = TupleType::from_element_expressions(
                &db,
                &env,
                vec![
                    TupleElementExpression::Element(Type::object()),
                    TupleElementExpression::Unpack(pack),
                    TupleElementExpression::Element(Type::object()),
                ],
            );
            let head = tuple
                .observe_element(&db, TupleShapePosition::Element(0))
                .unwrap();
            let tail = tuple
                .observe_element(&db, TupleShapePosition::Element(length + 1))
                .unwrap();
            assert_eq!(head.ty, tail.ty);
            assert_eq!(
                head.source,
                TupleShapePath {
                    part: Some(0),
                    position: None
                }
            );
            assert_eq!(
                tail.source,
                TupleShapePath {
                    part: Some(2),
                    position: None
                }
            );
            for index in 0..length {
                let element = tuple
                    .observe_element(&db, TupleShapePosition::Element(index + 1))
                    .unwrap();
                assert_eq!(element.ty, head.ty);
                assert_eq!(
                    element.source,
                    TupleShapePath {
                        part: Some(1),
                        position: Some(TupleShapePosition::Element(index)),
                    }
                );
            }
        }
    }
}
