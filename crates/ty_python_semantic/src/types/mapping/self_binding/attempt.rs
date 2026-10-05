//! Self ownership checks within an installed expansion attempt.

use std::future::{Future, ready};

use super::{SelfBindingEffects, SelfBindingWork, sealed};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::instance::NominalVisitorChildren;
use crate::types::source_read::{SourceReadControl, read_source};
use crate::types::typevar::{BindingContext, TypeVarBoundOrConstraints};
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, Type, class_mro_literals, self_typevar_owner_class_literal,
};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum UnsupportedSelfBindingOperation {
    UncontrolledMro,
    AliasOwner,
    RecursiveOwner,
    TupleOwner,
    BuiltinOwner,
    DynamicOwner,
    ProtocolOwner,
    NewTypeOwner,
    TypeVariableOwner,
    LiteralOwner,
    PropertyOwner,
    SlotDescriptorOwner,
    LazyUpperBound,
}

pub(in crate::types) struct AttemptSelfBindingEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> AttemptSelfBindingEffects<'db> {
    pub(in crate::types) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }

    #[cfg_attr(test, track_caller)]
    fn admit(&self) -> Result<(), Incomplete> {
        expansion_probe::charge_work(self.db, 1)
    }

    fn unsupported<T>(&self, operation: UnsupportedSelfBindingOperation) -> Result<T, Incomplete> {
        self.check()?;
        Err(expansion_probe::refuse(
            self.db,
            Incomplete::UnsupportedSelfBindingOperation(operation),
        ))
    }

    /// Admit only branches where `nominal_class` reads stored fields or returns no owner.
    /// Alias expansion, builtin lookup, and recursive fallback need their own effects before
    /// they can participate; treating them as absent owners would change Self matching.
    fn preflight_nominal_owner(&self, ty: Type<'db>) -> Result<(), Incomplete> {
        self.admit()?;
        match ty {
            Type::NominalInstance(instance) => match instance.children_for_visitor(self.db) {
                NominalVisitorChildren::Class(
                    Type::ClassLiteral(ClassLiteral::Static(_)) | Type::GenericAlias(_),
                ) => Ok(()),
                NominalVisitorChildren::Class(_) => {
                    self.unsupported(UnsupportedSelfBindingOperation::DynamicOwner)
                }
                NominalVisitorChildren::Tuple(_) => {
                    self.unsupported(UnsupportedSelfBindingOperation::TupleOwner)
                }
                NominalVisitorChildren::None => {
                    self.unsupported(UnsupportedSelfBindingOperation::BuiltinOwner)
                }
            },
            Type::TypeAlias(_) => self.unsupported(UnsupportedSelfBindingOperation::AliasOwner),
            Type::Recursive(_) | Type::RecursiveVar(_) => {
                self.unsupported(UnsupportedSelfBindingOperation::RecursiveOwner)
            }
            Type::ProtocolInstance(_) => {
                self.unsupported(UnsupportedSelfBindingOperation::ProtocolOwner)
            }
            Type::NewTypeInstance(_) => {
                self.unsupported(UnsupportedSelfBindingOperation::NewTypeOwner)
            }
            Type::TypeVar(_) => {
                self.unsupported(UnsupportedSelfBindingOperation::TypeVariableOwner)
            }
            Type::LiteralValue(_) => {
                self.unsupported(UnsupportedSelfBindingOperation::LiteralOwner)
            }
            Type::PropertyInstance(_) => {
                self.unsupported(UnsupportedSelfBindingOperation::PropertyOwner)
            }
            Type::SlotDescriptor(_) => {
                self.unsupported(UnsupportedSelfBindingOperation::SlotDescriptorOwner)
            }
            Type::Dynamic(_)
            | Type::Divergent(_)
            | Type::Never
            | Type::FunctionLiteral(_)
            | Type::BoundMethod(_)
            | Type::KnownBoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::Callable(_)
            | Type::ModuleLiteral(_)
            | Type::ClassLiteral(_)
            | Type::GenericAlias(_)
            | Type::SubclassOf(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::Union(_)
            | Type::Intersection(_)
            | Type::EnumComplement(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::BoundSuper(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
            | Type::TypedDict(_) => Ok(()),
        }
    }

    fn preflight_self_owner(
        &self,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<(), Incomplete> {
        self.admit()?;
        let (bounds, skipped_lazy) = variable
            .typevar(self.db)
            .bounds_for_visitor(self.db, env, false);
        if skipped_lazy {
            return self.unsupported(UnsupportedSelfBindingOperation::LazyUpperBound);
        }
        if let Some(TypeVarBoundOrConstraints::UpperBound(bound)) = bounds {
            self.preflight_nominal_owner(bound)?;
        }
        Ok(())
    }
}

impl SourceReadControl for AttemptSelfBindingEffects<'_> {
    type Error = Incomplete;

    fn check(&self) -> Result<(), Incomplete> {
        expansion_probe::continue_work(self.db)
    }
}

impl sealed::Sealed for AttemptSelfBindingEffects<'_> {}

impl<'db> SelfBindingEffects<'db> for AttemptSelfBindingEffects<'db> {
    type Error = Incomplete;

    fn checkpoint(&self, work: SelfBindingWork) -> impl Future<Output = Result<(), Incomplete>> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        ready(self.admit())
    }

    fn is_self(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(variable.typevar(db).is_self(db)))
    }

    fn binding_context(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<BindingContext<'db>, Self::Error>> {
        ready(Ok(variable.binding_context(db)))
    }

    fn nominal_owner(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<Option<ClassLiteral<'db>>, Incomplete>> {
        ready(self.preflight_nominal_owner(ty).map(|()| {
            ty.nominal_class(db, env)
                .map(|class| class.class_literal(db))
        }))
    }

    fn self_owner(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<Option<ClassLiteral<'db>>, Incomplete>> {
        ready(
            self.preflight_self_owner(env, variable)
                .map(|()| self_typevar_owner_class_literal(db, env, variable)),
        )
    }

    fn mro_literals(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> impl Future<Output = Result<&'db [ClassLiteral<'db>], Incomplete>> {
        ready(if expansion_probe::mro_effects_enabled() {
            read_source(self, || class_mro_literals(db, class).as_ref())
        } else {
            self.unsupported(UnsupportedSelfBindingOperation::UncontrolledMro)
        })
    }
}
