//! Protocol satisfaction with suspended nominal, structural, and class-side dependencies.

use std::ops::ControlFlow;

#[cfg(test)]
mod tests;

use super::{NominalInstanceType, ProtocolInstanceType, non_recursive_protocol_interface};
use crate::Db;
use crate::types::call::Bindings;
use crate::types::constraints::{
    ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
};
use crate::types::protocol_class::outer_step::InterfaceMembers;
use crate::types::protocol_class::{
    ProtocolClass, ProtocolInterface, ProtocolInterfaceView, ProtocolMember,
    StructuralMemberPriority, has_all_protocol_members_defined,
};
use crate::types::relation::dependencies::{OrdinaryDependencies, RelationDependencies};
use crate::types::relation::{
    RelationFieldReads, TypeRelation, TypeRelationChecker, TypeVarEvaluation,
};
use crate::types::visitor::any_over_type_expanding_aliases;
use crate::types::{
    ClassType, ErrorContext, GenericAlias, KnownClass, MaterializationKind, StaticClassLiteral,
    Type,
};

#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum ProtocolRelationWork {
    Entry,
    Transition,
    Argument,
    Member,
}

pub(in crate::types) trait ProtocolRelationEffects<'a, 'c, 'db> {
    type Error;

    async fn checkpoint(&self, work: ProtocolRelationWork) -> Result<(), Self::Error>;

    async fn check_type_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn check_type_satisfies_protocol(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn check_protocol_interface(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source_type: Type<'db>,
        source: ProtocolInterfaceView<'db>,
        target: ProtocolInterfaceView<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn check_protocol_member<'member>(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
        member: ProtocolMember<'member, 'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn check_meta_protocol_members(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        instance_ty: Type<'db>,
        meta_ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn is_never_satisfied(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Self::Error>;

    async fn is_always_satisfied(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Self::Error>;

    async fn protocol_interface(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<ProtocolInterfaceView<'db>, Self::Error>;

    async fn has_all_protocol_members_defined(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<bool, Self::Error>;

    async fn nominal_class(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        nominal: NominalInstanceType<'db>,
    ) -> Result<ClassType<'db>, Self::Error>;

    async fn materialization_changes_requirements(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        protocol: ProtocolInstanceType<'db>,
        required: ProtocolInstanceType<'db>,
    ) -> Result<bool, Self::Error>;

    async fn identity_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Self::Error>;

    async fn into_protocol_class(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<ProtocolClass<'db>>, Self::Error>;

    async fn non_recursive_protocol_interface(
        &self,
        interface: ProtocolInterface<'db>,
        protocol: ProtocolClass<'db>,
        receiver: Type<'db>,
    ) -> Result<ProtocolInterface<'db>, Self::Error>;

    async fn argument_has_unmentioned_typevar(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        argument: Type<'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Self::Error>;

    async fn member_has_explicit_receiver_annotation<'member>(
        &self,
        member: ProtocolMember<'member, 'db>,
    ) -> Result<bool, Self::Error>;

    async fn structural_member_priority<'member>(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        member: ProtocolMember<'member, 'db>,
    ) -> Result<StructuralMemberPriority, Self::Error>;

    async fn to_class_type(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;

    async fn bindings(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> Result<Bindings<'db>, Self::Error>;

    async fn bindings_return_type(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        bindings: &Bindings<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn combine_constraints(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn union_constraints(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        result: &mut ConstraintSet<'db, 'c>,
        other: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn imply_constraints(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        antecedent: ConstraintSet<'db, 'c>,
        consequent: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn push_constraints(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Self::Error>;

    async fn finish_constraints(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn next_interface_member(
        &self,
        members: &mut InterfaceMembers<'db>,
    ) -> Result<Option<ProtocolMember<'db, 'db>>, Self::Error>;

    async fn reserve_member_priorities(
        &self,
        capacity: usize,
    ) -> Result<Vec<(StructuralMemberPriority, ProtocolMember<'db, 'db>)>, Self::Error>;

    async fn push_member_priority(
        &self,
        members: &mut Vec<(StructuralMemberPriority, ProtocolMember<'db, 'db>)>,
        priority: StructuralMemberPriority,
        member: ProtocolMember<'db, 'db>,
    ) -> Result<(), Self::Error>;

    async fn sort_member_priorities(
        &self,
        members: &mut [(StructuralMemberPriority, ProtocolMember<'db, 'db>)],
    ) -> Result<(), Self::Error>;

    async fn advance_prioritized_member(
        &self,
        members: &[(StructuralMemberPriority, ProtocolMember<'db, 'db>)],
        index: &mut usize,
    ) -> Result<ProtocolMember<'db, 'db>, Self::Error>;

    async fn report_error(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        error: ErrorContext<'db>,
    ) -> Result<(), Self::Error>;
}

pub(in crate::types) trait SyncProtocolRelationEffects<'a, 'c, 'db> {
    type Error;

    fn checkpoint(&self, work: ProtocolRelationWork) -> Result<(), Self::Error>;

    fn check_type_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    fn check_type_satisfies_protocol(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    fn check_protocol_interface(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source_type: Type<'db>,
        source: ProtocolInterfaceView<'db>,
        target: ProtocolInterfaceView<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    fn check_protocol_member<'member>(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
        member: ProtocolMember<'member, 'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    fn check_meta_protocol_members(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        instance_ty: Type<'db>,
        meta_ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    fn is_never_satisfied(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Self::Error>;

    fn is_always_satisfied(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Self::Error>;

    fn protocol_interface(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<ProtocolInterfaceView<'db>, Self::Error>;

    fn has_all_protocol_members_defined(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<bool, Self::Error>;

    fn nominal_class(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        nominal: NominalInstanceType<'db>,
    ) -> Result<ClassType<'db>, Self::Error>;

    fn materialization_changes_requirements(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        protocol: ProtocolInstanceType<'db>,
        required: ProtocolInstanceType<'db>,
    ) -> Result<bool, Self::Error>;

    fn identity_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Self::Error>;

    fn into_protocol_class(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<ProtocolClass<'db>>, Self::Error>;

    fn non_recursive_protocol_interface(
        &self,
        interface: ProtocolInterface<'db>,
        protocol: ProtocolClass<'db>,
        receiver: Type<'db>,
    ) -> Result<ProtocolInterface<'db>, Self::Error>;

    fn argument_has_unmentioned_typevar(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        argument: Type<'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Self::Error>;

    fn member_has_explicit_receiver_annotation<'member>(
        &self,
        member: ProtocolMember<'member, 'db>,
    ) -> Result<bool, Self::Error>;

    fn structural_member_priority<'member>(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        member: ProtocolMember<'member, 'db>,
    ) -> Result<StructuralMemberPriority, Self::Error>;

    fn to_class_type(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;

    fn bindings(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> Result<Bindings<'db>, Self::Error>;

    fn bindings_return_type(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        bindings: &Bindings<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    fn combine_constraints(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    fn union_constraints(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        result: &mut ConstraintSet<'db, 'c>,
        other: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    fn imply_constraints(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        antecedent: ConstraintSet<'db, 'c>,
        consequent: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    fn push_constraints(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Self::Error>;

    fn finish_constraints(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    fn next_interface_member(
        &self,
        members: &mut InterfaceMembers<'db>,
    ) -> Result<Option<ProtocolMember<'db, 'db>>, Self::Error>;

    fn reserve_member_priorities(
        &self,
        capacity: usize,
    ) -> Result<Vec<(StructuralMemberPriority, ProtocolMember<'db, 'db>)>, Self::Error>;

    fn push_member_priority(
        &self,
        members: &mut Vec<(StructuralMemberPriority, ProtocolMember<'db, 'db>)>,
        priority: StructuralMemberPriority,
        member: ProtocolMember<'db, 'db>,
    ) -> Result<(), Self::Error>;

    fn sort_member_priorities(
        &self,
        members: &mut [(StructuralMemberPriority, ProtocolMember<'db, 'db>)],
    ) -> Result<(), Self::Error>;

    fn advance_prioritized_member(
        &self,
        members: &[(StructuralMemberPriority, ProtocolMember<'db, 'db>)],
        index: &mut usize,
    ) -> Result<ProtocolMember<'db, 'db>, Self::Error>;

    fn report_error(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        error: ErrorContext<'db>,
    ) -> Result<(), Self::Error>;
}

pub(super) struct InlineProtocolRelationEffects<'deps, 'db, D: RelationDependencies> {
    db: &'db dyn Db,
    dependencies: &'deps D,
}

impl<'deps, 'db, D: RelationDependencies> InlineProtocolRelationEffects<'deps, 'db, D> {
    pub(super) fn new(db: &'db dyn Db, dependencies: &'deps D) -> Self {
        Self { db, dependencies }
    }
}

impl<'a, 'c, 'db, D: RelationDependencies> SyncProtocolRelationEffects<'a, 'c, 'db>
    for InlineProtocolRelationEffects<'_, 'db, D>
{
    type Error = D::Error;

    fn checkpoint(&self, work: ProtocolRelationWork) -> Result<(), Self::Error> {
        self.dependencies.run(self.db, || {
            let _ = work;
        })
    }

    fn check_type_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error> {
        self.dependencies
            .run(self.db, || checker.check_type_pair(self.db, source, target))
    }

    fn check_type_satisfies_protocol(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error> {
        self.dependencies.run(self.db, || {
            checker.check_type_satisfies_protocol(self.db, ty, protocol)
        })
    }

    fn check_protocol_interface(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source_type: Type<'db>,
        source: ProtocolInterfaceView<'db>,
        target: ProtocolInterfaceView<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error> {
        self.dependencies.run(self.db, || {
            checker.check_protocol_interface_pair(self.db, source_type, source, target)
        })
    }

    fn check_protocol_member<'member>(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
        member: ProtocolMember<'member, 'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error> {
        self.dependencies.run(self.db, || {
            checker.type_satisfies_protocol_member(self.db, ty, &member)
        })
    }

    fn check_meta_protocol_members(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        instance_ty: Type<'db>,
        meta_ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error> {
        self.dependencies.run(self.db, || {
            checker.check_meta_protocol_members(self.db, instance_ty, meta_ty, protocol)
        })
    }

    fn is_never_satisfied(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Self::Error> {
        self.dependencies.run(self.db, || {
            constraints.is_never_satisfied(self.db, checker.env)
        })
    }

    fn is_always_satisfied(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Self::Error> {
        self.dependencies.run(self.db, || {
            constraints.is_always_satisfied(self.db, checker.env)
        })
    }

    fn protocol_interface(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<ProtocolInterfaceView<'db>, Self::Error> {
        self.dependencies
            .run(self.db, || protocol.interface(self.db))
    }

    fn has_all_protocol_members_defined(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<bool, Self::Error> {
        self.dependencies.run(self.db, || {
            has_all_protocol_members_defined(self.db, checker.env, ty, protocol)
        })
    }

    fn nominal_class(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        nominal: NominalInstanceType<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        self.dependencies
            .run(self.db, || nominal.class(self.db, checker.env))
    }

    fn materialization_changes_requirements(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        protocol: ProtocolInstanceType<'db>,
        required: ProtocolInstanceType<'db>,
    ) -> Result<bool, Self::Error> {
        self.dependencies.run(self.db, || {
            protocol.materialization_changes_requirements(self.db, checker.env, required)
        })
    }

    fn identity_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        self.dependencies
            .run(self.db, || class.identity_specialization(self.db))
    }

    fn into_protocol_class(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<ProtocolClass<'db>>, Self::Error> {
        self.dependencies
            .run(self.db, || class.into_protocol_class(self.db))
    }

    fn non_recursive_protocol_interface(
        &self,
        interface: ProtocolInterface<'db>,
        protocol: ProtocolClass<'db>,
        receiver: Type<'db>,
    ) -> Result<ProtocolInterface<'db>, Self::Error> {
        self.dependencies.run(self.db, || {
            non_recursive_protocol_interface(self.db, interface, protocol, receiver)
        })
    }

    fn argument_has_unmentioned_typevar(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        argument: Type<'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Self::Error> {
        self.dependencies.run(self.db, || {
            any_over_type_expanding_aliases(self.db, checker.env, argument, |nested| {
                matches!(nested, Type::TypeVar(typevar) if !constraints.mentions_typevar(self.db, typevar))
            })
        })
    }

    fn member_has_explicit_receiver_annotation<'member>(
        &self,
        member: ProtocolMember<'member, 'db>,
    ) -> Result<bool, Self::Error> {
        self.dependencies
            .run(self.db, || member.has_explicit_receiver_annotation(self.db))
    }

    fn structural_member_priority<'member>(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        member: ProtocolMember<'member, 'db>,
    ) -> Result<StructuralMemberPriority, Self::Error> {
        self.dependencies.run(self.db, || {
            member.structural_member_priority(self.db, checker.env)
        })
    }

    fn to_class_type(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Self::Error> {
        self.dependencies.run(self.db, || ty.to_class_type(self.db))
    }

    fn bindings(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> Result<Bindings<'db>, Self::Error> {
        self.dependencies
            .run(self.db, || ty.bindings(self.db, checker.env))
    }

    fn bindings_return_type(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        bindings: &Bindings<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.dependencies
            .run(self.db, || bindings.return_type(self.db, checker.env))
    }

    fn combine_constraints(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error> {
        self.dependencies.run(self.db, || match kind {
            ConstraintFoldKind::All => left.and(self.db, builder, || right),
            ConstraintFoldKind::Any => left.or(self.db, builder, || right),
        })
    }

    fn union_constraints(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        result: &mut ConstraintSet<'db, 'c>,
        other: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error> {
        self.dependencies
            .run(self.db, || result.union(self.db, builder, other))
    }

    fn imply_constraints(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        antecedent: ConstraintSet<'db, 'c>,
        consequent: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error> {
        self.dependencies.run(self.db, || {
            antecedent.implies(self.db, builder, || consequent)
        })
    }

    fn push_constraints(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Self::Error> {
        self.dependencies.run(self.db, || fold.push(next))
    }

    fn finish_constraints(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error> {
        self.dependencies.run(self.db, || fold.finish_borrowed())
    }

    fn next_interface_member(
        &self,
        members: &mut InterfaceMembers<'db>,
    ) -> Result<Option<ProtocolMember<'db, 'db>>, Self::Error> {
        self.dependencies.run(self.db, || members.next())
    }

    fn reserve_member_priorities(
        &self,
        capacity: usize,
    ) -> Result<Vec<(StructuralMemberPriority, ProtocolMember<'db, 'db>)>, Self::Error> {
        self.dependencies
            .run(self.db, || Vec::with_capacity(capacity))
    }

    fn push_member_priority(
        &self,
        members: &mut Vec<(StructuralMemberPriority, ProtocolMember<'db, 'db>)>,
        priority: StructuralMemberPriority,
        member: ProtocolMember<'db, 'db>,
    ) -> Result<(), Self::Error> {
        self.dependencies
            .run(self.db, || members.push((priority, member)))
    }

    fn sort_member_priorities(
        &self,
        members: &mut [(StructuralMemberPriority, ProtocolMember<'db, 'db>)],
    ) -> Result<(), Self::Error> {
        self.dependencies.run(self.db, || {
            members.sort_by(|(left, _), (right, _)| left.cmp(right))
        })
    }

    fn advance_prioritized_member(
        &self,
        members: &[(StructuralMemberPriority, ProtocolMember<'db, 'db>)],
        index: &mut usize,
    ) -> Result<ProtocolMember<'db, 'db>, Self::Error> {
        self.dependencies.run(self.db, || {
            let member = members[*index].1;
            *index += 1;
            member
        })
    }

    fn report_error(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        error: ErrorContext<'db>,
    ) -> Result<(), Self::Error> {
        self.dependencies.run(self.db, || {
            if let Some(context) = checker.report_context() {
                context.push(error);
            }
        })
    }
}

pub(in crate::types) enum ProtocolRelationStep<'checker, 'a, 'c, 'db> {
    Complete(ConstraintSet<'db, 'c>),
    Relate(PendingProtocolPair<'checker, 'a, 'c, 'db>),
    Interface(PendingProtocolInterface<'checker, 'a, 'c, 'db>),
    Member(PendingProtocolMember<'checker, 'a, 'c, 'db>),
    MetaBindings(PendingMetaBindings<'checker, 'a, 'c, 'db>),
    MetaMembers(PendingMetaMembers<'checker, 'a, 'c, 'db>),
}

#[derive(Clone, Copy)]
struct ProtocolInput<'checker, 'a, 'c, 'db> {
    checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    ty: Type<'db>,
    protocol: ProtocolInstanceType<'db>,
}

impl<'checker, 'a, 'c, 'db> ProtocolRelationStep<'checker, 'a, 'c, 'db> {
    pub(in crate::types) fn start<D: RelationDependencies>(
        db: &'db dyn Db,
        checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
        dependencies: &D,
    ) -> Result<Self, D::Error> {
        protocol_relation_start_sync(
            RelationFieldReads::new(db),
            checker,
            ty,
            protocol,
            &InlineProtocolRelationEffects::new(db, dependencies),
        )
    }

    pub(in crate::types) fn start_meta<D: RelationDependencies>(
        db: &'db dyn Db,
        checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
        meta_ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
        dependencies: &D,
    ) -> Result<Self, D::Error> {
        protocol_relation_start_meta_sync(
            RelationFieldReads::new(db),
            checker,
            meta_ty,
            protocol,
            &InlineProtocolRelationEffects::new(db, dependencies),
        )
    }
}

#[ty_mapping_probe_macros::dual_protocol_relation]
async fn protocol_relation_start_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    ty: Type<'db>,
    protocol: ProtocolInstanceType<'db>,
    effects: &E,
) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {
    effects.checkpoint(ProtocolRelationWork::Transition).await?;
    let source_protocol = ty.as_protocol_instance();
    // Every gradual type lies between its bottom and top materializations. The exact same
    // specialization therefore settles these directions without expanding recursive members.
    if let Some(source) = source_protocol
        && matches!(
            (
                fields.protocol_materialization_kind(source),
                fields.protocol_materialization_kind(protocol)
            ),
            (
                None | Some(MaterializationKind::Bottom),
                Some(MaterializationKind::Top)
            ) | (Some(MaterializationKind::Bottom), None)
        )
        && let (Some(source_origin), Some(target_origin)) = (
            fields.protocol_class_origin(source),
            fields.protocol_class_origin(protocol),
        )
        && source_origin == target_origin
    {
        return Ok(ProtocolRelationStep::Complete(checker.always()));
    }
    let input = ProtocolInput {
        checker,
        ty,
        protocol,
    };
    let source_nominal = match source_protocol {
        Some(source) => fields.protocol_nominal_origin_instance(source),
        None => None,
    };
    if let Some(target_nominal) = fields.protocol_nominal_origin_instance(protocol) {
        // Explicit protocol inheritance remains valid even when an override changes a member
        // incompatibly, so both protocol origins participate in the nominal comparison.
        return Ok(ProtocolRelationStep::Relate(PendingProtocolPair {
            checker,
            source: source_nominal.map(Type::NominalInstance).unwrap_or(ty),
            target: Type::NominalInstance(target_nominal),
            continuation: PairContinuation::Nominal {
                input,
                source_nominal,
                target_nominal,
            },
        }));
    }
    protocol_relation_structural_with(fields, input, checker.never(), effects).await
}

#[ty_mapping_probe_macros::dual_protocol_relation]
async fn protocol_relation_start_meta_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    meta_ty: Type<'db>,
    protocol: ProtocolInstanceType<'db>,
    effects: &E,
) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {
    let _ = fields;
    effects.checkpoint(ProtocolRelationWork::Transition).await?;
    debug_assert!(matches!(
        meta_ty,
        Type::ClassLiteral(_) | Type::SubclassOf(_) | Type::GenericAlias(_)
    ));
    // There are no constructor arguments from which to infer class type arguments. Preserve
    // the ordinary meta-protocol operation's default specialization before obtaining bindings.
    let constructor_ty = effects
        .to_class_type(meta_ty)
        .await?
        .map_or(meta_ty, Type::from);
    Ok(ProtocolRelationStep::MetaBindings(PendingMetaBindings {
        checker,
        constructor_ty,
        meta_ty,
        protocol,
    }))
}

pub(in crate::types) struct PendingProtocolPair<'checker, 'a, 'c, 'db> {
    pub(in crate::types) checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    pub(in crate::types) source: Type<'db>,
    pub(in crate::types) target: Type<'db>,
    continuation: PairContinuation<'checker, 'a, 'c, 'db>,
}

enum PairContinuation<'checker, 'a, 'c, 'db> {
    Nominal {
        input: ProtocolInput<'checker, 'a, 'c, 'db>,
        source_nominal: Option<NominalInstanceType<'db>>,
        target_nominal: NominalInstanceType<'db>,
    },
    Meta {
        meta_ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
    },
}

impl<'checker, 'a, 'c, 'db> PendingProtocolPair<'checker, 'a, 'c, 'db> {
    pub(in crate::types) fn resume<D: RelationDependencies>(
        self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, D::Error> {
        protocol_relation_pair_resume_sync(
            RelationFieldReads::new(db),
            self,
            result,
            &InlineProtocolRelationEffects::new(db, dependencies),
        )
    }
}

#[ty_mapping_probe_macros::dual_protocol_relation]
async fn protocol_relation_pair_resume_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    pending: PendingProtocolPair<'checker, 'a, 'c, 'db>,
    result: ConstraintSet<'db, 'c>,
    effects: &E,
) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {
    effects.checkpoint(ProtocolRelationWork::Transition).await?;
    match pending.continuation {
        PairContinuation::Nominal {
            input,
            source_nominal,
            target_nominal,
        } => {
            protocol_relation_after_nominal_with(
                fields,
                input,
                source_nominal,
                target_nominal,
                result,
                effects,
            )
            .await
        }
        PairContinuation::Meta { meta_ty, protocol } => {
            if result.is_trivially_never_satisfied() {
                return Ok(ProtocolRelationStep::Complete(result));
            }
            Ok(ProtocolRelationStep::MetaMembers(PendingMetaMembers {
                checker: pending.checker,
                instance_ty: pending.source,
                meta_ty,
                protocol,
                instance_result: result,
            }))
        }
    }
}

#[ty_mapping_probe_macros::dual_protocol_relation]
async fn protocol_relation_after_nominal_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    input: ProtocolInput<'checker, 'a, 'c, 'db>,
    source_nominal: Option<NominalInstanceType<'db>>,
    target_nominal: NominalInstanceType<'db>,
    nominally_satisfied: ConstraintSet<'db, 'c>,
    effects: &E,
) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {
    effects.checkpoint(ProtocolRelationWork::Transition).await?;
    let checker = input.checker;
    let source_protocol = input.ty.as_protocol_instance();
    // Generator parameters must be compared nominally: structural inference through
    // close() -> ReturnT | None can infer a spurious None on Python 3.13 and newer.
    // TODO: Remove the Python 3.13+ extension when https://github.com/astral-sh/ty/issues/3596 is fixed.
    if fields.nominal_has_known_class(target_nominal, KnownClass::Generator)
        && let Some(source) = source_nominal
        && fields.nominal_has_known_class(source, KnownClass::Generator)
    {
        return Ok(ProtocolRelationStep::Complete(nominally_satisfied));
    }
    // Check the cheap rejection before expanding materialized requirements: an unrelated
    // recursive protocol can otherwise expand before a finite member gets a chance to reject.
    let can_use_nominal_result_directly = if effects
        .is_never_satisfied(checker, nominally_satisfied)
        .await?
    {
        true
    } else if fields.protocol_materialization_kind(input.protocol) == Some(MaterializationKind::Top)
        || !effects
            .materialization_changes_requirements(checker, input.protocol, input.protocol)
            .await?
    {
        match source_protocol {
            Some(source) => {
                !effects
                    .materialization_changes_requirements(checker, source, input.protocol)
                    .await?
            }
            None => true,
        }
    } else {
        false
    };
    let mut result = checker.never();
    if can_use_nominal_result_directly
        && effects
            .union_constraints(checker.constraints, &mut result, nominally_satisfied)
            .await?
            .is_trivially_always_satisfied()
    {
        return Ok(ProtocolRelationStep::Complete(result));
    }
    // A failed redundancy check can retain both union arms without expanding every member.
    let can_use_nominal_redundancy = can_use_nominal_result_directly
        && matches!(checker.relation, TypeRelation::Redundancy { pure: false })
        && match source_nominal {
            Some(source) => {
                let source_class = effects.nominal_class(checker, source).await?;
                let target_class = effects.nominal_class(checker, target_nominal).await?;
                fields.class_literal(source_class) == fields.class_literal(target_class)
            }
            None => false,
        };
    // Lazy finite comparisons may add structural solutions; eager comparisons only reject.
    if (checker.typevar_evaluation == TypeVarEvaluation::Lazy || !can_use_nominal_redundancy)
        && let Some(finite) = protocol_relation_non_recursive_interface_with(
            fields,
            input,
            source_nominal,
            target_nominal,
            effects,
        )
        .await?
    {
        return Ok(ProtocolRelationStep::Interface(PendingProtocolInterface {
            checker,
            source_type: input.ty,
            source: finite.source,
            target: finite.target,
            continuation: InterfaceContinuation::Finite {
                input,
                finite,
                nominally_satisfied,
                nominal_result: result,
                can_use_nominal_redundancy,
            },
        }));
    }
    if can_use_nominal_redundancy {
        return Ok(ProtocolRelationStep::Complete(nominally_satisfied));
    }
    protocol_relation_structural_with(fields, input, result, effects).await
}

/// Prepares the finite-member comparison for specializations of the same protocol.
///
/// In this example, `value` can be checked without comparing another `Chain`, while checking
/// `child` leads to another protocol comparison:
///
/// ```python
/// class Chain[T](Protocol):
///     def value(self) -> T: ...
///     def child(self) -> Chain[tuple[T]]: ...
/// ```
///
/// Expanding `child` while comparing `Chain[S]` with `Chain[T]` produces a comparison of
/// `Chain[tuple[S]]` with `Chain[tuple[T]]`, then another with doubly nested tuples, and so on.
/// Each pair is different, so checking for an already-visited pair does not stop the expansion.
/// Comparing `value` instead relates `S` to `T` directly. In this example, that also establishes
/// the relationship between their tuples, without expanding `child` at all.
///
/// For materialized protocols, we need more than a successful check of the remaining members.
/// Their constraints must mention every type variable in both sets of type arguments and imply
/// the nominal relation: every solution they allow must also satisfy the comparison of the
/// type arguments, according to the protocol's variance. Together with the materialization
/// checks below, this establishes that the recursive members cannot add further restrictions.
///
/// The continuation keeps the structural constraints, not the nominal result. In particular, the
/// unmaterialized path retains structural solutions from members such as `value() -> T | int`
/// that comparing type arguments alone would miss.
///
/// Eager comparisons can only reject: matching the finite requirements does not prove that
/// the omitted recursive members are compatible. Materialized protocols use this shortcut only
/// during lazy evaluation.
///
/// Returning `None` means that we cannot attempt this shortcut, not that the relation fails. The
/// caller continues with its usual checks, including the full recursive comparison when needed.
#[ty_mapping_probe_macros::dual_protocol_relation]
async fn protocol_relation_non_recursive_interface_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    input: ProtocolInput<'checker, 'a, 'c, 'db>,
    source_nominal: Option<NominalInstanceType<'db>>,
    target_nominal: NominalInstanceType<'db>,
    effects: &E,
) -> Result<Option<FiniteInterface<'db>>, E::Error> {
    effects.checkpoint(ProtocolRelationWork::Transition).await?;
    let checker = input.checker;
    if checker.is_context_collection_enabled() {
        return Ok(None);
    }
    let Type::ProtocolInstance(source_protocol) = input.ty else {
        return Ok(None);
    };
    let Some(source_nominal) = source_nominal else {
        return Ok(None);
    };
    let source_class = effects.nominal_class(checker, source_nominal).await?;
    let target_class = effects.nominal_class(checker, target_nominal).await?;
    let (ClassType::Generic(source_alias), ClassType::Generic(target_alias)) =
        (source_class, target_class)
    else {
        return Ok(None);
    };
    if fields.alias_origin(source_alias) != fields.alias_origin(target_alias) {
        return Ok(None);
    }
    // Assignability chooses `Bottom` for an unmaterialized source and `Top` for an
    // unmaterialized target. An explicit `Top -> Bottom` comparison is different:
    // materialization can make a recursive requirement incompatible even when the type
    // arguments are compatible.
    //
    // For example, consider:
    //
    //   class P[T](Protocol):
    //       def value(self) -> T: ...
    //       def consume(self, other: P[Any]) -> Any: ...
    //
    // Comparing `Top[P[str]]` with `Bottom[P[object]]` accepts `value`, since `str` is a
    // subtype of `object`. But `consume` returns `object` in the source and must return
    // `Never` in the target. This fixed `Any` changes independently of `T`, so neither the
    // finite member nor the nominal comparison detects the mismatch. Leave that direction
    // to the full structural check.
    let materialized = match (
        fields.protocol_materialization_kind(source_protocol),
        fields.protocol_materialization_kind(input.protocol),
    ) {
        (None, None) => false,
        (Some(MaterializationKind::Top), Some(MaterializationKind::Bottom)) => return Ok(None),
        _ if checker.typevar_evaluation == TypeVarEvaluation::Lazy
            && checker.relation.is_assignability() =>
        {
            true
        }
        _ => return Ok(None),
    };
    let identity = effects
        .identity_specialization(fields.alias_origin(target_alias))
        .await?;
    let Some(identity_protocol) = effects.into_protocol_class(identity).await? else {
        return Ok(None);
    };
    let source = effects.protocol_interface(source_protocol).await?;
    let target = effects.protocol_interface(input.protocol).await?;
    let non_recursive = effects
        .non_recursive_protocol_interface(
            target.base(),
            identity_protocol,
            Type::ProtocolInstance(input.protocol),
        )
        .await?;
    if non_recursive == target.base() {
        return Ok(None);
    }
    // Remove recursive requirements only from the target, and keep the complete source as
    // evidence that the remaining requirements are satisfied. For example, when comparing
    // `Chain[Chain[int]]` with `Chain[object]`, the target's `value() -> object` is
    // non-recursive, but the source's `value() -> Chain[int]` refers to `Chain`. Filtering both
    // interfaces would remove the source member we need to establish that valid return-type
    // comparison.
    Ok(Some(FiniteInterface {
        source,
        target: ProtocolInterfaceView::new(non_recursive, target.materialization_kind()),
        source_alias,
        target_alias,
        materialized,
    }))
}

#[ty_mapping_probe_macros::dual_protocol_relation]
async fn protocol_relation_structural_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    input: ProtocolInput<'checker, 'a, 'c, 'db>,
    nominal_result: ConstraintSet<'db, 'c>,
    effects: &E,
) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {
    effects.checkpoint(ProtocolRelationWork::Transition).await?;
    if !input.checker.is_context_collection_enabled()
        && !effects
            .has_all_protocol_members_defined(input.checker, input.ty, input.protocol)
            .await?
    {
        return Ok(ProtocolRelationStep::Complete(nominal_result));
    }
    if let Type::ProtocolInstance(source_protocol) = input.ty {
        let source = effects.protocol_interface(source_protocol).await?;
        let target = effects.protocol_interface(input.protocol).await?;
        return Ok(ProtocolRelationStep::Interface(PendingProtocolInterface {
            checker: input.checker,
            source_type: input.ty,
            source,
            target,
            continuation: InterfaceContinuation::Structural {
                input,
                nominal_result,
            },
        }));
    }
    if let Some(members) =
        protocol_relation_nominal_recursive_members_with(fields, input, nominal_result, effects)
            .await?
    {
        return protocol_relation_nominal_finite_next_with(fields, members, effects).await;
    }
    let interface = effects.protocol_interface(input.protocol).await?;
    let members = StructuralMembers {
        input,
        nominal_result,
        members: InterfaceMembers::with_fields(fields, interface),
        fold: ConstraintFold::new(input.checker.constraints, ConstraintFoldKind::All),
    };
    protocol_relation_structural_next_with(fields, members, effects).await
}

#[ty_mapping_probe_macros::dual_protocol_relation]
async fn protocol_relation_nominal_recursive_members_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    input: ProtocolInput<'checker, 'a, 'c, 'db>,
    nominally_satisfied: ConstraintSet<'db, 'c>,
    effects: &E,
) -> Result<Option<NominalRecursiveMembers<'checker, 'a, 'c, 'db>>, E::Error> {
    effects.checkpoint(ProtocolRelationWork::Transition).await?;
    let checker = input.checker;
    if checker.typevar_evaluation != TypeVarEvaluation::Lazy
        || checker.is_context_collection_enabled()
        || nominally_satisfied.is_trivially_never_satisfied()
    {
        return Ok(None);
    }
    let Some(source) = input.ty.as_nominal_instance() else {
        return Ok(None);
    };
    let source_class = effects.nominal_class(checker, source).await?;
    let Some(source_alias) = source_class.into_generic_alias() else {
        return Ok(None);
    };
    let source_arguments = fields.specialization_types(fields.alias_specialization(source_alias));
    // Structural inference is still needed for variables that the nominal proof cannot see,
    // including nested variables such as T in Concrete[T | Iterable[T]].
    for argument in source_arguments {
        effects.checkpoint(ProtocolRelationWork::Argument).await?;
        if effects
            .argument_has_unmentioned_typevar(checker, *argument, nominally_satisfied)
            .await?
        {
            return Ok(None);
        }
    }
    let interface = effects.protocol_interface(input.protocol).await?;
    // Concrete sources normally add useful inference. Recursive receiver binding is the
    // exception for same-origin sources or an explicit receiver annotation.
    if !source_arguments
        .iter()
        .any(|argument| argument.is_type_var())
        && !fields
            .protocol_class_origin(input.protocol)
            .is_some_and(|target| {
                fields.class_literal(source_class) == fields.class_literal(*target)
            })
    {
        let mut has_explicit_receiver = false;
        let mut cursor = InterfaceMembers::with_fields(fields, interface);
        while let Some(member) = effects.next_interface_member(&mut cursor).await? {
            effects.checkpoint(ProtocolRelationWork::Member).await?;
            if effects
                .member_has_explicit_receiver_annotation(member)
                .await?
            {
                has_explicit_receiver = true;
                break;
            }
        }
        if !has_explicit_receiver {
            return Ok(None);
        }
    }
    let mut members = effects
        .reserve_member_priorities(fields.protocol_interface_member_count(interface))
        .await?;
    let mut cursor = InterfaceMembers::with_fields(fields, interface);
    while let Some(member) = effects.next_interface_member(&mut cursor).await? {
        effects.checkpoint(ProtocolRelationWork::Member).await?;
        let priority = effects.structural_member_priority(checker, member).await?;
        effects
            .push_member_priority(&mut members, priority, member)
            .await?;
    }
    effects.sort_member_priorities(&mut members).await?;
    let first_recursive = members
        .partition_point(|(priority, _)| !matches!(priority, StructuralMemberPriority::Recursive));
    if first_recursive == members.len() {
        return Ok(None);
    }
    Ok(Some(NominalRecursiveMembers {
        input,
        nominally_satisfied,
        members,
        index: 0,
        first_recursive,
        fold: ConstraintFold::new(checker.constraints, ConstraintFoldKind::All),
    }))
}

#[ty_mapping_probe_macros::dual_protocol_relation]
async fn protocol_relation_finish_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    input: ProtocolInput<'checker, 'a, 'c, 'db>,
    nominal_result: ConstraintSet<'db, 'c>,
    structural: ConstraintSet<'db, 'c>,
    effects: &E,
) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {
    let _ = fields;
    effects.checkpoint(ProtocolRelationWork::Transition).await?;
    if input.checker.is_context_collection_enabled()
        && effects
            .is_never_satisfied(input.checker, structural)
            .await?
    {
        effects
            .report_error(
                input.checker,
                ErrorContext::TypeNotCompatibleWithProtocol {
                    ty: input.ty,
                    protocol: Type::ProtocolInstance(input.protocol),
                },
            )
            .await?;
    }
    effects
        .combine_constraints(
            input.checker.constraints,
            ConstraintFoldKind::Any,
            nominal_result,
            structural,
        )
        .await
        .map(ProtocolRelationStep::Complete)
}

#[derive(Clone, Copy)]
struct FiniteInterface<'db> {
    source: ProtocolInterfaceView<'db>,
    target: ProtocolInterfaceView<'db>,
    source_alias: GenericAlias<'db>,
    target_alias: GenericAlias<'db>,
    materialized: bool,
}

pub(in crate::types) struct PendingProtocolInterface<'checker, 'a, 'c, 'db> {
    pub(in crate::types) checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    pub(in crate::types) source_type: Type<'db>,
    pub(in crate::types) source: ProtocolInterfaceView<'db>,
    pub(in crate::types) target: ProtocolInterfaceView<'db>,
    continuation: InterfaceContinuation<'checker, 'a, 'c, 'db>,
}

enum InterfaceContinuation<'checker, 'a, 'c, 'db> {
    Structural {
        input: ProtocolInput<'checker, 'a, 'c, 'db>,
        nominal_result: ConstraintSet<'db, 'c>,
    },
    Finite {
        input: ProtocolInput<'checker, 'a, 'c, 'db>,
        finite: FiniteInterface<'db>,
        nominally_satisfied: ConstraintSet<'db, 'c>,
        nominal_result: ConstraintSet<'db, 'c>,
        can_use_nominal_redundancy: bool,
    },
}

impl<'checker, 'a, 'c, 'db> PendingProtocolInterface<'checker, 'a, 'c, 'db> {
    pub(in crate::types) fn resume<D: RelationDependencies>(
        self,
        db: &'db dyn Db,
        structural: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, D::Error> {
        protocol_relation_interface_resume_sync(
            RelationFieldReads::new(db),
            self,
            structural,
            &InlineProtocolRelationEffects::new(db, dependencies),
        )
    }
}

#[ty_mapping_probe_macros::dual_protocol_relation]
async fn protocol_relation_interface_resume_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    pending: PendingProtocolInterface<'checker, 'a, 'c, 'db>,
    structural: ConstraintSet<'db, 'c>,
    effects: &E,
) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {
    effects.checkpoint(ProtocolRelationWork::Transition).await?;
    match pending.continuation {
        InterfaceContinuation::Structural {
            input,
            nominal_result,
        } => {
            protocol_relation_finish_with(fields, input, nominal_result, structural, effects).await
        }
        InterfaceContinuation::Finite {
            input,
            finite,
            nominally_satisfied,
            nominal_result,
            can_use_nominal_redundancy,
        } => {
            // A skipped member can be the only source of information about a type variable. In this
            // example, `marker: Any` ensures materialization changes the interface for static arguments:
            //
            //   class Pair[First, Second](Protocol):
            //       marker: Any
            //       @property
            //       def first(self) -> First: ...
            //       def recursive_second(self, child: Pair[Any, Any]) -> Second: ...
            //
            // For `Top[Pair[int, str]] -> Top[Pair[int, Second]]`, checking `first` tells us nothing
            // about `Second`; only `recursive_second` supplies `str <: Second`. Check variables in both
            // source and target arguments, since contravariant callable parameters can reverse the
            // comparison. Also look through aliases: given `type Identity[T] = T`, the argument
            // `Identity[Second]` still needs evidence for `Second`.
            //
            // Merely mentioning a variable is not enough: a skipped member may add its other bound.
            // For example:
            //
            //   class Invariant[T](Protocol):
            //       marker: Any
            //       @property
            //       def value(self) -> T: ...
            //       def consume(self, other: Invariant[T]) -> None: ...
            //
            // Comparing `Top[Invariant[str]]` with `Top[Invariant[T]]`, `value` supplies `str <: T`,
            // but `consume` also requires `T <: str`. The nominal comparison requires both bounds
            // because `T` is invariant. Requiring the finite constraints to imply that comparison
            // catches the missing bound: allowing every supertype of `str` is not enough to prove
            // `T` must equal `str`.
            let mut valid = true;
            if finite.materialized {
                for argument in fields
                    .specialization_types(fields.alias_specialization(finite.target_alias))
                    .iter()
                    .chain(
                        fields
                            .specialization_types(fields.alias_specialization(finite.source_alias)),
                    )
                {
                    effects.checkpoint(ProtocolRelationWork::Argument).await?;
                    if effects
                        .argument_has_unmentioned_typevar(pending.checker, *argument, structural)
                        .await?
                    {
                        valid = false;
                        break;
                    }
                }
                if valid {
                    let implication = effects
                        .imply_constraints(
                            pending.checker.constraints,
                            structural,
                            nominally_satisfied,
                        )
                        .await?;
                    valid = effects
                        .is_always_satisfied(pending.checker, implication)
                        .await?;
                }
            }
            if valid
                && (pending.checker.typevar_evaluation == TypeVarEvaluation::Lazy
                    || effects
                        .is_never_satisfied(pending.checker, structural)
                        .await?)
            {
                return effects
                    .combine_constraints(
                        pending.checker.constraints,
                        ConstraintFoldKind::Any,
                        nominal_result,
                        structural,
                    )
                    .await
                    .map(ProtocolRelationStep::Complete);
            }
            if can_use_nominal_redundancy {
                return Ok(ProtocolRelationStep::Complete(nominally_satisfied));
            }
            protocol_relation_structural_with(fields, input, nominal_result, effects).await
        }
    }
}

struct StructuralMembers<'checker, 'a, 'c, 'db> {
    input: ProtocolInput<'checker, 'a, 'c, 'db>,
    nominal_result: ConstraintSet<'db, 'c>,
    members: InterfaceMembers<'db>,
    fold: ConstraintFold<'db, 'c>,
}

#[ty_mapping_probe_macros::dual_protocol_relation]
async fn protocol_relation_structural_next_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    members: StructuralMembers<'checker, 'a, 'c, 'db>,
    effects: &E,
) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {
    effects.checkpoint(ProtocolRelationWork::Transition).await?;
    let mut members = members;
    if let Some(member) = effects.next_interface_member(&mut members.members).await? {
        return Ok(ProtocolRelationStep::Member(PendingProtocolMember {
            checker: members.input.checker,
            ty: members.input.ty,
            member,
            continuation: MemberContinuation::Structural(members),
        }));
    }
    let result = effects.finish_constraints(&mut members.fold).await?;
    protocol_relation_finish_with(
        fields,
        members.input,
        members.nominal_result,
        result,
        effects,
    )
    .await
}

struct NominalRecursiveMembers<'checker, 'a, 'c, 'db> {
    input: ProtocolInput<'checker, 'a, 'c, 'db>,
    nominally_satisfied: ConstraintSet<'db, 'c>,
    members: Vec<(StructuralMemberPriority, ProtocolMember<'db, 'db>)>,
    index: usize,
    first_recursive: usize,
    fold: ConstraintFold<'db, 'c>,
}

#[ty_mapping_probe_macros::dual_protocol_relation]
async fn protocol_relation_nominal_finite_next_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    members: NominalRecursiveMembers<'checker, 'a, 'c, 'db>,
    effects: &E,
) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {
    effects.checkpoint(ProtocolRelationWork::Transition).await?;
    let mut members = members;
    if members.index < members.first_recursive {
        let member = effects
            .advance_prioritized_member(&members.members, &mut members.index)
            .await?;
        return Ok(ProtocolRelationStep::Member(PendingProtocolMember {
            checker: members.input.checker,
            ty: members.input.ty,
            member,
            continuation: MemberContinuation::NominalFinite(members),
        }));
    }
    // The original finite when_all is balanced; recursive members then extend its result
    // in source order, stopping as soon as the nominal proof adds no further restrictions.
    let replacement =
        ConstraintFold::new(members.input.checker.constraints, ConstraintFoldKind::All);
    let mut fold = std::mem::replace(&mut members.fold, replacement);
    let structural = effects.finish_constraints(&mut fold).await?;
    protocol_relation_nominal_recursive_next_with(fields, members, structural, effects).await
}

#[ty_mapping_probe_macros::dual_protocol_relation]
async fn protocol_relation_nominal_recursive_next_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    members: NominalRecursiveMembers<'checker, 'a, 'c, 'db>,
    structural: ConstraintSet<'db, 'c>,
    effects: &E,
) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {
    effects.checkpoint(ProtocolRelationWork::Transition).await?;
    let mut members = members;
    let checker = members.input.checker;
    while members.index < members.members.len() {
        let member = effects
            .advance_prioritized_member(&members.members, &mut members.index)
            .await?;
        let implication = effects
            .imply_constraints(checker.constraints, structural, members.nominally_satisfied)
            .await?;
        if effects.is_always_satisfied(checker, implication).await? {
            break;
        }
        if structural.is_trivially_never_satisfied() {
            continue;
        }
        return Ok(ProtocolRelationStep::Member(PendingProtocolMember {
            checker,
            ty: members.input.ty,
            member,
            continuation: MemberContinuation::NominalRecursive {
                members,
                structural,
            },
        }));
    }
    protocol_relation_finish_with(
        fields,
        members.input,
        members.nominally_satisfied,
        structural,
        effects,
    )
    .await
}

pub(in crate::types) struct PendingProtocolMember<'checker, 'a, 'c, 'db> {
    pub(in crate::types) checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    pub(in crate::types) ty: Type<'db>,
    pub(in crate::types) member: ProtocolMember<'db, 'db>,
    continuation: MemberContinuation<'checker, 'a, 'c, 'db>,
}

enum MemberContinuation<'checker, 'a, 'c, 'db> {
    Structural(StructuralMembers<'checker, 'a, 'c, 'db>),
    NominalFinite(NominalRecursiveMembers<'checker, 'a, 'c, 'db>),
    NominalRecursive {
        members: NominalRecursiveMembers<'checker, 'a, 'c, 'db>,
        structural: ConstraintSet<'db, 'c>,
    },
}

impl<'checker, 'a, 'c, 'db> PendingProtocolMember<'checker, 'a, 'c, 'db> {
    pub(in crate::types) fn resume<D: RelationDependencies>(
        self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, D::Error> {
        protocol_relation_member_resume_sync(
            RelationFieldReads::new(db),
            self,
            result,
            &InlineProtocolRelationEffects::new(db, dependencies),
        )
    }
}

#[ty_mapping_probe_macros::dual_protocol_relation]
async fn protocol_relation_member_resume_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    pending: PendingProtocolMember<'checker, 'a, 'c, 'db>,
    result: ConstraintSet<'db, 'c>,
    effects: &E,
) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {
    effects.checkpoint(ProtocolRelationWork::Transition).await?;
    match pending.continuation {
        MemberContinuation::Structural(mut members) => {
            match effects.push_constraints(&mut members.fold, result).await? {
                ControlFlow::Break(result) => {
                    protocol_relation_finish_with(
                        fields,
                        members.input,
                        members.nominal_result,
                        result,
                        effects,
                    )
                    .await
                }
                ControlFlow::Continue(()) => {
                    protocol_relation_structural_next_with(fields, members, effects).await
                }
            }
        }
        MemberContinuation::NominalFinite(mut members) => {
            match effects.push_constraints(&mut members.fold, result).await? {
                ControlFlow::Break(result) => {
                    members.index = members.first_recursive;
                    protocol_relation_nominal_recursive_next_with(fields, members, result, effects)
                        .await
                }
                ControlFlow::Continue(()) => {
                    protocol_relation_nominal_finite_next_with(fields, members, effects).await
                }
            }
        }
        MemberContinuation::NominalRecursive {
            members,
            structural,
        } => {
            let combined = effects
                .combine_constraints(
                    pending.checker.constraints,
                    ConstraintFoldKind::All,
                    structural,
                    result,
                )
                .await?;
            protocol_relation_nominal_recursive_next_with(fields, members, combined, effects).await
        }
    }
}

pub(in crate::types) struct PendingMetaBindings<'checker, 'a, 'c, 'db> {
    pub(in crate::types) checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    pub(in crate::types) constructor_ty: Type<'db>,
    meta_ty: Type<'db>,
    protocol: ProtocolInstanceType<'db>,
}

impl<'checker, 'a, 'c, 'db> PendingMetaBindings<'checker, 'a, 'c, 'db> {
    pub(in crate::types) fn resume<D: RelationDependencies>(
        self,
        db: &'db dyn Db,
        bindings: &Bindings<'db>,
        dependencies: &D,
    ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, D::Error> {
        protocol_relation_meta_bindings_resume_sync(
            RelationFieldReads::new(db),
            self,
            bindings,
            &InlineProtocolRelationEffects::new(db, dependencies),
        )
    }
}

#[ty_mapping_probe_macros::dual_protocol_relation]
async fn protocol_relation_meta_bindings_resume_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    pending: PendingMetaBindings<'checker, 'a, 'c, 'db>,
    bindings: &Bindings<'db>,
    effects: &E,
) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {
    let _ = fields;
    effects.checkpoint(ProtocolRelationWork::Transition).await?;
    let instance_ty = effects
        .bindings_return_type(pending.checker, bindings)
        .await?;
    Ok(ProtocolRelationStep::Relate(PendingProtocolPair {
        checker: pending.checker,
        source: instance_ty,
        target: Type::ProtocolInstance(pending.protocol),
        continuation: PairContinuation::Meta {
            meta_ty: pending.meta_ty,
            protocol: pending.protocol,
        },
    }))
}

pub(in crate::types) struct PendingMetaMembers<'checker, 'a, 'c, 'db> {
    pub(in crate::types) checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    pub(in crate::types) instance_ty: Type<'db>,
    pub(in crate::types) meta_ty: Type<'db>,
    pub(in crate::types) protocol: ProtocolInstanceType<'db>,
    instance_result: ConstraintSet<'db, 'c>,
}

impl<'checker, 'a, 'c, 'db> PendingMetaMembers<'checker, 'a, 'c, 'db> {
    pub(in crate::types) fn resume<D: RelationDependencies>(
        self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, D::Error> {
        protocol_relation_meta_members_resume_sync(
            RelationFieldReads::new(db),
            self,
            result,
            &InlineProtocolRelationEffects::new(db, dependencies),
        )
    }
}

#[ty_mapping_probe_macros::dual_protocol_relation]
async fn protocol_relation_meta_members_resume_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    pending: PendingMetaMembers<'checker, 'a, 'c, 'db>,
    result: ConstraintSet<'db, 'c>,
    effects: &E,
) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {
    let _ = fields;
    effects.checkpoint(ProtocolRelationWork::Transition).await?;
    effects
        .combine_constraints(
            pending.checker.constraints,
            ConstraintFoldKind::All,
            pending.instance_result,
            result,
        )
        .await
        .map(ProtocolRelationStep::Complete)
}

#[ty_mapping_probe_macros::dual_protocol_relation]
pub(in crate::types) async fn check_type_satisfies_protocol_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    ty: Type<'db>,
    protocol: ProtocolInstanceType<'db>,
    effects: &E,
) -> Result<ConstraintSet<'db, 'c>, E::Error> {
    effects.checkpoint(ProtocolRelationWork::Entry).await?;
    let step = protocol_relation_start_with(fields, checker, ty, protocol, effects).await?;
    protocol_relation_run_with(fields, step, effects).await
}
#[ty_mapping_probe_macros::dual_protocol_relation]
pub(in crate::types) async fn check_meta_type_satisfies_protocol_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    meta_ty: Type<'db>,
    protocol: ProtocolInstanceType<'db>,
    effects: &E,
) -> Result<ConstraintSet<'db, 'c>, E::Error> {
    effects.checkpoint(ProtocolRelationWork::Entry).await?;
    let step =
        protocol_relation_start_meta_with(fields, checker, meta_ty, protocol, effects).await?;
    protocol_relation_run_with(fields, step, effects).await
}
#[ty_mapping_probe_macros::dual_protocol_relation]
async fn protocol_relation_run_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    fields: RelationFieldReads<'db>,
    step: ProtocolRelationStep<'checker, 'a, 'c, 'db>,
    effects: &E,
) -> Result<ConstraintSet<'db, 'c>, E::Error> {
    let mut step = step;
    loop {
        effects.checkpoint(ProtocolRelationWork::Transition).await?;
        step = match step {
            ProtocolRelationStep::Complete(result) => return Ok(result),
            ProtocolRelationStep::Relate(pending) => {
                let result = effects
                    .check_type_pair(pending.checker, pending.source, pending.target)
                    .await?;
                protocol_relation_pair_resume_with(fields, pending, result, effects).await?
            }
            ProtocolRelationStep::Interface(pending) => {
                let result = effects
                    .check_protocol_interface(
                        pending.checker,
                        pending.source_type,
                        pending.source,
                        pending.target,
                    )
                    .await?;
                protocol_relation_interface_resume_with(fields, pending, result, effects).await?
            }
            ProtocolRelationStep::Member(pending) => {
                let result = effects
                    .check_protocol_member(pending.checker, pending.ty, pending.member)
                    .await?;
                protocol_relation_member_resume_with(fields, pending, result, effects).await?
            }
            ProtocolRelationStep::MetaBindings(pending) => {
                let bindings = effects
                    .bindings(pending.checker, pending.constructor_ty)
                    .await?;
                protocol_relation_meta_bindings_resume_with(fields, pending, &bindings, effects)
                    .await?
            }
            ProtocolRelationStep::MetaMembers(pending) => {
                let result = effects
                    .check_meta_protocol_members(
                        pending.checker,
                        pending.instance_ty,
                        pending.meta_ty,
                        pending.protocol,
                    )
                    .await?;
                protocol_relation_meta_members_resume_with(fields, pending, result, effects).await?
            }
        };
    }
}

pub(super) fn ordinary<'c, 'db>(
    db: &'db dyn Db,
    step: Result<ProtocolRelationStep<'_, '_, 'c, 'db>, std::convert::Infallible>,
) -> ConstraintSet<'db, 'c> {
    let step = match step {
        Ok(step) => step,
        Err(never) => match never {},
    };
    match protocol_relation_run_sync(
        RelationFieldReads::new(db),
        step,
        &InlineProtocolRelationEffects::new(db, &OrdinaryDependencies),
    ) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}
