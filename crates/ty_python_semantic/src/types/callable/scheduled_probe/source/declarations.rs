//! Declaration facts for the constructor experiment, prepared outside semantic evaluation.
//!
//! These are raw declaration inputs. MRO selection, owner substitution and descriptor calls
//! still belong to evaluation. Missing facts have no source-inference fallback.

use std::fmt;

use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, name::Name};
use salsa::prepared_source_probe::{self as probe, CaptureError, Read, Stamp};
use ty_python_core::{ProgramFile, place_table, use_def_map};

use crate::place::{PlaceAndQualifiers, explicit_global_symbol, place_from_bindings};
use crate::types::callable::scheduled_probe::mapping::PromotionFactKey;
use crate::types::class::member_lookup::MroImplicitAttribute;
use crate::types::class::{ClassMetaclass, CodeGeneratorKind, MethodDecorator};
use crate::types::class_base::ClassBase;
use crate::types::enums::{enum_metadata, is_enum_class_by_inheritance};
use crate::types::generics::GenericContext;
use crate::types::member::{self, Member};
use crate::types::signatures::CallableSignature;
use crate::types::{CallableType, ClassLiteral, ClassType, FunctionType, StaticClassLiteral, Type};
use crate::{Db, FxOrderMap, Program, ProgramEnvironment};

mod mro;
mod promotion;

use mro::MroConstructionFacts;
use promotion::PromotionFacts;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::types::callable::scheduled_probe) enum DeclarationKey<'db> {
    File(ProgramFile<'db>),
    Global(Name),
    Context(StaticClassLiteral<'db>),
    ExplicitBases(StaticClassLiteral<'db>),
    ConvertedExplicitBase(StaticClassLiteral<'db>, usize),
    ObjectBase(Program<'db>),
    ProperMro(StaticClassLiteral<'db>),
    Namespace(StaticClassLiteral<'db>, Name),
    ClassInput(StaticClassLiteral<'db>, ClassInput),
    MemberInput(StaticClassLiteral<'db>, Name, MemberInput),
    Signature(FunctionType<'db>),
    Promotion(PromotionFactKey<'db>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::callable::scheduled_probe) enum ClassInput {
    InheritedGenericContext,
    HasPep695TypeParams,
    CodeGenerator,
    GeneratedSlots,
    ExplicitSlots,
    Metaclass,
    IsEnum,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::callable::scheduled_probe) enum MemberInput {
    OwnSlot,
    ImplicitClass,
    EnumMember,
    OwnInstance,
    RuntimeBindingAbsent,
    MroImplicit,
}

#[derive(Debug)]
pub(in crate::types::callable::scheduled_probe) enum PreparationError<'db> {
    Observation(DeclarationKey<'db>, CaptureError),
    InvalidMro(StaticClassLiteral<'db>),
    ProgramDomain(StaticClassLiteral<'db>),
}

impl fmt::Display for PreparationError<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Observation(key, error) => write!(formatter, "{key:?}: {error:?}"),
            Self::InvalidMro(class) => {
                write!(formatter, "unresolved declaration MRO for {class:?}")
            }
            Self::ProgramDomain(class) => {
                write!(
                    formatter,
                    "declaration belongs to another program: {class:?}"
                )
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::types::callable::scheduled_probe) struct MissingDeclaration<'db>(
    pub(in crate::types::callable::scheduled_probe) DeclarationKey<'db>,
);

struct Fact<T> {
    value: T,
    support: Box<[Read]>,
    stamp: Stamp,
}

impl<T> Fact<T> {
    fn read<'db>(
        db: &'db dyn Db,
        key: DeclarationKey<'db>,
        producer: impl FnOnce() -> T,
    ) -> Result<Self, PreparationError<'db>> {
        let captured = probe::capture(db, producer)
            .map_err(|error| PreparationError::Observation(key.clone(), error))?;
        captured
            .check_root_reads()
            .map_err(|error| PreparationError::Observation(key, error))?;
        Ok(Self {
            value: captured.value,
            stamp: captured.stamp,
            support: captured
                .reads
                .into_iter()
                .filter(|read| read.parent.is_none())
                .collect(),
        })
    }

    fn stored<U>(&self, value: U) -> Fact<U> {
        Fact {
            value,
            support: self.support.clone(),
            stamp: self.stamp,
        }
    }

    fn derive<'db, U>(
        &self,
        db: &'db dyn Db,
        key: DeclarationKey<'db>,
        producer: impl FnOnce() -> U,
    ) -> Result<Fact<U>, PreparationError<'db>> {
        let captured = probe::capture(db, producer)
            .map_err(|error| PreparationError::Observation(key.clone(), error))?;
        if captured.stamp != self.stamp {
            return Err(PreparationError::Observation(
                key,
                CaptureError::ChangedDatabaseStamp,
            ));
        }
        match captured.check_root_reads() {
            Ok(()) => {}
            // Declaration APIs can return stored interned fields without reading a query.
            // Their already-final owner fact supplies the evidence in that case.
            Err(CaptureError::NoRootReads) if captured.reads.is_empty() => {}
            Err(error) => return Err(PreparationError::Observation(key, error)),
        }
        Ok(Fact {
            value: captured.value,
            stamp: self.stamp,
            support: self
                .support
                .iter()
                .copied()
                .chain(
                    captured
                        .reads
                        .into_iter()
                        .filter(|read| read.parent.is_none()),
                )
                .collect(),
        })
    }
}

struct ClassFacts<'db> {
    context: Fact<Option<GenericContext<'db>>>,
    inherited_generic_context: Fact<Option<GenericContext<'db>>>,
    proper_mro: Option<Fact<Box<[ClassBase<'db>]>>>,
    namespace: FxOrderMap<Name, Fact<Member<'db>>>,
    code_generator: Fact<Option<CodeGeneratorKind<'db>>>,
    generated_slots: Fact<bool>,
    explicit_slots: Fact<bool>,
    metaclass: Fact<ClassMetaclass<'db>>,
    is_enum: Fact<bool>,
    member_inputs: FxOrderMap<Name, MemberFacts<'db>>,
}

struct MemberFacts<'db> {
    own_slot: Fact<bool>,
    implicit_class: Fact<Member<'db>>,
    enum_member: Fact<bool>,
    own_instance: Fact<Member<'db>>,
    runtime_binding_absent: Fact<bool>,
    mro_implicit: Fact<MroImplicitAttribute<'db>>,
}

pub(in crate::types::callable::scheduled_probe) struct PreparedDeclarations<'db> {
    // The borrow keeps the source revision and all retained memo addresses alive.
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    stamp: Stamp,
    globals: FxOrderMap<Name, Fact<PlaceAndQualifiers<'db>>>,
    classes: FxOrderMap<StaticClassLiteral<'db>, ClassFacts<'db>>,
    mro_construction: FxOrderMap<StaticClassLiteral<'db>, MroConstructionFacts<'db>>,
    object_base: Option<Fact<ClassBase<'db>>>,
    signatures: FxOrderMap<FunctionType<'db>, Fact<CallableType<'db>>>,
    promotion: PromotionFacts<'db>,
}

impl<'db> PreparedDeclarations<'db> {
    pub(in crate::types::callable::scheduled_probe) fn program(&self) -> Program<'db> {
        self.file.program(self.db)
    }

    fn empty(db: &'db dyn Db, file: ProgramFile<'db>) -> Self {
        Self {
            db,
            file,
            stamp: Stamp::current(db),
            globals: FxOrderMap::default(),
            classes: FxOrderMap::default(),
            mro_construction: FxOrderMap::default(),
            object_base: None,
            signatures: FxOrderMap::default(),
            promotion: PromotionFacts::default(),
        }
    }

    pub(in crate::types::callable::scheduled_probe) fn prepare(
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> Result<Self, PreparationError<'db>> {
        let mut prepared = Self::empty(db, file);
        let parsed = parsed_module(db, file.python_file(db)).load(db);
        for statement in parsed.suite() {
            let name = match statement {
                ast::Stmt::ClassDef(class) => &class.name.id,
                ast::Stmt::FunctionDef(function) => &function.name.id,
                _ => continue,
            };
            let global = Fact::read(db, DeclarationKey::Global(name.clone()), || {
                explicit_global_symbol(db, file, name)
            })?;
            match global.value.place.ignore_possibly_undefined() {
                Some(Type::ClassLiteral(ClassLiteral::Static(class))) => {
                    prepared.prepare_class(class)?;
                }
                Some(Type::FunctionLiteral(function)) => {
                    prepared.prepare_signature(function, &global)?;
                }
                _ => {}
            }
            prepared.globals.insert(name.clone(), global);
        }
        prepared.promotion = PromotionFacts::prepare(&prepared)?;
        if !prepared.stamp.belongs_to(db) {
            return Err(PreparationError::Observation(
                DeclarationKey::File(file),
                CaptureError::ChangedDatabaseStamp,
            ));
        }
        Ok(prepared)
    }

    fn prepare_class(
        &mut self,
        class: StaticClassLiteral<'db>,
    ) -> Result<(), PreparationError<'db>> {
        let db = self.db;
        let context = Fact::read(db, DeclarationKey::Context(class), || {
            class.generic_context(db)
        })?;
        let mro = Fact::read(db, DeclarationKey::ProperMro(class), || {
            class.try_mro(db, None)
        })?;
        let proper_mro = mro.stored(
            mro.value
                .map_err(|_| PreparationError::InvalidMro(class))?
                .iter()
                .skip(1)
                .copied()
                .collect(),
        );
        let env = ProgramEnvironment::from_file(self.file);
        let class_input = |input| DeclarationKey::ClassInput(class, input);
        let inherited_generic_context =
            context.derive(db, class_input(ClassInput::InheritedGenericContext), || {
                class.inherited_generic_context(db)
            })?;
        let code_generator = context.derive(db, class_input(ClassInput::CodeGenerator), || {
            CodeGeneratorKind::from_class(db, class.into())
        })?;
        let generated_slots =
            context.derive(db, class_input(ClassInput::GeneratedSlots), || {
                class.has_generated_slots(db)
            })?;
        let explicit_slots = context.derive(db, class_input(ClassInput::ExplicitSlots), || {
            class.has_explicit_slots(db)
        })?;
        let metaclass = context.derive(db, class_input(ClassInput::Metaclass), || {
            class.inferred_metaclass(db)
        })?;
        let is_enum = context.derive(db, class_input(ClassInput::IsEnum), || {
            is_enum_class_by_inheritance(db, &env, class)
        })?;

        // Even an absent dunder is a prepared fact. A missing map key instead means that
        // preparation did not supply the declaration requested by evaluation.
        let scope = class.body_scope(db);
        let names = place_table(db, scope)
            .symbols()
            .map(|symbol| symbol.name().clone())
            .chain(
                [
                    "__call__",
                    "__new__",
                    "__init__",
                    "__get__",
                    "__set__",
                    "__delete__",
                ]
                .map(Name::new),
            );
        let mut namespace = FxOrderMap::default();
        let mut member_inputs = FxOrderMap::default();
        for name in names {
            if namespace.contains_key(&name) {
                continue;
            }
            let member = Fact::read(db, DeclarationKey::Namespace(class, name.clone()), || {
                member::class_member(db, scope, &name)
            })?;
            if let Some(Type::FunctionLiteral(function)) = member.value.ignore_possibly_undefined()
            {
                self.prepare_signature(function, &member)?;
            }
            let member_input = |input| DeclarationKey::MemberInput(class, name.clone(), input);
            let own_slot = context.derive(db, member_input(MemberInput::OwnSlot), || {
                class.has_own_slot_descriptor(db, &name)
            })?;
            let implicit_class =
                context.derive(db, member_input(MemberInput::ImplicitClass), || {
                    class.implicit_attribute(db, &name, MethodDecorator::ClassMethod)
                })?;
            let enum_member = context.derive(db, member_input(MemberInput::EnumMember), || {
                enum_metadata(db, class.into())
                    .is_some_and(|metadata| metadata.contains_member(&name))
            })?;
            let own_instance =
                context.derive(db, member_input(MemberInput::OwnInstance), || {
                    ClassType::NonGeneric(class.into()).own_instance_member(db, &env, &name)
                })?;
            let runtime_binding_absent =
                context.derive(db, member_input(MemberInput::RuntimeBindingAbsent), || {
                    place_table(db, scope)
                        .symbol_id(&name)
                        .is_some_and(|symbol| {
                            place_from_bindings(
                                db,
                                &env,
                                use_def_map(db, scope).end_of_scope_symbol_bindings(symbol),
                            )
                            .place
                            .is_undefined()
                        })
                })?;
            let mro_implicit =
                context.derive(db, member_input(MemberInput::MroImplicit), || {
                    MroImplicitAttribute::read(db, class, &name)
                })?;
            member_inputs.insert(
                name.clone(),
                MemberFacts {
                    own_slot,
                    implicit_class,
                    enum_member,
                    own_instance,
                    runtime_binding_absent,
                    mro_implicit,
                },
            );
            namespace.insert(name, member);
        }
        self.classes.insert(
            class,
            ClassFacts {
                context,
                inherited_generic_context,
                proper_mro: Some(proper_mro),
                namespace,
                code_generator,
                generated_slots,
                explicit_slots,
                metaclass,
                is_enum,
                member_inputs,
            },
        );
        Ok(())
    }

    fn prepare_signature<T>(
        &mut self,
        function: FunctionType<'db>,
        owner: &Fact<T>,
    ) -> Result<(), PreparationError<'db>> {
        if self.signatures.contains_key(&function) {
            return Ok(());
        }
        let signature = owner.derive(self.db, DeclarationKey::Signature(function), || {
            function.into_callable_type(self.db)
        })?;
        self.signatures.insert(function, signature);
        Ok(())
    }

    pub(in crate::types::callable::scheduled_probe) fn namespace(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<&Member<'db>, MissingDeclaration<'db>> {
        self.classes
            .get(&class)
            .and_then(|facts| facts.namespace.get(name))
            .map(|fact| &fact.value)
            .ok_or_else(|| MissingDeclaration(DeclarationKey::Namespace(class, Name::new(name))))
    }

    pub(in crate::types::callable::scheduled_probe) fn context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, MissingDeclaration<'db>> {
        self.classes
            .get(&class)
            .map(|facts| facts.context.value)
            .or_else(|| {
                self.mro_construction
                    .get(&class)
                    .map(|facts| facts.context.value)
            })
            .ok_or(MissingDeclaration(DeclarationKey::Context(class)))
    }

    pub(in crate::types::callable::scheduled_probe) fn inherited_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, MissingDeclaration<'db>> {
        self.class_input(class, ClassInput::InheritedGenericContext)
            .map(|facts| facts.inherited_generic_context.value)
    }

    pub(in crate::types::callable::scheduled_probe) fn proper_mro(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&[ClassBase<'db>], MissingDeclaration<'db>> {
        self.classes
            .get(&class)
            .and_then(|facts| facts.proper_mro.as_ref())
            .map(|fact| fact.value.as_ref())
            .ok_or(MissingDeclaration(DeclarationKey::ProperMro(class)))
    }

    fn class_input(
        &self,
        class: StaticClassLiteral<'db>,
        input: ClassInput,
    ) -> Result<&ClassFacts<'db>, MissingDeclaration<'db>> {
        self.classes
            .get(&class)
            .ok_or(MissingDeclaration(DeclarationKey::ClassInput(class, input)))
    }

    pub(in crate::types::callable::scheduled_probe) fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, MissingDeclaration<'db>> {
        self.class_input(class, ClassInput::CodeGenerator)
            .map(|facts| facts.code_generator.value)
    }

    pub(in crate::types::callable::scheduled_probe) fn generated_slots(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, MissingDeclaration<'db>> {
        self.class_input(class, ClassInput::GeneratedSlots)
            .map(|facts| facts.generated_slots.value)
    }

    pub(in crate::types::callable::scheduled_probe) fn explicit_slots(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, MissingDeclaration<'db>> {
        self.class_input(class, ClassInput::ExplicitSlots)
            .map(|facts| facts.explicit_slots.value)
    }

    pub(in crate::types::callable::scheduled_probe) fn metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassMetaclass<'db>, MissingDeclaration<'db>> {
        self.class_input(class, ClassInput::Metaclass)
            .map(|facts| facts.metaclass.value)
    }

    pub(in crate::types::callable::scheduled_probe) fn is_enum(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, MissingDeclaration<'db>> {
        self.class_input(class, ClassInput::IsEnum)
            .map(|facts| facts.is_enum.value)
    }

    fn member_input(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
        input: MemberInput,
    ) -> Result<&MemberFacts<'db>, MissingDeclaration<'db>> {
        self.classes
            .get(&class)
            .and_then(|facts| facts.member_inputs.get(name))
            .ok_or_else(|| {
                MissingDeclaration(DeclarationKey::MemberInput(class, Name::new(name), input))
            })
    }

    pub(in crate::types::callable::scheduled_probe) fn own_slot(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, MissingDeclaration<'db>> {
        self.member_input(class, name, MemberInput::OwnSlot)
            .map(|facts| facts.own_slot.value)
    }

    pub(in crate::types::callable::scheduled_probe) fn implicit_class(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, MissingDeclaration<'db>> {
        self.member_input(class, name, MemberInput::ImplicitClass)
            .map(|facts| facts.implicit_class.value)
    }

    pub(in crate::types::callable::scheduled_probe) fn enum_member(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, MissingDeclaration<'db>> {
        self.member_input(class, name, MemberInput::EnumMember)
            .map(|facts| facts.enum_member.value)
    }

    pub(in crate::types::callable::scheduled_probe) fn own_instance(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, MissingDeclaration<'db>> {
        self.member_input(class, name, MemberInput::OwnInstance)
            .map(|facts| facts.own_instance.value)
    }

    pub(in crate::types::callable::scheduled_probe) fn runtime_binding_absent(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, MissingDeclaration<'db>> {
        self.member_input(class, name, MemberInput::RuntimeBindingAbsent)
            .map(|facts| facts.runtime_binding_absent.value)
    }

    pub(in crate::types::callable::scheduled_probe) fn mro_implicit(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<MroImplicitAttribute<'db>, MissingDeclaration<'db>> {
        self.member_input(class, name, MemberInput::MroImplicit)
            .map(|facts| facts.mro_implicit.value)
    }

    fn signature(
        &self,
        function: FunctionType<'db>,
    ) -> Result<&'db CallableSignature<'db>, MissingDeclaration<'db>> {
        self.signatures
            .get(&function)
            .map(|fact| fact.value.signatures(self.db))
            .ok_or(MissingDeclaration(DeclarationKey::Signature(function)))
    }

    pub(in crate::types::callable::scheduled_probe) fn matches(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> bool {
        self.stamp.belongs_to(db) && self.file.program(db) == env.program(db)
    }

    pub(in crate::types::callable::scheduled_probe) fn callable(
        &self,
        function: FunctionType<'db>,
    ) -> Option<CallableType<'db>> {
        self.signatures.get(&function).map(|fact| fact.value)
    }
}

#[cfg(test)]
mod tests;
