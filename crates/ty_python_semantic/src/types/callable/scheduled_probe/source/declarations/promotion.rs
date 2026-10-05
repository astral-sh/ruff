//! Promotion inputs captured from declaration-owned types before semantic evaluation.

use std::cell::RefCell;
use std::collections::VecDeque;

use rustc_hash::{FxHashMap, FxHashSet};

use super::{DeclarationKey, Fact, MemberInput, PreparationError, PreparedDeclarations};
use crate::types::callable::scheduled_probe::mapping::PromotionFactKey;
use crate::types::class::walk_generic_alias;
use crate::types::enums::is_single_member_enum;
use crate::types::generics::GenericContext;
use crate::types::literal::LiteralValueTypeKind;
use crate::types::typevar::TypeVarInstance;
use crate::types::visitor::{TypeCollector, TypeVisitor, walk_type_with_recursion_guard};
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, GenericAlias, KnownClass, Type, TypeVarVariance,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

#[derive(Default)]
pub(super) struct PromotionFacts<'db> {
    pub(super) variances: FxHashMap<BoundTypeVarInstance<'db>, Fact<TypeVarVariance>>,
    pub(super) enum_singletons: FxHashMap<ClassLiteral<'db>, Fact<bool>>,
    pub(super) scalar_fallbacks: FxHashMap<KnownClass, Fact<Type<'db>>>,
}

impl<'db> PromotionFacts<'db> {
    pub(super) fn prepare(
        declarations: &PreparedDeclarations<'db>,
    ) -> Result<Self, PreparationError<'db>> {
        PromotionPreparation {
            declarations,
            env: ProgramEnvironment::from_file(declarations.file),
            facts: Self::default(),
            origins: FxHashSet::default(),
            visited: TypeCollector::default(),
        }
        .prepare()
    }
}

struct PromotionPreparation<'a, 'db> {
    declarations: &'a PreparedDeclarations<'db>,
    env: ProgramEnvironment<'db>,
    facts: PromotionFacts<'db>,
    origins: FxHashSet<ClassLiteral<'db>>,
    // Each scan produces its facts before the next owner can reuse this visited set.
    // A failed capture or producer aborts preparation rather than retaining partial support.
    visited: TypeCollector<'db>,
}

impl<'db> PromotionPreparation<'_, 'db> {
    fn prepare(mut self) -> Result<PromotionFacts<'db>, PreparationError<'db>> {
        let declarations = self.declarations;
        for (name, global) in &declarations.globals {
            self.scan(
                DeclarationKey::Global(name.clone()),
                global,
                global.value.place.ignore_possibly_undefined(),
            )?;
        }
        for (class, facts) in &declarations.classes {
            self.context(&facts.context)?;
            for (name, member) in &facts.namespace {
                self.scan(
                    DeclarationKey::Namespace(*class, name.clone()),
                    member,
                    member.value.ignore_possibly_undefined(),
                )?;
            }
            for (name, inputs) in &facts.member_inputs {
                for (input, member) in [
                    (MemberInput::ImplicitClass, &inputs.implicit_class),
                    (MemberInput::OwnInstance, &inputs.own_instance),
                ] {
                    self.scan(
                        DeclarationKey::MemberInput(*class, name.clone(), input),
                        member,
                        member.value.ignore_possibly_undefined(),
                    )?;
                }
            }
        }
        for (function, signature) in &declarations.signatures {
            self.scan(
                DeclarationKey::Signature(*function),
                signature,
                Some(Type::Callable(signature.value)),
            )?;
        }

        // These are the complete scalar fallback inputs of regular literal promotion.
        // Preparing them once also covers literals supplied by later specialization.
        for known in [
            KnownClass::Str,
            KnownClass::Bool,
            KnownClass::Int,
            KnownClass::Bytes,
        ] {
            let key = DeclarationKey::Promotion(PromotionFactKey::ScalarFallback(known));
            let fallback = Fact::read(declarations.db, key.clone(), || {
                known.to_instance(declarations.db, &self.env)
            })?;
            self.scan(key, &fallback, Some(fallback.value))?;
            self.facts.scalar_fallbacks.insert(known, fallback);
        }
        Ok(self.facts)
    }

    fn scan<T>(
        &mut self,
        key: DeclarationKey<'db>,
        owner: &Fact<T>,
        ty: Option<Type<'db>>,
    ) -> Result<(), PreparationError<'db>> {
        let Some(ty) = ty else {
            return Ok(());
        };
        let declarations = self.declarations;
        let db = declarations.db;
        let inputs = owner.derive(db, key, || {
            StoredInputScanner::collect(db, &self.env, ty, &self.visited)
        })?;
        for variable in &inputs.value.variables {
            self.variance(&inputs, *variable)?;
        }
        for class in &inputs.value.origins {
            if !self.origins.insert(*class) {
                continue;
            }
            if let ClassLiteral::Static(class) = class {
                if let Some(facts) = declarations.classes.get(class) {
                    self.context(&facts.context)?;
                } else {
                    let context = inputs.derive(db, DeclarationKey::Context(*class), || {
                        class.generic_context(db)
                    })?;
                    self.context(&context)?;
                }
            }
            if class.known(db).is_none() {
                let singleton = inputs.derive(
                    db,
                    DeclarationKey::Promotion(PromotionFactKey::EnumSingleton(*class)),
                    || is_single_member_enum(db, *class),
                )?;
                self.facts.enum_singletons.insert(*class, singleton);
            }
        }
        Ok(())
    }

    fn context(
        &mut self,
        owner: &Fact<Option<GenericContext<'db>>>,
    ) -> Result<(), PreparationError<'db>> {
        if let Some(context) = owner.value {
            for variable in context.variables(self.declarations.db) {
                self.variance(owner, variable)?;
            }
        }
        Ok(())
    }

    fn variance<T>(
        &mut self,
        owner: &Fact<T>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<(), PreparationError<'db>> {
        if self.facts.variances.contains_key(&variable) {
            return Ok(());
        }
        let db = self.declarations.db;
        let variance = owner.derive(
            db,
            DeclarationKey::Promotion(PromotionFactKey::Variance(variable)),
            || variable.variance(db),
        )?;
        self.facts.variances.insert(variable, variance);
        Ok(())
    }
}

#[derive(Default)]
struct PromotionInputs<'db> {
    variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    origins: FxOrderSet<ClassLiteral<'db>>,
}

struct StoredInputScanner<'a, 'db> {
    env: &'a ProgramEnvironment<'db>,
    pending: RefCell<VecDeque<Type<'db>>>,
    visited: &'a TypeCollector<'db>,
    inputs: RefCell<PromotionInputs<'db>>,
}

impl<'db> StoredInputScanner<'_, 'db> {
    fn collect(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        root: Type<'db>,
        visited: &TypeCollector<'db>,
    ) -> PromotionInputs<'db> {
        let scanner = StoredInputScanner {
            env,
            pending: RefCell::new(VecDeque::from([root])),
            visited,
            inputs: RefCell::new(PromotionInputs::default()),
        };
        loop {
            let Some(ty) = scanner.pending.borrow_mut().pop_front() else {
                break;
            };
            match ty {
                Type::ClassLiteral(class) => {
                    scanner.inputs.borrow_mut().origins.insert(class);
                }
                Type::LiteralValue(literal) => {
                    if let LiteralValueTypeKind::Enum(literal) = literal.kind() {
                        scanner
                            .inputs
                            .borrow_mut()
                            .origins
                            .insert(literal.enum_class(db));
                    }
                }
                Type::TypeVar(variable) => scanner.visit_bound_type_var_type(db, variable),
                Type::NominalInstance(_)
                | Type::GenericAlias(_)
                | Type::SubclassOf(_)
                | Type::Callable(_)
                | Type::FunctionLiteral(_)
                | Type::Union(_)
                | Type::Intersection(_) => {
                    walk_type_with_recursion_guard(db, ty, &scanner, scanner.visited);
                }
                // Only stored payloads of the admitted containers are traversed. In particular,
                // aliases and recursive types cannot grow new specializations during preparation.
                _ => {}
            }
        }
        scanner.inputs.into_inner()
    }
}

impl<'db> TypeVisitor<'db> for StoredInputScanner<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }

    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_type(&self, _db: &'db dyn Db, ty: Type<'db>) {
        self.pending.borrow_mut().push_back(ty);
    }

    fn visit_generic_alias_type(&self, db: &'db dyn Db, alias: GenericAlias<'db>) {
        self.inputs
            .borrow_mut()
            .origins
            .insert(alias.origin(db).into());
        walk_generic_alias(db, alias, self);
    }

    fn visit_bound_type_var_type(&self, _db: &'db dyn Db, variable: BoundTypeVarInstance<'db>) {
        self.inputs.borrow_mut().variables.insert(variable);
    }

    fn visit_type_var_type(&self, _db: &'db dyn Db, _variable: TypeVarInstance<'db>) {
        // Promotion retains variables; their bounds and defaults are not mapping dependencies.
    }
}

impl<'db> PreparedDeclarations<'db> {
    pub(in crate::types::callable::scheduled_probe) fn promotion_variance(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarVariance, PromotionFactKey<'db>> {
        self.promotion
            .variances
            .get(&variable)
            .map(|fact| fact.value)
            .ok_or(PromotionFactKey::Variance(variable))
    }

    pub(in crate::types::callable::scheduled_probe) fn promotion_enum_singleton(
        &self,
        class: ClassLiteral<'db>,
    ) -> Result<bool, PromotionFactKey<'db>> {
        self.promotion
            .enum_singletons
            .get(&class)
            .map(|fact| fact.value)
            .ok_or(PromotionFactKey::EnumSingleton(class))
    }

    pub(in crate::types::callable::scheduled_probe) fn promotion_scalar_fallback(
        &self,
        known: KnownClass,
    ) -> Result<Type<'db>, PromotionFactKey<'db>> {
        self.promotion
            .scalar_fallbacks
            .get(&known)
            .map(|fact| fact.value)
            .ok_or(PromotionFactKey::ScalarFallback(known))
    }
}

#[cfg(test)]
mod tests;
