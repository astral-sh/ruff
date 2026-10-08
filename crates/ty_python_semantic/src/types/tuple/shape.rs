//! Tuple constructors retain unpack operands until their finite shape is observed.
//!
//! An ordinary element guards the shape of a tuple, even when its type is recursive. An unpack
//! does not: `tuple[int, *Alias]` needs Alias's sequence before it can determine its own sequence.
//! Analyze those dependencies on declaration bodies before applying substitutions, so growing
//! applications cannot turn shape observation into an infinite sequence of larger tuples.

use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use ty_python_core::definition::Definition;

use super::{Tuple, TupleSpec, TupleSpecBuilder, TupleType, VariableLengthTuple, VariableSegment};
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
                let Some(body) = recursive.shape_body(db) else {
                    return ShapeDependencies { error: Some(TupleShapeError::NotTuple), ..ShapeDependencies::default() };
                };
                let body = self.declaration(db, definition, |_| body);
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
                let body = recursive.shape_body(db).ok_or(TupleShapeError::NotTuple)?;
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
            return TupleShapeObservation {
                tuple,
                error: None,
                multiple_variadic_parts: Box::new([]),
            };
        };
        let mut builder = TupleSpecBuilder::with_capacity(elements.len());
        let mut error = None;
        let mut first_variadic = None;
        let mut multiple_variadic_parts = Vec::new();
        for (part, element) in elements.iter().enumerate() {
            match element {
                TupleElementExpression::Element(ty) => {
                    builder.push(*ty);
                }
                TupleElementExpression::Unpack(ty)
                | TupleElementExpression::UnpackSpecialization(ty)
                | TupleElementExpression::UnpackRecovery(ty) => {
                    let unpacked = self.unpack(db, *ty).unwrap_or_else(|invalid| {
                        if invalid == TupleShapeError::NotTuple
                            && matches!(element, TupleElementExpression::UnpackSpecialization(_))
                        {
                            return TupleType::homogeneous(db, &self.env, *ty).tuple(db).clone();
                        }
                        error = error.or(Some(invalid));
                        // Invalid sequences retain a gradual recovery segment, separately from
                        // the error that prevents treating their shape as a successful result.
                        TupleSpec::homogeneous(Type::unknown())
                    });
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
                }
            }
        }
        TupleShapeObservation {
            tuple: builder.build(),
            error,
            multiple_variadic_parts: multiple_variadic_parts.into_boxed_slice(),
        }
    }
}
