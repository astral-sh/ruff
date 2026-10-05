//! Ordered protocol-interface and class-side member comparisons.

use std::collections::btree_map;
use std::ops::ControlFlow;

use ruff_python_ast::name::Name;

use super::{
    ProtocolInterfaceView, ProtocolMember, ProtocolMemberAccess, ProtocolMemberAccessMode,
    ProtocolMemberData, ProtocolMemberType, StructuralMemberPriority,
    non_object_protocol_member_count,
};
use crate::Db;
use crate::types::constraints::{ConstraintFold, ConstraintFoldKind, ConstraintSet};
use crate::types::relation::dependencies::RelationDependencies;
use crate::types::relation::{RelationFieldReads, TypeRelationChecker};
use crate::types::{ErrorContext, MaterializationKind, ProtocolInstanceType, Type};

/// Borrows the interned interface, independently of the continuation that owns this cursor.
pub(in crate::types) struct InterfaceMembers<'db> {
    members: btree_map::Iter<'db, Name, ProtocolMemberData<'db>>,
    materialization: Option<MaterializationKind>,
}

impl<'db> InterfaceMembers<'db> {
    pub(in crate::types) fn new(db: &'db dyn Db, view: ProtocolInterfaceView<'db>) -> Self {
        Self::with_fields(RelationFieldReads::new(db), view)
    }

    pub(in crate::types) fn with_fields(
        fields: RelationFieldReads<'db>,
        view: ProtocolInterfaceView<'db>,
    ) -> Self {
        Self {
            members: fields.protocol_interface_members(view),
            materialization: view.materialization,
        }
    }
}

impl<'db> Iterator for InterfaceMembers<'db> {
    type Item = ProtocolMember<'db, 'db>;

    fn next(&mut self) -> Option<Self::Item> {
        self.members.next().map(|(name, data)| ProtocolMember {
            name,
            data,
            materialization: self.materialization,
        })
    }
}

pub(in crate::types) enum ProtocolInterfaceStep<'checker, 'a, 'c, 'db> {
    Complete(ConstraintSet<'db, 'c>),
    Member(PendingInterfaceMember<'checker, 'a, 'c, 'db>),
    Access(PendingInterfaceAccess<'checker, 'a, 'c, 'db>),
}

struct InterfaceComparison<'checker, 'a, 'c, 'db> {
    checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    source_type: Type<'db>,
    source: ProtocolInterfaceView<'db>,
    targets: std::vec::IntoIter<(StructuralMemberPriority, ProtocolMember<'db, 'db>)>,
    fold: ConstraintFold<'db, 'c>,
}

impl<'checker, 'a, 'c, 'db> ProtocolInterfaceStep<'checker, 'a, 'c, 'db> {
    pub(in crate::types) fn start<D: RelationDependencies>(
        db: &'db dyn Db,
        checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
        source_type: Type<'db>,
        source: ProtocolInterfaceView<'db>,
        target: ProtocolInterfaceView<'db>,
        dependencies: &D,
    ) -> Result<Self, D::Error> {
        if source.member_count(db) < target.member_count(db)
            && !checker.is_context_collection_enabled()
            && dependencies.run(db, || {
                source.member_count(db) < non_object_protocol_member_count(db, target.interface)
            })?
        {
            return Ok(Self::Complete(checker.never()));
        }
        let mut targets = dependencies.run(db, || Vec::with_capacity(target.member_count(db)))?;
        for member in InterfaceMembers::new(db, target) {
            let priority =
                dependencies.run(db, || member.structural_member_priority(db, checker.env))?;
            dependencies.run(db, || targets.push((priority, member)))?;
        }
        // The stable ordering preserves the declaration order of equal-priority requirements.
        dependencies.run(db, || {
            targets.sort_by(|(left, _), (right, _)| left.cmp(right));
        })?;
        InterfaceComparison {
            checker,
            source_type,
            source,
            targets: targets.into_iter(),
            fold: ConstraintFold::new(checker.constraints, ConstraintFoldKind::All),
        }
        .next(db, dependencies)
    }
}

impl<'checker, 'a, 'c, 'db> InterfaceComparison<'checker, 'a, 'c, 'db> {
    fn next<D: RelationDependencies>(
        mut self,
        db: &'db dyn Db,
        dependencies: &D,
    ) -> Result<ProtocolInterfaceStep<'checker, 'a, 'c, 'db>, D::Error> {
        let Some((_, target_member)) = dependencies.run(db, || self.targets.next())? else {
            return dependencies
                .run(db, || self.fold.finish())
                .map(ProtocolInterfaceStep::Complete);
        };
        let source_member = self.source.member_by_name(db, target_member.name);
        if source_member.is_none()
            && dependencies.run(db, || {
                self.source.includes_member_or_object_fallback(
                    db,
                    self.checker.env,
                    target_member.name,
                )
            })?
        {
            return Ok(ProtocolInterfaceStep::Member(PendingInterfaceMember {
                checker: self.checker,
                source_type: self.source_type,
                target_member,
                comparison: self,
            }));
        }
        if let Some(context) = self.checker.report_context()
            && source_member.is_none()
        {
            context.push(ErrorContext::ProtocolMemberNotDefined {
                member_name: target_member.name.into(),
                ty: self.source_type,
            });
            let result = self.checker.never();
            return self.push(db, result, dependencies);
        }
        let Some(source_member) = source_member else {
            let result = self.checker.never();
            return self.after_member(db, target_member, result, dependencies);
        };
        Ok(ProtocolInterfaceStep::Access(PendingInterfaceAccess {
            checker: self.checker,
            source_type: self.source_type,
            source_member,
            target_member,
            access: ProtocolMemberAccessMode::Instance,
            instance_result: None,
            comparison: self,
        }))
    }

    fn after_member<D: RelationDependencies>(
        self,
        db: &'db dyn Db,
        member: ProtocolMember<'db, 'db>,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolInterfaceStep<'checker, 'a, 'c, 'db>, D::Error> {
        if let Some(context) = self.checker.report_context()
            && dependencies.run(db, || result.is_never_satisfied(db, self.checker.env))?
        {
            context.push(ErrorContext::ProtocolMemberIncompatible {
                member_name: member.name.into(),
            });
        }
        self.push(db, result, dependencies)
    }

    fn push<D: RelationDependencies>(
        mut self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolInterfaceStep<'checker, 'a, 'c, 'db>, D::Error> {
        match dependencies.run(db, || self.fold.push(result))? {
            ControlFlow::Break(result) => Ok(ProtocolInterfaceStep::Complete(result)),
            ControlFlow::Continue(()) => self.next(db, dependencies),
        }
    }
}

pub(in crate::types) struct PendingInterfaceMember<'checker, 'a, 'c, 'db> {
    pub(in crate::types) checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    pub(in crate::types) source_type: Type<'db>,
    pub(in crate::types) target_member: ProtocolMember<'db, 'db>,
    comparison: InterfaceComparison<'checker, 'a, 'c, 'db>,
}

impl<'checker, 'a, 'c, 'db> PendingInterfaceMember<'checker, 'a, 'c, 'db> {
    pub(in crate::types) fn resume<D: RelationDependencies>(
        self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolInterfaceStep<'checker, 'a, 'c, 'db>, D::Error> {
        // The nominal-member operation already reports its own incompatibility context.
        self.comparison.push(db, result, dependencies)
    }
}

pub(in crate::types) struct PendingInterfaceAccess<'checker, 'a, 'c, 'db> {
    pub(in crate::types) checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    pub(in crate::types) source_type: Type<'db>,
    pub(in crate::types) source_member: ProtocolMember<'db, 'db>,
    pub(in crate::types) target_member: ProtocolMember<'db, 'db>,
    pub(in crate::types) access: ProtocolMemberAccessMode,
    instance_result: Option<ConstraintSet<'db, 'c>>,
    comparison: InterfaceComparison<'checker, 'a, 'c, 'db>,
}

impl<'checker, 'a, 'c, 'db> PendingInterfaceAccess<'checker, 'a, 'c, 'db> {
    pub(in crate::types) fn resume<D: RelationDependencies>(
        mut self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolInterfaceStep<'checker, 'a, 'c, 'db>, D::Error> {
        if let Some(instance_result) = self.instance_result {
            let result = dependencies.run(db, || {
                instance_result.and(db, self.checker.constraints, || result)
            })?;
            self.comparison
                .after_member(db, self.target_member, result, dependencies)
        } else if result.is_trivially_never_satisfied() {
            self.comparison
                .after_member(db, self.target_member, result, dependencies)
        } else {
            self.instance_result = Some(result);
            self.access = ProtocolMemberAccessMode::Class;
            Ok(ProtocolInterfaceStep::Access(self))
        }
    }
}

pub(in crate::types) enum MetaProtocolMembersStep<'checker, 'a, 'c, 'db> {
    Complete(ConstraintSet<'db, 'c>),
    Read(PendingMetaProtocolRead<'checker, 'a, 'c, 'db>),
    Access(PendingMetaProtocolAccess<'checker, 'a, 'c, 'db>),
}

struct MetaProtocolMembers<'checker, 'a, 'c, 'db> {
    checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    instance_ty: Type<'db>,
    meta_ty: Type<'db>,
    members: InterfaceMembers<'db>,
    fold: ConstraintFold<'db, 'c>,
}

impl<'checker, 'a, 'c, 'db> MetaProtocolMembersStep<'checker, 'a, 'c, 'db> {
    pub(in crate::types) fn start<D: RelationDependencies>(
        db: &'db dyn Db,
        checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
        instance_ty: Type<'db>,
        meta_ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
        dependencies: &D,
    ) -> Result<Self, D::Error> {
        let interface = dependencies.run(db, || protocol.interface(db))?;
        MetaProtocolMembers {
            checker,
            instance_ty,
            meta_ty,
            members: InterfaceMembers::new(db, interface),
            fold: ConstraintFold::new(checker.constraints, ConstraintFoldKind::All),
        }
        .next(db, dependencies)
    }
}

impl<'checker, 'a, 'c, 'db> MetaProtocolMembers<'checker, 'a, 'c, 'db> {
    fn next<D: RelationDependencies>(
        mut self,
        db: &'db dyn Db,
        dependencies: &D,
    ) -> Result<MetaProtocolMembersStep<'checker, 'a, 'c, 'db>, D::Error> {
        loop {
            let Some(member) = dependencies.run(db, || self.members.next())? else {
                return dependencies
                    .run(db, || self.fold.finish())
                    .map(MetaProtocolMembersStep::Complete);
            };
            let required = dependencies.run(db, || {
                member.access(db, self.checker.env, ProtocolMemberAccessMode::Class)
            })?;
            if required.read.is_none() && required.write.is_none() {
                // Preserve the fold input even when this member has no class-side requirements.
                if let ControlFlow::Break(result) =
                    dependencies.run(db, || self.fold.push(self.checker.always()))?
                {
                    return Ok(MetaProtocolMembersStep::Complete(result));
                }
                continue;
            }
            if member.is_method() {
                if let Some(required_ty) = required.read {
                    return Ok(MetaProtocolMembersStep::Read(PendingMetaProtocolRead {
                        checker: self.checker,
                        instance_ty: self.instance_ty,
                        meta_ty: self.meta_ty,
                        member,
                        required_ty,
                        members: self,
                    }));
                }
                let result = self.checker.always();
                self.report(db, member, result, dependencies)?;
                if let ControlFlow::Break(result) =
                    dependencies.run(db, || self.fold.push(result))?
                {
                    return Ok(MetaProtocolMembersStep::Complete(result));
                }
                continue;
            }
            return Ok(MetaProtocolMembersStep::Access(PendingMetaProtocolAccess {
                checker: self.checker,
                instance_ty: self.instance_ty,
                meta_ty: self.meta_ty,
                member,
                required,
                members: self,
            }));
        }
    }

    fn report<D: RelationDependencies>(
        &self,
        db: &'db dyn Db,
        member: ProtocolMember<'db, 'db>,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<(), D::Error> {
        if let Some(context) = self.checker.report_context()
            && dependencies.run(db, || result.is_never_satisfied(db, self.checker.env))?
        {
            context.push(ErrorContext::ProtocolMemberIncompatible {
                member_name: member.name.into(),
            });
        }
        Ok(())
    }

    fn resume<D: RelationDependencies>(
        mut self,
        db: &'db dyn Db,
        member: ProtocolMember<'db, 'db>,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<MetaProtocolMembersStep<'checker, 'a, 'c, 'db>, D::Error> {
        self.report(db, member, result, dependencies)?;
        match dependencies.run(db, || self.fold.push(result))? {
            ControlFlow::Break(result) => Ok(MetaProtocolMembersStep::Complete(result)),
            ControlFlow::Continue(()) => self.next(db, dependencies),
        }
    }
}

pub(in crate::types) struct PendingMetaProtocolRead<'checker, 'a, 'c, 'db> {
    pub(in crate::types) checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    pub(in crate::types) instance_ty: Type<'db>,
    pub(in crate::types) meta_ty: Type<'db>,
    pub(in crate::types) member: ProtocolMember<'db, 'db>,
    pub(in crate::types) required_ty: ProtocolMemberType<'db>,
    members: MetaProtocolMembers<'checker, 'a, 'c, 'db>,
}

impl<'checker, 'a, 'c, 'db> PendingMetaProtocolRead<'checker, 'a, 'c, 'db> {
    pub(in crate::types) fn resume<D: RelationDependencies>(
        self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<MetaProtocolMembersStep<'checker, 'a, 'c, 'db>, D::Error> {
        self.members.resume(db, self.member, result, dependencies)
    }
}

pub(in crate::types) struct PendingMetaProtocolAccess<'checker, 'a, 'c, 'db> {
    pub(in crate::types) checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    pub(in crate::types) instance_ty: Type<'db>,
    pub(in crate::types) meta_ty: Type<'db>,
    pub(in crate::types) member: ProtocolMember<'db, 'db>,
    pub(in crate::types) required: ProtocolMemberAccess<'db>,
    members: MetaProtocolMembers<'checker, 'a, 'c, 'db>,
}

impl<'checker, 'a, 'c, 'db> PendingMetaProtocolAccess<'checker, 'a, 'c, 'db> {
    pub(in crate::types) fn resume<D: RelationDependencies>(
        self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<MetaProtocolMembersStep<'checker, 'a, 'c, 'db>, D::Error> {
        self.members.resume(db, self.member, result, dependencies)
    }
}
