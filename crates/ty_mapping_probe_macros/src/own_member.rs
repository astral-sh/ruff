use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{Error, Expr, FnArg, ItemFn, Path, Result, Signature, TypeParamBound};

use super::{MatchesInput, Scope, is_identifier};

mod member_lookup;
use member_lookup::LookupManifest;

pub(super) fn expand(arguments: TokenStream, original: &TokenStream) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::OwnMember)
}

pub(super) fn expand_mro(arguments: TokenStream, original: &TokenStream) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::MroMember)
}

pub(super) fn expand_namespace_lookup(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::NamespaceLookup)
}

pub(super) fn expand_instance_mro(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::InstanceMro)
}

pub(super) fn expand_mro_root(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::MroRoot)
}

pub(super) fn expand_mro_iteration(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::MroIteration)
}

pub(super) fn expand_static_mro(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::StaticMro)
}

pub(super) fn expand_base_mro(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::BaseMro)
}

pub(super) fn expand_c3(arguments: TokenStream, original: &TokenStream) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::C3Merge)
}

pub(super) fn expand_class_type(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::ClassTypeOwnMember)
}

pub(super) fn expand_synthesized(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::SynthesizedMember)
}

pub(super) fn expand_promotion(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::PublicPromotion)
}

pub(super) fn expand_protocol_interface(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::ProtocolInterface)
}

pub(super) fn expand_protocol_object(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::ProtocolObject)
}

pub(super) fn expand_protocol_members_defined(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::ProtocolMembersDefined)
}

pub(super) fn expand_protocol_relation(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::ProtocolRelation)
}

pub(super) fn expand_satisfaction(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::Satisfaction)
}

pub(super) fn expand_constraint_type(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::ConstraintType)
}

pub(super) fn expand_sequent(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::Sequent)
}

pub(super) fn expand_member_source(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::MemberSource)
}

fn expand_member(
    arguments: TokenStream,
    original: &TokenStream,
    attribute: MemberAttribute,
) -> Result<TokenStream> {
    if !arguments.is_empty() {
        return Err(Error::new_spanned(
            arguments,
            format!("{} takes no arguments", attribute.name()),
        ));
    }

    let mut synchronous: ItemFn = syn::parse2(original.clone())?;
    let manifest = attribute.manifest(&synchronous.sig.ident)?;
    validate_signature(&synchronous.sig, manifest)?;
    if let MemberManifest::Lookup(entry) = manifest {
        entry.validate_body(&synchronous.block)?;
    }
    let mut lowerer = MemberLowerer {
        manifest,
        scope: Scope::Signature,
        in_matches: false,
        error: None,
    };
    visit_mut::visit_signature_mut(&mut lowerer, &mut synchronous.sig);
    if let Some(error) = lowerer.error.take() {
        return Err(error);
    }

    if manifest.uses_mro_fields() {
        synchronous.sig.inputs[0] = syn::parse_quote!(db: &'db dyn Db);
    }
    synchronous.sig.asyncness = None;
    synchronous.sig.ident = Ident::new(manifest.synchronous_name(), synchronous.sig.ident.span());
    for parameter in synchronous.sig.generics.type_params_mut() {
        if parameter.ident != "E" {
            continue;
        }
        for bound in &mut parameter.bounds {
            if let TypeParamBound::Trait(bound) = bound {
                bound.path = manifest.synchronous_bound(bound.path.span());
            }
        }
    }
    lowerer.scope = Scope::Body;
    lowerer.visit_block_mut(&mut synchronous.block);
    if let Some(error) = lowerer.error {
        return Err(error);
    }

    if manifest.uses_mro_fields() {
        synchronous
            .block
            .stmts
            .insert(0, syn::parse_quote!(let fields = MroFieldReads::new(db);));
        synchronous
            .block
            .stmts
            .insert(1, syn::parse_quote!(let _ = fields;));
    }
    Ok(quote! { #original #synchronous })
}

#[derive(Clone, Copy)]
enum MemberAttribute {
    InstanceStorage,
    SlotSelector,
    MemberSource,
    NamespaceLookup,
    InstanceMro,
    Sequent,
    ConstraintType,
    Satisfaction,
    OwnMember,
    MroMember,
    MroRoot,
    MroIteration,
    StaticMro,
    BaseMro,
    C3Merge,
    ClassTypeOwnMember,
    SynthesizedMember,
    PublicPromotion,
    ProtocolInterface,
    ProtocolObject,
    ProtocolRelation,
    ProtocolMembersDefined,
}

impl MemberAttribute {
    fn name(self) -> &'static str {
        match self {
            Self::MemberSource => "dual_member_source",
            Self::InstanceStorage => "dual_instance_storage",
            Self::SlotSelector => "dual_slot_selector",
            Self::NamespaceLookup => "dual_namespace_lookup",
            Self::InstanceMro => "dual_instance_mro",
            Self::Sequent => "dual_sequent",
            Self::ConstraintType => "dual_constraint_type",
            Self::Satisfaction => "dual_satisfaction",
            Self::OwnMember => "dual_own_member",
            Self::MroMember => "dual_mro_member",
            Self::MroRoot => "dual_mro_root",
            Self::MroIteration => "dual_mro_iteration",
            Self::StaticMro => "dual_static_mro",
            Self::BaseMro => "dual_base_mro",
            Self::C3Merge => "dual_c3_merge",
            Self::ClassTypeOwnMember => "dual_class_type_own_member",
            Self::SynthesizedMember => "dual_synthesized_member",
            Self::PublicPromotion => "dual_public_promotion",
            Self::ProtocolInterface => "dual_protocol_interface",
            Self::ProtocolRelation => "dual_protocol_relation",
            Self::ProtocolObject => "dual_protocol_object",
            Self::ProtocolMembersDefined => "dual_protocol_members_defined",
        }
    }

    fn manifest(self, name: &Ident) -> Result<MemberManifest> {
        if matches!(self, Self::InstanceStorage | Self::SlotSelector) {
            return LookupManifest::storage_from_name(name, matches!(self, Self::InstanceStorage))
                .map(MemberManifest::Lookup)
                .ok_or_else(|| {
                    Error::new_spanned(name, "attribute requires a declared storage or slot body")
                });
        }

        if matches!(self, Self::MemberSource) {
            return member_lookup::LookupManifest::source_from_name(name)
                .map(MemberManifest::Lookup)
                .ok_or_else(|| {
                    Error::new_spanned(name, "dual_member_source requires a declared source body")
                });
        }
        if matches!(self, Self::Sequent) {
            return SequentManifest::from_name(name)
                .map(MemberManifest::Sequent)
                .ok_or_else(|| {
                    Error::new_spanned(name, "dual_sequent requires a declared sequent body")
                });
        }
        match self {
            Self::NamespaceLookup if name == "namespace_lookup_with" => {
                Ok(MemberManifest::Lookup(LookupManifest::Namespace))
            }
            Self::InstanceMro if name == "mro_instance_member_with" => {
                Ok(MemberManifest::Lookup(LookupManifest::InstanceMro))
            }
            Self::ConstraintType if name == "constraint_as_concrete_with" => {
                Ok(MemberManifest::ConstraintConcrete)
            }
            Self::ConstraintType if name == "constraint_bound_depth_with" => {
                Ok(MemberManifest::ConstraintDepth)
            }
            Self::ConstraintType if name == "cached_constraint_bound_depth_with" => {
                Ok(MemberManifest::ConstraintCachedDepth)
            }
            Self::Satisfaction if name == "node_satisfaction_with" => {
                Ok(MemberManifest::SatisfactionRoot)
            }
            Self::Satisfaction if name == "simple_conjunction_satisfiable_with" => {
                Ok(MemberManifest::SatisfactionConjunction)
            }
            Self::Satisfaction if name == "path_assignments_with" => {
                Ok(MemberManifest::SatisfactionAssignments)
            }
            Self::Satisfaction if name == "path_visit_owned_with" => {
                Ok(MemberManifest::PathVisitOwned)
            }
            Self::Satisfaction if name == "path_visit_body_with" => {
                Ok(MemberManifest::PathVisitBody)
            }
            Self::Satisfaction if name == "path_enter_edge_with" => {
                Ok(MemberManifest::PathEnterEdge)
            }
            Self::Satisfaction if name == "path_drain_assignments_with" => {
                Ok(MemberManifest::PathDrainAssignments)
            }
            Self::Satisfaction if name == "path_add_assignment_with" => {
                Ok(MemberManifest::PathAddAssignment)
            }
            Self::Satisfaction if name == "path_discover_constraint_with" => {
                Ok(MemberManifest::PathDiscoverConstraint)
            }
            Self::Satisfaction if name == "path_import_sequents_with" => {
                Ok(MemberManifest::PathImportSequents)
            }
            Self::Satisfaction if name == "path_import_slice_with" => {
                Ok(MemberManifest::PathImportSlice)
            }
            Self::Satisfaction if name == "path_import_sequent_with" => {
                Ok(MemberManifest::PathImportSequent)
            }
            Self::Satisfaction if name == "path_check_sequent_with" => {
                Ok(MemberManifest::PathCheckSequent)
            }
            Self::Satisfaction if name == "path_check_pair_implication_with" => {
                Ok(MemberManifest::PathCheckPairImplication)
            }
            Self::Satisfaction if name == "path_check_single_implication_with" => {
                Ok(MemberManifest::PathCheckSingleImplication)
            }
            Self::OwnMember if name == "own_class_member_with" => Ok(MemberManifest::OwnMember),
            Self::MroMember if name == "mro_class_member_with" => Ok(MemberManifest::MroClass),
            Self::MroMember if name == "finalize_class_member_with" => {
                Ok(MemberManifest::FinalizeClass)
            }
            Self::MroRoot if name == "apply_optional_class_specialization_with" => {
                Ok(MemberManifest::OptionalClassSpecialization)
            }
            Self::MroRoot if name == "mro_first_with" => Ok(MemberManifest::MroFirst),
            Self::MroRoot if name == "mro_tail_request_with" => Ok(MemberManifest::MroTailRequest),
            Self::MroIteration if name == "mro_next_with" => Ok(MemberManifest::MroNext),
            Self::StaticMro if name == "static_mro_cycle_with" => {
                Ok(MemberManifest::StaticMroCycle)
            }
            Self::StaticMro if name == "static_mro_with" => Ok(MemberManifest::StaticMro),
            Self::StaticMro if name == "maybe_add_generic_with" => {
                Ok(MemberManifest::MaybeAddGeneric)
            }
            Self::StaticMro if name == "base_has_cyclic_mro_with" => {
                Ok(MemberManifest::BaseHasCyclicMro)
            }
            Self::BaseMro if name == "base_cursor_next_with" => Ok(MemberManifest::BaseCursorNext),
            Self::BaseMro if name == "collect_start_with" => Ok(MemberManifest::CollectStart),
            Self::BaseMro if name == "collect_start_with_root_with" => {
                Ok(MemberManifest::CollectStartWithRoot)
            }
            Self::BaseMro if name == "collect_class_literals_with" => {
                Ok(MemberManifest::CollectClassLiterals)
            }
            Self::BaseMro if name == "class_mro_start_with" => Ok(MemberManifest::ClassMroStart),
            Self::BaseMro if name == "base_mro_start_with" => Ok(MemberManifest::BaseMroStart),
            Self::BaseMro if name == "collect_base_mro_with" => Ok(MemberManifest::CollectBaseMro),
            Self::BaseMro if name == "collect_single_base_mro_with" => {
                Ok(MemberManifest::CollectSingleBaseMro)
            }
            Self::C3Merge if name == "c3_merge_with" => Ok(MemberManifest::C3Merge),
            Self::ClassTypeOwnMember if name == "class_type_own_member_with" => {
                Ok(MemberManifest::ClassTypeOwnMember)
            }
            Self::SynthesizedMember if name == "own_synthesized_member_with" => {
                Ok(MemberManifest::SynthesizedMember)
            }
            Self::PublicPromotion if name == "promote_public_with" => {
                Ok(MemberManifest::PublicPromotion)
            }
            Self::PublicPromotion if name == "promote_singletons_impl_with" => {
                Ok(MemberManifest::SingletonPromotion)
            }
            Self::ProtocolInterface if name == "for_each_protocol_member_candidate_with" => {
                Ok(MemberManifest::ProtocolCandidates)
            }
            Self::ProtocolInterface if name == "protocol_interface_candidate_with" => {
                Ok(MemberManifest::ProtocolInterfaceCandidate)
            }
            Self::ProtocolInterface if name == "protocol_interface_normalize_with" => {
                Ok(MemberManifest::ProtocolInterfaceNormalize)
            }
            Self::ProtocolInterface if name == "protocol_interface_build_with" => {
                Ok(MemberManifest::ProtocolInterfaceBuild)
            }
            Self::ProtocolRelation if name == "check_type_satisfies_protocol_with" => {
                Ok(MemberManifest::ProtocolRelationDirect)
            }
            Self::ProtocolRelation if name == "check_meta_type_satisfies_protocol_with" => {
                Ok(MemberManifest::ProtocolRelationMeta)
            }
            Self::ProtocolRelation if name == "protocol_relation_run_with" => {
                Ok(MemberManifest::ProtocolRelationRun)
            }
            Self::ProtocolRelation if name == "protocol_relation_start_with" => {
                Ok(MemberManifest::ProtocolRelationStart)
            }
            Self::ProtocolRelation if name == "protocol_relation_start_meta_with" => {
                Ok(MemberManifest::ProtocolRelationStartMeta)
            }
            Self::ProtocolRelation if name == "protocol_relation_pair_resume_with" => {
                Ok(MemberManifest::ProtocolRelationPairResume)
            }
            Self::ProtocolRelation if name == "protocol_relation_after_nominal_with" => {
                Ok(MemberManifest::ProtocolRelationAfterNominal)
            }
            Self::ProtocolRelation if name == "protocol_relation_non_recursive_interface_with" => {
                Ok(MemberManifest::ProtocolRelationNonRecursiveInterface)
            }
            Self::ProtocolRelation if name == "protocol_relation_structural_with" => {
                Ok(MemberManifest::ProtocolRelationStructural)
            }
            Self::ProtocolRelation
                if name == "protocol_relation_nominal_recursive_members_with" =>
            {
                Ok(MemberManifest::ProtocolRelationNominalRecursiveMembers)
            }
            Self::ProtocolRelation if name == "protocol_relation_finish_with" => {
                Ok(MemberManifest::ProtocolRelationFinish)
            }
            Self::ProtocolRelation if name == "protocol_relation_interface_resume_with" => {
                Ok(MemberManifest::ProtocolRelationInterfaceResume)
            }
            Self::ProtocolRelation if name == "protocol_relation_structural_next_with" => {
                Ok(MemberManifest::ProtocolRelationStructuralNext)
            }
            Self::ProtocolRelation if name == "protocol_relation_nominal_finite_next_with" => {
                Ok(MemberManifest::ProtocolRelationNominalFiniteNext)
            }
            Self::ProtocolRelation if name == "protocol_relation_nominal_recursive_next_with" => {
                Ok(MemberManifest::ProtocolRelationNominalRecursiveNext)
            }
            Self::ProtocolRelation if name == "protocol_relation_member_resume_with" => {
                Ok(MemberManifest::ProtocolRelationMemberResume)
            }
            Self::ProtocolRelation if name == "protocol_relation_meta_bindings_resume_with" => {
                Ok(MemberManifest::ProtocolRelationMetaBindingsResume)
            }
            Self::ProtocolRelation if name == "protocol_relation_meta_members_resume_with" => {
                Ok(MemberManifest::ProtocolRelationMetaMembersResume)
            }
            Self::ProtocolRelation if name == "protocol_object_compare_with" => {
                Ok(MemberManifest::ProtocolRelationObjectCompare)
            }
            Self::ProtocolObject if name == "protocol_object_equivalence_with" => {
                Ok(MemberManifest::ProtocolObject)
            }
            Self::ProtocolMembersDefined if name == "protocol_members_defined_with" => {
                Ok(MemberManifest::ProtocolMembersDefined)
            }
            _ => Err(Error::new_spanned(
                name,
                format!("this function is not in the {} manifest", self.name()),
            )),
        }
    }
}

#[derive(Clone, Copy)]
enum MemberManifest {
    Lookup(LookupManifest),
    Sequent(SequentManifest),
    ConstraintConcrete,
    ConstraintDepth,
    ConstraintCachedDepth,
    SatisfactionRoot,
    SatisfactionConjunction,
    SatisfactionAssignments,
    PathVisitOwned,
    PathVisitBody,
    PathEnterEdge,
    PathDrainAssignments,
    PathAddAssignment,
    PathDiscoverConstraint,
    PathImportSequents,
    PathImportSlice,
    PathImportSequent,
    PathCheckSequent,
    PathCheckPairImplication,
    PathCheckSingleImplication,
    OwnMember,
    MroClass,
    FinalizeClass,
    OptionalClassSpecialization,
    MroFirst,
    MroTailRequest,
    MroNext,
    StaticMroCycle,
    BaseCursorNext,
    CollectStart,
    CollectStartWithRoot,
    CollectClassLiterals,
    StaticMro,
    MaybeAddGeneric,
    BaseHasCyclicMro,
    ClassMroStart,
    BaseMroStart,
    CollectBaseMro,
    CollectSingleBaseMro,
    C3Merge,
    ClassTypeOwnMember,
    SynthesizedMember,
    PublicPromotion,
    SingletonPromotion,
    ProtocolCandidates,
    ProtocolInterfaceCandidate,
    ProtocolInterfaceBuild,
    ProtocolInterfaceNormalize,
    ProtocolObject,
    ProtocolMembersDefined,
    ProtocolRelationDirect,
    ProtocolRelationMeta,
    ProtocolRelationRun,
    ProtocolRelationStart,
    ProtocolRelationStartMeta,
    ProtocolRelationPairResume,
    ProtocolRelationAfterNominal,
    ProtocolRelationNonRecursiveInterface,
    ProtocolRelationStructural,
    ProtocolRelationNominalRecursiveMembers,
    ProtocolRelationFinish,
    ProtocolRelationInterfaceResume,
    ProtocolRelationStructuralNext,
    ProtocolRelationNominalFiniteNext,
    ProtocolRelationNominalRecursiveNext,
    ProtocolRelationMemberResume,
    ProtocolRelationMetaBindingsResume,
    ProtocolRelationMetaMembersResume,
    ProtocolRelationObjectCompare,
}

impl MemberManifest {
    fn uses_mro_fields(self) -> bool {
        matches!(
            self,
            Self::OptionalClassSpecialization
                | Self::MroFirst
                | Self::MroTailRequest
                | Self::MroNext
                | Self::StaticMro
                | Self::StaticMroCycle
                | Self::BaseHasCyclicMro
                | Self::ClassMroStart
                | Self::BaseMroStart
                | Self::CollectBaseMro
                | Self::CollectSingleBaseMro
                | Self::C3Merge
                | Self::BaseCursorNext
                | Self::CollectStart
                | Self::CollectStartWithRoot
                | Self::CollectClassLiterals
        )
    }

    fn attribute(self) -> MemberAttribute {
        match self {
            Self::Lookup(entry) => entry.attribute(),
            Self::Sequent(_) => MemberAttribute::Sequent,
            Self::ConstraintConcrete => MemberAttribute::ConstraintType,
            Self::ConstraintDepth => MemberAttribute::ConstraintType,
            Self::ConstraintCachedDepth => MemberAttribute::ConstraintType,
            Self::SatisfactionRoot
            | Self::SatisfactionConjunction
            | Self::SatisfactionAssignments
            | Self::PathVisitOwned
            | Self::PathVisitBody
            | Self::PathEnterEdge
            | Self::PathDrainAssignments
            | Self::PathAddAssignment
            | Self::PathDiscoverConstraint
            | Self::PathImportSequents
            | Self::PathImportSlice
            | Self::PathImportSequent
            | Self::PathCheckSequent
            | Self::PathCheckPairImplication
            | Self::PathCheckSingleImplication => MemberAttribute::Satisfaction,
            Self::OwnMember => MemberAttribute::OwnMember,
            Self::MroClass | Self::FinalizeClass => MemberAttribute::MroMember,
            Self::OptionalClassSpecialization | Self::MroFirst | Self::MroTailRequest => {
                MemberAttribute::MroRoot
            }
            Self::MroNext => MemberAttribute::MroIteration,
            Self::StaticMro
            | Self::StaticMroCycle
            | Self::MaybeAddGeneric
            | Self::BaseHasCyclicMro => MemberAttribute::StaticMro,
            Self::BaseCursorNext
            | Self::CollectStart
            | Self::CollectStartWithRoot
            | Self::CollectClassLiterals => {
                MemberAttribute::BaseMro
            }
            Self::ClassMroStart
            | Self::BaseMroStart
            | Self::CollectBaseMro
            | Self::CollectSingleBaseMro => MemberAttribute::BaseMro,
            Self::C3Merge => MemberAttribute::C3Merge,
            Self::ClassTypeOwnMember => MemberAttribute::ClassTypeOwnMember,
            Self::SynthesizedMember => MemberAttribute::SynthesizedMember,
            Self::PublicPromotion | Self::SingletonPromotion => MemberAttribute::PublicPromotion,
            Self::ProtocolCandidates
            | Self::ProtocolInterfaceCandidate
            | Self::ProtocolInterfaceBuild
            | Self::ProtocolInterfaceNormalize => MemberAttribute::ProtocolInterface,
            Self::ProtocolRelationDirect
            | Self::ProtocolRelationMeta
            | Self::ProtocolRelationRun
            | Self::ProtocolRelationStart
            | Self::ProtocolRelationStartMeta
            | Self::ProtocolRelationPairResume
            | Self::ProtocolRelationAfterNominal
            | Self::ProtocolRelationNonRecursiveInterface
            | Self::ProtocolRelationStructural
            | Self::ProtocolRelationNominalRecursiveMembers
            | Self::ProtocolRelationFinish
            | Self::ProtocolRelationInterfaceResume
            | Self::ProtocolRelationStructuralNext
            | Self::ProtocolRelationNominalFiniteNext
            | Self::ProtocolRelationNominalRecursiveNext
            | Self::ProtocolRelationMemberResume
            | Self::ProtocolRelationMetaBindingsResume
            | Self::ProtocolRelationMetaMembersResume
            | Self::ProtocolRelationObjectCompare => MemberAttribute::ProtocolRelation,
            Self::ProtocolObject => MemberAttribute::ProtocolObject,
            Self::ProtocolMembersDefined => MemberAttribute::ProtocolMembersDefined,
        }
    }

    fn body_name(self) -> &'static str {
        match self {
            Self::Lookup(entry) => entry.body_name(),
            Self::Sequent(entry) => entry.body_name(),
            Self::ConstraintConcrete => "constraint_as_concrete_with",
            Self::ConstraintDepth => "constraint_bound_depth_with",
            Self::ConstraintCachedDepth => "cached_constraint_bound_depth_with",
            Self::SatisfactionRoot => "node_satisfaction_with",
            Self::SatisfactionConjunction => "simple_conjunction_satisfiable_with",
            Self::SatisfactionAssignments => "path_assignments_with",
            Self::PathVisitOwned => "path_visit_owned_with",
            Self::PathVisitBody => "path_visit_body_with",
            Self::PathEnterEdge => "path_enter_edge_with",
            Self::PathDrainAssignments => "path_drain_assignments_with",
            Self::PathAddAssignment => "path_add_assignment_with",
            Self::PathDiscoverConstraint => "path_discover_constraint_with",
            Self::PathImportSequents => "path_import_sequents_with",
            Self::PathImportSlice => "path_import_slice_with",
            Self::PathImportSequent => "path_import_sequent_with",
            Self::PathCheckSequent => "path_check_sequent_with",
            Self::PathCheckPairImplication => "path_check_pair_implication_with",
            Self::PathCheckSingleImplication => "path_check_single_implication_with",
            Self::StaticMroCycle => "static MRO cycle seed",
            Self::BaseCursorNext => "base MRO cursor",
            Self::CollectStart | Self::CollectStartWithRoot => "base MRO collection",
            Self::CollectClassLiterals => "class-literal MRO collection",
            Self::OwnMember => "own-member",
            Self::MroClass => "MRO member",
            Self::FinalizeClass => "member finalization",
            Self::OptionalClassSpecialization | Self::MroFirst | Self::MroTailRequest => "MRO root",
            Self::MroNext => "MRO iteration",
            Self::StaticMro | Self::MaybeAddGeneric | Self::BaseHasCyclicMro => "static MRO",
            Self::ClassMroStart
            | Self::BaseMroStart
            | Self::CollectBaseMro
            | Self::CollectSingleBaseMro => "base MRO",
            Self::C3Merge => "C3 merge",
            Self::ClassTypeOwnMember => "ClassType own-member",
            Self::SynthesizedMember => "synthesized-member",
            Self::PublicPromotion => "public promotion",
            Self::SingletonPromotion => "singleton promotion",
            Self::ProtocolCandidates => "protocol candidates",
            Self::ProtocolInterfaceCandidate => "protocol-interface candidate",
            Self::ProtocolInterfaceBuild => "protocol-interface construction",
            Self::ProtocolInterfaceNormalize => "protocol-interface normalization",
            Self::ProtocolRelationDirect => "check type satisfies protocol",
            Self::ProtocolRelationMeta => "check meta type satisfies protocol",
            Self::ProtocolRelationRun => "protocol relation run",
            Self::ProtocolRelationStart => "protocol relation start",
            Self::ProtocolRelationStartMeta => "protocol relation start meta",
            Self::ProtocolRelationPairResume => "protocol relation pair resume",
            Self::ProtocolRelationAfterNominal => "protocol relation after nominal",
            Self::ProtocolRelationNonRecursiveInterface => {
                "protocol relation non recursive interface"
            }
            Self::ProtocolRelationStructural => "protocol relation structural",
            Self::ProtocolRelationNominalRecursiveMembers => {
                "protocol relation nominal recursive members"
            }
            Self::ProtocolRelationFinish => "protocol relation finish",
            Self::ProtocolRelationInterfaceResume => "protocol relation interface resume",
            Self::ProtocolRelationStructuralNext => "protocol relation structural next",
            Self::ProtocolRelationNominalFiniteNext => "protocol relation nominal finite next",
            Self::ProtocolRelationNominalRecursiveNext => {
                "protocol relation nominal recursive next"
            }
            Self::ProtocolRelationMemberResume => "protocol relation member resume",
            Self::ProtocolRelationMetaBindingsResume => "protocol relation meta bindings resume",
            Self::ProtocolRelationMetaMembersResume => "protocol relation meta members resume",
            Self::ProtocolRelationObjectCompare => "protocol object compare",
            Self::ProtocolObject => "protocol-object equivalence",
            Self::ProtocolMembersDefined => "protocol member presence",
        }
    }

    fn synchronous_name(self) -> &'static str {
        match self {
            Self::Lookup(entry) => entry.synchronous_name(),
            Self::Sequent(entry) => entry.synchronous_name(),
            Self::ConstraintConcrete => "constraint_as_concrete_sync",
            Self::ConstraintDepth => "constraint_bound_depth_sync",
            Self::ConstraintCachedDepth => "cached_constraint_bound_depth_sync",
            Self::SatisfactionRoot => "node_satisfaction_sync",
            Self::SatisfactionConjunction => "simple_conjunction_satisfiable_sync",
            Self::SatisfactionAssignments => "path_assignments_sync",
            Self::PathVisitOwned => "path_visit_owned_sync",
            Self::PathVisitBody => "path_visit_body_sync",
            Self::PathEnterEdge => "path_enter_edge_sync",
            Self::PathDrainAssignments => "path_drain_assignments_sync",
            Self::PathAddAssignment => "path_add_assignment_sync",
            Self::PathDiscoverConstraint => "path_discover_constraint_sync",
            Self::PathImportSequents => "path_import_sequents_sync",
            Self::PathImportSlice => "path_import_slice_sync",
            Self::PathImportSequent => "path_import_sequent_sync",
            Self::PathCheckSequent => "path_check_sequent_sync",
            Self::PathCheckPairImplication => "path_check_pair_implication_sync",
            Self::PathCheckSingleImplication => "path_check_single_implication_sync",
            Self::StaticMroCycle => "static_mro_cycle_sync",
            Self::BaseCursorNext => "base_cursor_next_sync",
            Self::CollectStart => "collect_start_sync",
            Self::CollectStartWithRoot => "collect_start_with_root_sync",
            Self::CollectClassLiterals => "collect_class_literals_sync",
            Self::OwnMember => "own_class_member_sync",
            Self::MroClass => "mro_class_member_sync",
            Self::FinalizeClass => "finalize_class_member_sync",
            Self::OptionalClassSpecialization => "apply_optional_class_specialization_sync",
            Self::MroFirst => "mro_first_sync",
            Self::MroTailRequest => "mro_tail_request_sync",
            Self::MroNext => "mro_next_sync",
            Self::StaticMro => "static_mro_sync",
            Self::MaybeAddGeneric => "maybe_add_generic_sync",
            Self::BaseHasCyclicMro => "base_has_cyclic_mro_sync",
            Self::ClassMroStart => "class_mro_start_sync",
            Self::BaseMroStart => "base_mro_start_sync",
            Self::CollectBaseMro => "collect_base_mro_sync",
            Self::CollectSingleBaseMro => "collect_single_base_mro_sync",
            Self::C3Merge => "c3_merge_sync",
            Self::ClassTypeOwnMember => "class_type_own_member_sync",
            Self::SynthesizedMember => "own_synthesized_member_sync",
            Self::PublicPromotion => "promote_public_sync",
            Self::SingletonPromotion => "promote_singletons_impl_sync",
            Self::ProtocolCandidates => "for_each_protocol_member_candidate_sync",
            Self::ProtocolInterfaceCandidate => "protocol_interface_candidate_sync",
            Self::ProtocolInterfaceBuild => "protocol_interface_build_sync",
            Self::ProtocolInterfaceNormalize => "protocol_interface_normalize_sync",
            Self::ProtocolRelationDirect => "check_type_satisfies_protocol_sync",
            Self::ProtocolRelationMeta => "check_meta_type_satisfies_protocol_sync",
            Self::ProtocolRelationRun => "protocol_relation_run_sync",
            Self::ProtocolRelationStart => "protocol_relation_start_sync",
            Self::ProtocolRelationStartMeta => "protocol_relation_start_meta_sync",
            Self::ProtocolRelationPairResume => "protocol_relation_pair_resume_sync",
            Self::ProtocolRelationAfterNominal => "protocol_relation_after_nominal_sync",
            Self::ProtocolRelationNonRecursiveInterface => {
                "protocol_relation_non_recursive_interface_sync"
            }
            Self::ProtocolRelationStructural => "protocol_relation_structural_sync",
            Self::ProtocolRelationNominalRecursiveMembers => {
                "protocol_relation_nominal_recursive_members_sync"
            }
            Self::ProtocolRelationFinish => "protocol_relation_finish_sync",
            Self::ProtocolRelationInterfaceResume => "protocol_relation_interface_resume_sync",
            Self::ProtocolRelationStructuralNext => "protocol_relation_structural_next_sync",
            Self::ProtocolRelationNominalFiniteNext => "protocol_relation_nominal_finite_next_sync",
            Self::ProtocolRelationNominalRecursiveNext => {
                "protocol_relation_nominal_recursive_next_sync"
            }
            Self::ProtocolRelationMemberResume => "protocol_relation_member_resume_sync",
            Self::ProtocolRelationMetaBindingsResume => {
                "protocol_relation_meta_bindings_resume_sync"
            }
            Self::ProtocolRelationMetaMembersResume => "protocol_relation_meta_members_resume_sync",
            Self::ProtocolRelationObjectCompare => "protocol_object_compare_sync",
            Self::ProtocolObject => "protocol_object_equivalence_sync",
            Self::ProtocolMembersDefined => "protocol_members_defined_sync",
        }
    }

    fn synchronous_bound(self, span: Span) -> Path {
        match self {
            Self::Lookup(entry) => entry.synchronous_bound(span),
            Self::Sequent(_) => {
                syn::parse_quote_spanned! { span => crate::types::constraints::sequents::effects::SyncSequentEffects<'db> }
            }
            Self::ConstraintConcrete => {
                syn::parse_quote_spanned! { span => crate::types::constraints::type_analysis::SyncConstraintTypeEffects<'db> }
            }
            Self::ConstraintDepth => {
                syn::parse_quote_spanned! { span => crate::types::constraints::type_analysis::SyncConstraintTypeEffects<'db> }
            }
            Self::ConstraintCachedDepth => {
                syn::parse_quote_spanned! { span => crate::types::constraints::type_analysis::SyncConstraintDepthCacheEffects<'db> }
            }
            Self::SatisfactionRoot => {
                syn::parse_quote_spanned! { span => crate::types::constraints::satisfaction::SyncSatisfactionEffects<'db> }
            }
            Self::SatisfactionConjunction => {
                syn::parse_quote_spanned! { span => crate::types::constraints::satisfaction::SyncSatisfactionEffects<'db> }
            }
            Self::SatisfactionAssignments => {
                syn::parse_quote_spanned! { span => crate::types::constraints::satisfaction::SyncSatisfactionEffects<'db> }
            }
            Self::PathVisitOwned => {
                syn::parse_quote_spanned! { span => crate::types::constraints::paths::SyncPathVisitEffects<'db, V> }
            }
            Self::PathVisitBody => {
                syn::parse_quote_spanned! { span => crate::types::constraints::paths::SyncPathVisitEffects<'db, V> }
            }
            Self::PathEnterEdge => {
                syn::parse_quote_spanned! { span => crate::types::constraints::paths::SyncPathEffects<'db> }
            }
            Self::PathDrainAssignments => {
                syn::parse_quote_spanned! { span => crate::types::constraints::paths::SyncPathEffects<'db> }
            }
            Self::PathAddAssignment => {
                syn::parse_quote_spanned! { span => crate::types::constraints::paths::SyncPathEffects<'db> }
            }
            Self::PathDiscoverConstraint => {
                syn::parse_quote_spanned! { span => crate::types::constraints::paths::SyncPathEffects<'db> }
            }
            Self::PathImportSequents => {
                syn::parse_quote_spanned! { span => crate::types::constraints::paths::SyncPathEffects<'db> }
            }
            Self::PathImportSlice => {
                syn::parse_quote_spanned! { span => crate::types::constraints::paths::SyncPathEffects<'db> }
            }
            Self::PathImportSequent => {
                syn::parse_quote_spanned! { span => crate::types::constraints::paths::SyncPathEffects<'db> }
            }
            Self::PathCheckSequent => {
                syn::parse_quote_spanned! { span => crate::types::constraints::paths::SyncPathEffects<'db> }
            }
            Self::PathCheckPairImplication => {
                syn::parse_quote_spanned! { span => crate::types::constraints::paths::SyncPathEffects<'db> }
            }
            Self::PathCheckSingleImplication => {
                syn::parse_quote_spanned! { span => crate::types::constraints::paths::SyncPathEffects<'db> }
            }
            Self::CollectStart | Self::CollectStartWithRoot => {
                syn::parse_quote_spanned! { span => crate::types::mro::collection::SynchronousMroCollectionEffects<'db> }
            }
            Self::CollectClassLiterals => {
                syn::parse_quote_spanned! { span => crate::types::mro::collection::SynchronousClassLiteralCollectionEffects<'db> }
            }
            Self::MroNext | Self::BaseCursorNext => syn::parse_quote_spanned! { span =>
                crate::types::mro::iteration::SynchronousMroIterationEffects<'db>
            },
            Self::OwnMember => syn::parse_quote_spanned! { span =>
                crate::types::class::own_member::SynchronousOwnMemberEffects<'db>
            },
            Self::MroClass => syn::parse_quote_spanned! { span =>
                crate::types::class::member_lookup::SynchronousMroMemberEffects<'db, C>
            },
            Self::FinalizeClass => syn::parse_quote_spanned! { span =>
                crate::types::class::member_lookup::SynchronousMemberFinalizationEffects<'db>
            },
            Self::OptionalClassSpecialization | Self::MroFirst | Self::MroTailRequest => {
                syn::parse_quote_spanned! { span =>
                    crate::types::mro::root::SynchronousMroRootEffects<'db>
                }
            }
            Self::StaticMro
            | Self::StaticMroCycle
            | Self::MaybeAddGeneric
            | Self::BaseHasCyclicMro => {
                syn::parse_quote_spanned! { span =>
                    crate::types::mro::construction::SynchronousStaticMroEffects<'db>
                }
            }
            Self::ClassMroStart
            | Self::BaseMroStart
            | Self::CollectBaseMro
            | Self::CollectSingleBaseMro => syn::parse_quote_spanned! { span =>
                crate::types::mro::base::SynchronousBaseMroEffects<'db>
            },
            Self::C3Merge => syn::parse_quote_spanned! { span =>
                crate::types::mro::c3::SynchronousC3Effects
            },
            Self::ClassTypeOwnMember => syn::parse_quote_spanned! { span =>
                crate::types::class::own_member::SynchronousClassTypeOwnMemberEffects<'db>
            },
            Self::SynthesizedMember => syn::parse_quote_spanned! { span =>
                crate::types::class::synthesized_member::SynchronousSynthesizedMemberEffects<'db>
            },
            Self::PublicPromotion | Self::SingletonPromotion => syn::parse_quote_spanned! { span =>
                crate::types::promotion::SynchronousPublicPromotionEffects<'db>
            },
            Self::ProtocolInterfaceNormalize => syn::parse_quote_spanned! { span =>
                crate::types::protocol_class::interface_build::SyncProtocolInterfaceNormalizationEffects<'db>
            },
            Self::ProtocolCandidates => syn::parse_quote_spanned! { span =>
                crate::types::protocol_class::interface_build::SyncProtocolCandidateEffects<'db, C>
            },
            Self::ProtocolInterfaceCandidate | Self::ProtocolInterfaceBuild => {
                syn::parse_quote_spanned! { span =>
                    crate::types::protocol_class::interface_build::SyncProtocolInterfaceEffects<'db>
                }
            }
            Self::ProtocolRelationDirect
            | Self::ProtocolRelationMeta
            | Self::ProtocolRelationRun
            | Self::ProtocolRelationStart
            | Self::ProtocolRelationStartMeta
            | Self::ProtocolRelationPairResume
            | Self::ProtocolRelationAfterNominal
            | Self::ProtocolRelationNonRecursiveInterface
            | Self::ProtocolRelationStructural
            | Self::ProtocolRelationNominalRecursiveMembers
            | Self::ProtocolRelationFinish
            | Self::ProtocolRelationInterfaceResume
            | Self::ProtocolRelationStructuralNext
            | Self::ProtocolRelationNominalFiniteNext
            | Self::ProtocolRelationNominalRecursiveNext
            | Self::ProtocolRelationMemberResume
            | Self::ProtocolRelationMetaBindingsResume
            | Self::ProtocolRelationMetaMembersResume
            | Self::ProtocolRelationObjectCompare => syn::parse_quote_spanned! { span =>
                crate::types::instance::protocol_relation::SyncProtocolRelationEffects<'a, 'c, 'db>
            },
            Self::ProtocolObject => syn::parse_quote_spanned! { span =>
                crate::types::instance::protocol_object::SyncProtocolObjectEffects<'db>
            },
            Self::ProtocolMembersDefined => syn::parse_quote_spanned! { span =>
                crate::types::protocol_class::member_presence::SyncProtocolMembersDefinedEffects<'db>
            },
        }
    }

    fn expected_signature(self) -> Signature {
        match self {
            Self::Lookup(entry) => entry.expected_signature(),
            Self::Sequent(entry) => entry.expected_signature(),
            Self::ConstraintConcrete => {
                syn::parse_quote! { async fn constraint_as_concrete_with<'db, E: ConstraintTypeEffects<'db>>(constraint: Constraint<'db>, effects: &mut E) -> Result<Option<BoundTypeVarInstance<'db>>, E::Error> }
            }
            Self::ConstraintDepth => {
                syn::parse_quote! { async fn constraint_bound_depth_with<'db, E: ConstraintTypeEffects<'db>>(constraint: Constraint<'db>, effects: &mut E) -> Result<(u16, u16), E::Error> }
            }
            Self::ConstraintCachedDepth => {
                syn::parse_quote! { async fn cached_constraint_bound_depth_with<'db, E: ConstraintDepthCacheEffects<'db>>(id: ConstraintId, effects: &mut E) -> Result<(u16, u16), E::Error> }
            }
            Self::SatisfactionRoot => {
                syn::parse_quote! { async fn node_satisfaction_with<'db, E: SatisfactionEffects<'db>>(
                    node: NodeId,
                    source_order: Option<SourceOrderId>,
                    kind: SatisfactionKind,
                    effects: &mut E,
                ) -> Result<bool, E::Error> }
            }
            Self::SatisfactionConjunction => {
                syn::parse_quote! { async fn simple_conjunction_satisfiable_with<'db, E: SatisfactionEffects<'db>>(
                    node: NodeId,
                    effects: &mut E,
                ) -> Result<bool, E::Error> }
            }
            Self::SatisfactionAssignments => {
                syn::parse_quote! { async fn path_assignments_with<'db, E: SatisfactionEffects<'db>>(
                    interior: InteriorNode,
                    source_order: Option<SourceOrderId>,
                    effects: &mut E,
                ) -> Result<PathAssignments, E::Error> }
            }
            Self::PathVisitOwned => {
                syn::parse_quote! { async fn path_visit_owned_with<'db, V: PathVisitor, E: PathVisitEffects<'db, V>>(
                    path: PathAssignments,
                    node: NodeId,
                    visitor: &mut V,
                    negated: bool,
                    effects: &mut E,
                ) -> Result<PathVisitCompletion<V>, E::Error> }
            }
            Self::PathVisitBody => {
                syn::parse_quote! { async fn path_visit_body_with<'db, V: PathVisitor, E: PathVisitEffects<'db, V>>(
                    state: &mut PathVisitState,
                    visitor: &mut V,
                    effects: &mut E,
                ) -> Result<ControlFlow<V::Break, V::Result>, E::Error> }
            }
            Self::PathEnterEdge => {
                syn::parse_quote! { async fn path_enter_edge_with<'db, E: PathEffects<'db>>(
                    path: &mut PathAssignments,
                    assignment: ConstraintAssignment,
                    effects: &mut E,
                ) -> Result<EdgeOutcome, E::Error> }
            }
            Self::PathDrainAssignments => {
                syn::parse_quote! { async fn path_drain_assignments_with<'db, E: PathEffects<'db>>(
                    path: &mut PathAssignments,
                    source_constraint: ConstraintId,
                    effects: &mut E,
                ) -> Result<Result<(), PathAssignmentConflict>, E::Error> }
            }
            Self::PathAddAssignment => {
                syn::parse_quote! { async fn path_add_assignment_with<'db, E: PathEffects<'db>>(
                    path: &mut PathAssignments,
                    assignment: ConstraintAssignment,
                    source_constraint: ConstraintId,
                    fuel: AssignmentFuel,
                    effects: &mut E,
                ) -> Result<Result<(), PathAssignmentConflict>, E::Error> }
            }
            Self::PathDiscoverConstraint => {
                syn::parse_quote! { async fn path_discover_constraint_with<'db, E: PathEffects<'db>>(
                    path: &mut PathAssignments,
                    constraint: ConstraintId,
                    effects: &mut E,
                ) -> Result<(), E::Error> }
            }
            Self::PathImportSequents => {
                syn::parse_quote! { async fn path_import_sequents_with<'db, E: PathEffects<'db>>(
                    path: &mut PathAssignments,
                    map: &SequentMap<'db>,
                    effects: &mut E,
                ) -> Result<Range<usize>, E::Error> }
            }
            Self::PathImportSlice => {
                syn::parse_quote! { async fn path_import_slice_with<'db, E: PathEffects<'db>>(
                    path: &mut PathAssignments,
                    sequents: &[Sequent<Constraint<'db>>],
                    effects: &mut E,
                ) -> Result<(), E::Error> }
            }
            Self::PathImportSequent => {
                syn::parse_quote! { async fn path_import_sequent_with<'db, E: PathEffects<'db>>(
                    sequent: Sequent<Constraint<'db>>,
                    effects: &mut E,
                ) -> Result<Sequent<ConstraintId, u16>, E::Error> }
            }
            Self::PathCheckSequent => {
                syn::parse_quote! { async fn path_check_sequent_with<'db, E: PathEffects<'db>>(
                    path: &mut PathAssignments,
                    sequent: Sequent<ConstraintId, u16>,
                    effects: &mut E,
                ) -> Result<Result<(), PathAssignmentConflict>, E::Error> }
            }
            Self::PathCheckPairImplication => {
                syn::parse_quote! { async fn path_check_pair_implication_with<'db, E: PathEffects<'db>>(
                    path: &mut PathAssignments,
                    ante1: ConstraintId,
                    ante2: ConstraintId,
                    post: ConstraintId,
                    fuel_cost: u16,
                    effects: &mut E,
                ) -> Result<(), E::Error> }
            }
            Self::PathCheckSingleImplication => {
                syn::parse_quote! { async fn path_check_single_implication_with<'db, E: PathEffects<'db>>(
                    path: &mut PathAssignments,
                    ante: ConstraintId,
                    post: ConstraintId,
                    fuel_cost: u16,
                    effects: &mut E,
                ) -> Result<(), E::Error> }
            }
            Self::StaticMroCycle => syn::parse_quote! {
                async fn static_mro_cycle_with<'db, E: StaticMroEffects<'db>>(
                    fields: MroFieldReads<'db>, class_literal: StaticClassLiteral<'db>,
                    specialization: Option<Specialization<'db>>, effects: &E,
                ) -> Result<StaticMroError<'db>, E::Error>
            },
            Self::BaseCursorNext => syn::parse_quote! {
                async fn base_cursor_next_with<'db, E: MroIterationEffects<'db>>(
                    fields: MroFieldReads<'db>, cursor: &mut BaseCursor<'db>, effects: &E,
                ) -> Result<Option<ClassBase<'db>>, E::Error>
            },
            Self::CollectStart => syn::parse_quote! {
                async fn collect_start_with<'db, E: MroCollectionEffects<'db>>(
                    fields: MroFieldReads<'db>, start: BaseMroStart<'db>, effects: &E,
                ) -> Result<VecDeque<ClassBase<'db>>, E::Error>
            },
            Self::CollectStartWithRoot => syn::parse_quote! {
                async fn collect_start_with_root_with<'db, E: MroCollectionEffects<'db>>(
                    fields: MroFieldReads<'db>, root: ClassType<'db>, start: BaseMroStart<'db>, effects: &E,
                ) -> Result<Mro<'db>, E::Error>
            },
            Self::CollectClassLiterals => syn::parse_quote! {
                async fn collect_class_literals_with<'db, E: ClassLiteralCollectionEffects<'db>>(
                    fields: MroFieldReads<'db>, class: ClassLiteral<'db>, effects: &E,
                ) -> Result<Box<[ClassLiteral<'db>]>, E::Error>
            },
            Self::MroNext => syn::parse_quote! {
                async fn mro_next_with<'db, E: MroIterationEffects<'db>>(
                    fields: MroFieldReads<'db>,
                    cursor: &mut MroCursor<'db>,
                    direction: MroDirection,
                    effects: &E,
                ) -> Result<Option<ClassBase<'db>>, E::Error>
            },
            Self::OwnMember => syn::parse_quote! {
                async fn own_class_member_with<'a, 'db, E: OwnMemberEffects<'db>>(
                    request: OwnMemberLookupRequest<'a, 'db>,
                    effects: &E,
                ) -> Result<Member<'db>, E::Error>
            },
            Self::MroClass => syn::parse_quote! {
                async fn mro_class_member_with<'a, 'db, C, E: MroMemberEffects<'db, C>>(
                    request: MroClassMemberRequest<'a, 'db>,
                    mut cursor: C,
                    effects: &E,
                ) -> Result<ClassMemberResult<'db>, E::Error>
            },
            Self::FinalizeClass => syn::parse_quote! {
                async fn finalize_class_member_with<'db, E: MemberFinalizationEffects<'db>>(
                    result: CompletedMemberLookup<'db>,
                    effects: &E,
                ) -> Result<PlaceAndQualifiers<'db>, E::Error>
            },
            Self::OptionalClassSpecialization => syn::parse_quote! {
                async fn apply_optional_class_specialization_with<'db, E: MroRootEffects<'db>>(
                    fields: MroFieldReads<'db>,
                    class: StaticClassLiteral<'db>,
                    specialization: Option<Specialization<'db>>,
                    effects: &E,
                ) -> Result<ClassType<'db>, E::Error>
            },
            Self::MroFirst => syn::parse_quote! {
                async fn mro_first_with<'db, E: MroRootEffects<'db>>(
                    fields: MroFieldReads<'db>,
                    class: ClassLiteral<'db>,
                    specialization: Option<Specialization<'db>>,
                    effects: &E,
                ) -> Result<ClassBase<'db>, E::Error>
            },
            Self::MroTailRequest => syn::parse_quote! {
                async fn mro_tail_request_with<'db, E: MroRootEffects<'db>>(
                    fields: MroFieldReads<'db>,
                    class: ClassLiteral<'db>,
                    specialization: Option<Specialization<'db>>,
                    effects: &E,
                ) -> Result<MroTailRequest<'db>, E::Error>
            },
            Self::StaticMro => syn::parse_quote! {
                async fn static_mro_with<'db, E: StaticMroEffects<'db>>(
                    fields: MroFieldReads<'db>,
                    class_literal: StaticClassLiteral<'db>,
                    specialization: Option<Specialization<'db>>,
                    effects: &E,
                ) -> Result<Result<Mro<'db>, StaticMroError<'db>>, E::Error>
            },
            Self::MaybeAddGeneric => syn::parse_quote! {
                async fn maybe_add_generic_with<'db, E: StaticMroEffects<'db>>(
                    resolved_bases: &mut Vec<ClassBase<'db>>,
                    original_bases: &[Type<'db>],
                    remaining_bases: &[Type<'db>],
                    effects: &E,
                ) -> Result<(), E::Error>
            },
            Self::BaseHasCyclicMro => syn::parse_quote! {
                async fn base_has_cyclic_mro_with<'db, E: StaticMroEffects<'db>>(
                    fields: MroFieldReads<'db>,
                    base: ClassBase<'db>,
                    effects: &E,
                ) -> Result<bool, E::Error>
            },
            Self::ClassMroStart => syn::parse_quote! {
                async fn class_mro_start_with<'db, E: BaseMroEffects<'db>>(
                    fields: MroFieldReads<'db>,
                    class: ClassType<'db>,
                    additional: Option<Specialization<'db>>,
                    effects: &E,
                ) -> Result<ClassMroStart<'db>, E::Error>
            },
            Self::BaseMroStart => syn::parse_quote! {
                async fn base_mro_start_with<'db, E: BaseMroEffects<'db>>(
                    fields: MroFieldReads<'db>,
                    env: &ProgramEnvironment<'db>,
                    base: ClassBase<'db>,
                    additional: Option<Specialization<'db>>,
                    effects: &E,
                ) -> Result<BaseMroStart<'db>, E::Error>
            },
            Self::CollectBaseMro => syn::parse_quote! {
                async fn collect_base_mro_with<'db, E: BaseMroEffects<'db>>(
                    fields: MroFieldReads<'db>,
                    env: &ProgramEnvironment<'db>,
                    base: ClassBase<'db>,
                    additional: Option<Specialization<'db>>,
                    effects: &E,
                ) -> Result<VecDeque<ClassBase<'db>>, E::Error>
            },
            Self::CollectSingleBaseMro => syn::parse_quote! {
                async fn collect_single_base_mro_with<'db, E: BaseMroEffects<'db>>(
                    fields: MroFieldReads<'db>,
                    env: &ProgramEnvironment<'db>,
                    root: ClassType<'db>,
                    base: ClassBase<'db>,
                    additional: Option<Specialization<'db>>,
                    effects: &E,
                ) -> Result<Mro<'db>, E::Error>
            },
            Self::C3Merge => syn::parse_quote! {
                async fn c3_merge_with<'db, E: C3Effects<'db>>(
                    fields: MroFieldReads<'db>,
                    mut sequences: Vec<VecDeque<ClassBase<'db>>>,
                    effects: &E,
                ) -> Result<Option<Mro<'db>>, E::Error>
            },
            Self::ClassTypeOwnMember => syn::parse_quote! {
                async fn class_type_own_member_with<'a, 'db, E: ClassTypeOwnMemberEffects<'db>>(
                    request: ClassTypeOwnMemberRequest<'a, 'db>,
                    effects: &E,
                ) -> Result<Member<'db>, E::Error>
            },
            Self::SynthesizedMember => syn::parse_quote! {
                async fn own_synthesized_member_with<'a, 'db, E: SynthesizedMemberEffects<'db>>(
                    request: OwnMemberLookupRequest<'a, 'db>,
                    effects: &E,
                ) -> Result<Option<Type<'db>>, E::Error>
            },
            Self::ProtocolCandidates => syn::parse_quote! {
                async fn for_each_protocol_member_candidate_with<'db, C, E: ProtocolCandidateEffects<'db, C>>(
                    class: ClassType<'db>,
                    env: &ProgramEnvironment<'db>,
                    consumer: &mut C,
                    effects: &E,
                ) -> Result<(), E::Error>
            },
            Self::ProtocolInterfaceCandidate => syn::parse_quote! {
                async fn protocol_interface_candidate_with<'db, E: ProtocolInterfaceEffects<'db>>(
                    env: &ProgramEnvironment<'db>,
                    build: &mut ProtocolInterfaceBuild<'db>,
                    name: &Name,
                    candidate: ProtocolMemberCandidate<'db>,
                    specialization: Option<Specialization<'db>>,
                    effects: &E,
                ) -> Result<(), E::Error>
            },
            Self::ProtocolInterfaceNormalize => syn::parse_quote! {
                async fn protocol_interface_normalize_with<'db, E: ProtocolInterfaceNormalizationEffects<'db>>(
                    env: &ProgramEnvironment<'db>,
                    previous: &BTreeMap<Name, ProtocolMemberData<'db>>,
                    current: &BTreeMap<Name, ProtocolMemberData<'db>>,
                    cycle: &salsa::Cycle<'_>,
                    effects: &E,
                ) -> Result<BTreeMap<Name, ProtocolMemberData<'db>>, E::Error>
            },
            Self::ProtocolInterfaceBuild => syn::parse_quote! {
                async fn protocol_interface_build_with<'db, E: ProtocolInterfaceEffects<'db>>(
                    class: ClassType<'db>,
                    effects: &E,
                ) -> Result<PreparedProtocolInterface<'db>, E::Error>
            },
            Self::ProtocolRelationDirect => syn::parse_quote! {
                            async fn check_type_satisfies_protocol_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
                ty: Type<'db>,
                protocol: ProtocolInstanceType<'db>,
                effects: &E,
            ) -> Result<ConstraintSet<'db, 'c>, E::Error>
                        },
            Self::ProtocolRelationMeta => syn::parse_quote! {
                            async fn check_meta_type_satisfies_protocol_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
                meta_ty: Type<'db>,
                protocol: ProtocolInstanceType<'db>,
                effects: &E,
            ) -> Result<ConstraintSet<'db, 'c>, E::Error>
                        },
            Self::ProtocolRelationRun => syn::parse_quote! {
                            async fn protocol_relation_run_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                step: ProtocolRelationStep<'checker, 'a, 'c, 'db>,
                effects: &E,
            ) -> Result<ConstraintSet<'db, 'c>, E::Error>
                        },
            Self::ProtocolRelationStart => syn::parse_quote! {
                            async fn protocol_relation_start_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
                ty: Type<'db>,
                protocol: ProtocolInstanceType<'db>,
                effects: &E,
            ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error>
                        },
            Self::ProtocolRelationStartMeta => syn::parse_quote! {
                            async fn protocol_relation_start_meta_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
                meta_ty: Type<'db>,
                protocol: ProtocolInstanceType<'db>,
                effects: &E,
            ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error>
                        },
            Self::ProtocolRelationPairResume => syn::parse_quote! {
                            async fn protocol_relation_pair_resume_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                pending: PendingProtocolPair<'checker, 'a, 'c, 'db>,
                result: ConstraintSet<'db, 'c>,
                effects: &E,
            ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error>
                        },
            Self::ProtocolRelationAfterNominal => syn::parse_quote! {
                            async fn protocol_relation_after_nominal_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                input: ProtocolInput<'checker, 'a, 'c, 'db>,
                source_nominal: Option<NominalInstanceType<'db>>,
                target_nominal: NominalInstanceType<'db>,
                nominally_satisfied: ConstraintSet<'db, 'c>,
                effects: &E,
            ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error>
                        },
            Self::ProtocolRelationNonRecursiveInterface => syn::parse_quote! {
                            async fn protocol_relation_non_recursive_interface_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                input: ProtocolInput<'checker, 'a, 'c, 'db>,
                source_nominal: Option<NominalInstanceType<'db>>,
                target_nominal: NominalInstanceType<'db>,
                effects: &E,
            ) -> Result<Option<FiniteInterface<'db>>, E::Error>
                        },
            Self::ProtocolRelationStructural => syn::parse_quote! {
                            async fn protocol_relation_structural_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                input: ProtocolInput<'checker, 'a, 'c, 'db>,
                nominal_result: ConstraintSet<'db, 'c>,
                effects: &E,
            ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error>
                        },
            Self::ProtocolRelationNominalRecursiveMembers => syn::parse_quote! {
                            async fn protocol_relation_nominal_recursive_members_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                input: ProtocolInput<'checker, 'a, 'c, 'db>,
                nominally_satisfied: ConstraintSet<'db, 'c>,
                effects: &E,
            ) -> Result<Option<NominalRecursiveMembers<'checker, 'a, 'c, 'db>>, E::Error>
                        },
            Self::ProtocolRelationFinish => syn::parse_quote! {
                            async fn protocol_relation_finish_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                input: ProtocolInput<'checker, 'a, 'c, 'db>,
                nominal_result: ConstraintSet<'db, 'c>,
                structural: ConstraintSet<'db, 'c>,
                effects: &E,
            ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error>
                        },
            Self::ProtocolRelationInterfaceResume => syn::parse_quote! {
                            async fn protocol_relation_interface_resume_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                pending: PendingProtocolInterface<'checker, 'a, 'c, 'db>,
                structural: ConstraintSet<'db, 'c>,
                effects: &E,
            ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error>
                        },
            Self::ProtocolRelationStructuralNext => syn::parse_quote! {
                            async fn protocol_relation_structural_next_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                members: StructuralMembers<'checker, 'a, 'c, 'db>,
                effects: &E,
            ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error>
                        },
            Self::ProtocolRelationNominalFiniteNext => syn::parse_quote! {
                            async fn protocol_relation_nominal_finite_next_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                members: NominalRecursiveMembers<'checker, 'a, 'c, 'db>,
                effects: &E,
            ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error>
                        },
            Self::ProtocolRelationNominalRecursiveNext => syn::parse_quote! {
                            async fn protocol_relation_nominal_recursive_next_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                members: NominalRecursiveMembers<'checker, 'a, 'c, 'db>,
                structural: ConstraintSet<'db, 'c>,
                effects: &E,
            ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error>
                        },
            Self::ProtocolRelationMemberResume => syn::parse_quote! {
                            async fn protocol_relation_member_resume_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                pending: PendingProtocolMember<'checker, 'a, 'c, 'db>,
                result: ConstraintSet<'db, 'c>,
                effects: &E,
            ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error>
                        },
            Self::ProtocolRelationMetaBindingsResume => syn::parse_quote! {
                            async fn protocol_relation_meta_bindings_resume_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                pending: PendingMetaBindings<'checker, 'a, 'c, 'db>,
                bindings: &Bindings<'db>,
                effects: &E,
            ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error>
                        },
            Self::ProtocolRelationMetaMembersResume => syn::parse_quote! {
                            async fn protocol_relation_meta_members_resume_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                fields: RelationFieldReads<'db>,
                pending: PendingMetaMembers<'checker, 'a, 'c, 'db>,
                result: ConstraintSet<'db, 'c>,
                effects: &E,
            ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error>
                        },
            Self::ProtocolRelationObjectCompare => syn::parse_quote! {
                            async fn protocol_object_compare_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
                checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
                protocol: ProtocolInstanceType<'db>,
                effects: &E,
            ) -> Result<bool, E::Error>
                        },
            Self::ProtocolObject => syn::parse_quote! {
                async fn protocol_object_equivalence_with<'db, E: ProtocolObjectEffects<'db>>(
                    fields: RelationFieldReads<'db>,
                    protocol: ProtocolInstanceType<'db>,
                    effects: &E,
                ) -> Result<bool, E::Error>
            },
            Self::ProtocolMembersDefined => syn::parse_quote! {
                async fn protocol_members_defined_with<'db, E: ProtocolMembersDefinedEffects<'db>>(
                    fields: RelationFieldReads<'db>,
                    env: &ProgramEnvironment<'db>,
                    ty: Type<'db>,
                    protocol: ProtocolInstanceType<'db>,
                    effects: &E,
                ) -> Result<bool, E::Error>
            },
            Self::PublicPromotion | Self::SingletonPromotion => {
                let name = Ident::new(
                    if matches!(self, Self::PublicPromotion) {
                        "promote_public_with"
                    } else {
                        "promote_singletons_impl_with"
                    },
                    Span::call_site(),
                );
                syn::parse_quote! {
                    async fn #name<E: PublicPromotionEffects<'db>>(
                        self,
                        db: &'db dyn Db,
                        env: &ProgramEnvironment<'db>,
                        effects: &E,
                    ) -> Result<Type<'db>, E::Error>
                }
            }
        }
    }

    fn signature_error(self) -> &'static str {
        match self {
            Self::Lookup(_) => "shared lookup requires its exact declared signature",
            Self::Sequent(_) => "dual_sequent requires its exact declared SequentEffects signature",
            Self::ConstraintConcrete => {
                "shared type analysis requires its exact declared signature and effects: &mut E"
            }
            Self::ConstraintDepth => {
                "shared type analysis requires its exact declared signature and effects: &mut E"
            }
            Self::ConstraintCachedDepth => {
                "shared type analysis requires its exact declared signature and effects: &mut E"
            }
            Self::SatisfactionRoot
            | Self::SatisfactionConjunction
            | Self::SatisfactionAssignments
            | Self::PathVisitOwned
            | Self::PathVisitBody
            | Self::PathEnterEdge
            | Self::PathDrainAssignments
            | Self::PathAddAssignment
            | Self::PathDiscoverConstraint
            | Self::PathImportSequents
            | Self::PathImportSlice
            | Self::PathImportSequent
            | Self::PathCheckSequent
            | Self::PathCheckPairImplication
            | Self::PathCheckSingleImplication => {
                "dual_satisfaction requires the exact declared signature and effects: &mut E"
            }
            Self::StaticMroCycle => {
                "dual_static_mro requires the declared cycle seed signature with fields, class_literal, specialization, and effects"
            }
            Self::BaseCursorNext | Self::CollectStart | Self::CollectStartWithRoot => {
                "dual_base_mro requires the declared cursor or collection signature with fields and effects"
            }
            Self::CollectClassLiterals => {
                "dual_base_mro requires the declared ClassLiteralCollectionEffects signature with fields, class, and effects: &E"
            }
            Self::MroNext => {
                "dual_mro_iteration requires the declared MroIterationEffects signature with fields, cursor: &mut MroCursor<'db>, direction, and effects: &E"
            }
            Self::OwnMember => {
                "dual_own_member requires the declared OwnMemberEffects signature: request, effects: &E, and Result<Member<'db>, E::Error>"
            }
            Self::MroClass => {
                "dual_mro_member requires the declared MroMemberEffects signature with unconstrained C, mut cursor: C, and effects: &E"
            }
            Self::FinalizeClass => {
                "dual_mro_member requires the declared MemberFinalizationEffects signature with result: CompletedMemberLookup<'db> and effects: &E"
            }
            Self::OptionalClassSpecialization | Self::MroFirst | Self::MroTailRequest => {
                "dual_mro_root requires the declared MroRootEffects signature with fields, class, specialization, and effects: &E"
            }
            Self::StaticMro => {
                "dual_static_mro requires the declared StaticMroEffects constructor signature with fields, class_literal, specialization, and effects: &E"
            }
            Self::MaybeAddGeneric => {
                "dual_static_mro requires the declared StaticMroEffects Generic helper signature with resolved_bases, original_bases, remaining_bases, and effects: &E"
            }
            Self::BaseHasCyclicMro => {
                "dual_static_mro requires the declared StaticMroEffects cycle helper signature with fields, base, and effects: &E"
            }
            Self::ClassMroStart => {
                "dual_base_mro requires the declared BaseMroEffects class-start signature with fields, class, additional, and effects: &E"
            }
            Self::BaseMroStart | Self::CollectBaseMro => {
                "dual_base_mro requires the declared BaseMroEffects base-start or collection signature with fields, env, base, additional, and effects: &E"
            }
            Self::CollectSingleBaseMro => {
                "dual_base_mro requires the declared BaseMroEffects single-base collection signature with fields, env, root, base, additional, and effects: &E"
            }
            Self::C3Merge => {
                "dual_c3_merge requires the declared C3Effects signature with fields, mut sequences: Vec<VecDeque<ClassBase<'db>>>, and effects: &E"
            }
            Self::ClassTypeOwnMember => {
                "dual_class_type_own_member requires the declared ClassTypeOwnMemberEffects signature with request and effects: &E"
            }
            Self::SynthesizedMember => {
                "dual_synthesized_member requires the declared SynthesizedMemberEffects signature with request, effects: &E, and Result<Option<Type<'db>>, E::Error>"
            }
            Self::PublicPromotion | Self::SingletonPromotion => {
                "dual_public_promotion requires the declared PublicPromotionEffects signature with self, db, env, and effects: &E"
            }
            Self::ProtocolCandidates => {
                "dual_protocol_interface requires the declared ProtocolCandidateEffects signature with class, env, consumer: &mut C, and effects: &E"
            }
            Self::ProtocolInterfaceCandidate => {
                "dual_protocol_interface requires the declared ProtocolInterfaceEffects candidate signature with env, build, name, candidate, specialization, and effects: &E"
            }
            Self::ProtocolInterfaceNormalize => {
                "dual_protocol_interface requires the declared normalization signature with env, previous, current, cycle, and effects: &E"
            }
            Self::ProtocolInterfaceBuild => {
                "dual_protocol_interface requires the declared ProtocolInterfaceEffects build signature with class, effects: &E, and Result<PreparedProtocolInterface<'db>, E::Error>"
            }
            Self::ProtocolRelationDirect
            | Self::ProtocolRelationMeta
            | Self::ProtocolRelationRun
            | Self::ProtocolRelationStart
            | Self::ProtocolRelationStartMeta
            | Self::ProtocolRelationPairResume
            | Self::ProtocolRelationAfterNominal
            | Self::ProtocolRelationNonRecursiveInterface
            | Self::ProtocolRelationStructural
            | Self::ProtocolRelationNominalRecursiveMembers
            | Self::ProtocolRelationFinish
            | Self::ProtocolRelationInterfaceResume
            | Self::ProtocolRelationStructuralNext
            | Self::ProtocolRelationNominalFiniteNext
            | Self::ProtocolRelationNominalRecursiveNext
            | Self::ProtocolRelationMemberResume
            | Self::ProtocolRelationMetaBindingsResume
            | Self::ProtocolRelationMetaMembersResume
            | Self::ProtocolRelationObjectCompare => {
                "dual_protocol_relation requires the declared ProtocolRelationEffects signature, exact state arguments, effects: &E, and Result output"
            }
            Self::ProtocolObject => {
                "dual_protocol_object requires the declared ProtocolObjectEffects signature with fields, protocol, effects: &E, and Result<bool, E::Error>"
            }
            Self::ProtocolMembersDefined => {
                "dual_protocol_members_defined requires the declared ProtocolMembersDefinedEffects signature with fields, env, ty, protocol, effects: &E, and Result<bool, E::Error>"
            }
        }
    }

    fn is_effect_method(self, method: &Ident) -> bool {
        let methods: &[&str] = match self {
            Self::Lookup(entry) => entry.effect_methods(),
            Self::Sequent(entry) => entry.effect_methods(),
            Self::ConstraintConcrete => &["checkpoint", "search_bound", "materialize_bound"],
            Self::ConstraintDepth => &["checkpoint", "type_depth"],
            Self::ConstraintCachedDepth => &[
                "checkpoint",
                "depth_cache_get",
                "depth_constraint",
                "depth_cache_publish",
            ],
            Self::SatisfactionRoot => &["checkpoint", "never_cache_get", "never_cache_insert"],
            Self::SatisfactionConjunction => &["checkpoint", "interior_data", "constraint_data"],
            Self::SatisfactionAssignments => &[
                "checkpoint",
                "collect_unique_constraints",
                "collect_source_order",
                "is_single_conjunction",
                "constraint_data",
                "as_concrete",
                "existing_typevar_id",
                "extend_dependent_support",
                "reserve_typevars",
            ],
            Self::PathVisitOwned => &[],
            Self::PathVisitBody => &[
                "checkpoint",
                "interior_data",
                "visit_node",
                "visit_satisfied",
                "visit_unsatisfied",
                "visit_impossible",
                "enter_interior",
                "visit_edge",
                "leave_interior",
                "or_nodes",
                "reserve_frames",
            ],
            Self::PathEnterEdge => &["checkpoint", "trace_path", "reserve_path"],
            Self::PathDrainAssignments => &["checkpoint"],
            Self::PathAddAssignment => &["checkpoint", "trace_path", "reserve_path"],
            Self::PathDiscoverConstraint => &[
                "checkpoint",
                "constraint_data",
                "single_sequents",
                "pair_sequents",
                "independent_pair_skip",
                "pair_cannot_produce",
                "reserve_path",
                "reserve_replay",
            ],
            Self::PathImportSequents => &["checkpoint", "group_imports_left_first"],
            Self::PathImportSlice => &["checkpoint", "reserve_path"],
            Self::PathImportSequent => &["intern_constraint", "constraint_depth"],
            Self::PathCheckSequent => &["checkpoint", "trace_path"],
            Self::PathCheckPairImplication => &[
                "checkpoint",
                "constraint_data",
                "reflexive_constraint",
                "reserve_path",
            ],
            Self::PathCheckSingleImplication => &[
                "checkpoint",
                "constraint_data",
                "reflexive_constraint",
                "reserve_path",
            ],
            Self::StaticMroCycle => &["checkpoint", "body_scope", "root_class", "make_error"],
            Self::BaseCursorNext => &["iteration_checkpoint"],
            Self::CollectStart | Self::CollectStartWithRoot => &["collection_checkpoint"],
            Self::CollectClassLiterals => &["literal_collection_checkpoint", "class_literal"],
            Self::OwnMember => &[
                "code_generator",
                "raw_member",
                "slot_exists",
                "generated_slots",
                "explicit_slots",
                "implicit_member",
                "is_kw_only",
                "is_enum_member",
                "is_enum_class",
                "checkpoint",
                "dataclass_fields",
                "named_tuple_field",
                "named_tuple_property",
                "dunder_paramspec",
                "constructor_context",
                "slot_descriptor",
                "synthesized_member",
                "nonmember_value",
            ],
            Self::MroClass => &[
                "known_class",
                "implicit_attribute",
                "checkpoint",
                "advance",
                "own_member",
                "push_pending",
                "clear_pending",
                "finish_pending",
                "infer_augmented",
                "union_augmented",
                "fall_back_to",
            ],
            Self::FinalizeClass => &["checkpoint", "intersect_dynamic"],
            Self::MroNext => &["iteration_checkpoint", "full_mro"],
            Self::OptionalClassSpecialization | Self::MroFirst | Self::MroTailRequest => &[
                "checkpoint",
                "generic_context",
                "generic_alias",
                "default_class_specialization",
                "tuple_runtime_specialization",
            ],
            Self::StaticMro => &[
                "body_scope",
                "is_object",
                "explicit_bases",
                "has_pep_695_type_params",
                "converted_explicit_base",
                "object_base",
                "checkpoint",
                "root_class",
                "collect_single_base_mro",
                "collect_base_mro",
                "specialize_base",
                "c3_merge",
                "make_error",
                "failed_c3",
            ],
            Self::MaybeAddGeneric => &["checkpoint"],
            Self::BaseMroStart => &["checkpoint", "object_base"],
            Self::C3Merge => &["checkpoint", "mro_identity", "start_occurrences", "publish_occurrences"],
            Self::BaseHasCyclicMro => &["checkpoint", "static_class_literal", "static_mro_is_cycle"],
            Self::ClassMroStart => &["checkpoint", "alias_origin", "alias_specialization", "compose_specialization"],
            Self::CollectBaseMro => &["checkpoint", "collect_start"],
            Self::CollectSingleBaseMro => &["checkpoint", "collect_start_with_root"],
            Self::ClassTypeOwnMember => &[
                "alias_origin",
                "alias_specialization",
                "is_tuple",
                "specialization_tuple",
                "checkpoint",
                "dynamic_member",
                "named_tuple_member",
                "typed_dict_member",
                "enum_member",
                "tuple_len",
                "tuple_getitem",
                "tuple_new",
                "tuple_runtime_specialization",
                "static_own_member",
                "owner_specialize",
            ],
            Self::SynthesizedMember => &[
                "total_ordering",
                "code_generator",
                "checkpoint",
                "total_ordering_member",
                "frozen_subclass_member",
                "generated_member",
            ],
            Self::PublicPromotion => &["checkpoint", "regular"],
            Self::SingletonPromotion => &["checkpoint", "is_singleton", "union_two"],
            Self::ProtocolCandidates => &[
                "checkpoint",
                "mro_start",
                "mro_next",
                "protocol_scope",
                "use_def_map",
                "place_table",
                "binding_place",
                "declaration_place",
                "visit_candidate",
            ],
            Self::ProtocolInterfaceCandidate => &[
                "checkpoint",
                "with_typevar_bounds",
                "specialize_candidate_type",
                "property_accessors",
                "callable_is_method_like",
                "function_is_staticmethod",
                "function_is_classmethod",
                "function_callable",
                "definition_is_function",
                "method_member",
                "descriptor_member",
            ],
            Self::ProtocolInterfaceBuild => &["checkpoint", "environment"],
            Self::ProtocolInterfaceNormalize => &["checkpoint", "normalize_member"],
            Self::ProtocolRelationDirect => &["checkpoint"],
            Self::ProtocolRelationMeta => &["checkpoint"],
            Self::ProtocolRelationRun => &[
                "checkpoint",
                "check_type_pair",
                "check_protocol_interface",
                "check_protocol_member",
                "bindings",
                "check_meta_protocol_members",
            ],
            Self::ProtocolRelationStart => &["checkpoint"],
            Self::ProtocolRelationStartMeta => &["checkpoint", "to_class_type"],
            Self::ProtocolRelationPairResume => &["checkpoint"],
            Self::ProtocolRelationAfterNominal => &[
                "checkpoint",
                "is_never_satisfied",
                "materialization_changes_requirements",
                "union_constraints",
                "nominal_class",
            ],
            Self::ProtocolRelationNonRecursiveInterface => &[
                "checkpoint",
                "nominal_class",
                "identity_specialization",
                "into_protocol_class",
                "protocol_interface",
                "non_recursive_protocol_interface",
            ],
            Self::ProtocolRelationStructural => &[
                "checkpoint",
                "has_all_protocol_members_defined",
                "protocol_interface",
            ],
            Self::ProtocolRelationNominalRecursiveMembers => &[
                "checkpoint",
                "nominal_class",
                "argument_has_unmentioned_typevar",
                "protocol_interface",
                "next_interface_member",
                "member_has_explicit_receiver_annotation",
                "reserve_member_priorities",
                "structural_member_priority",
                "push_member_priority",
                "sort_member_priorities",
            ],
            Self::ProtocolRelationFinish => &[
                "checkpoint",
                "is_never_satisfied",
                "report_error",
                "combine_constraints",
            ],
            Self::ProtocolRelationInterfaceResume => &[
                "checkpoint",
                "argument_has_unmentioned_typevar",
                "imply_constraints",
                "is_always_satisfied",
                "is_never_satisfied",
                "combine_constraints",
            ],
            Self::ProtocolRelationStructuralNext => {
                &["checkpoint", "next_interface_member", "finish_constraints"]
            }
            Self::ProtocolRelationNominalFiniteNext => &[
                "checkpoint",
                "advance_prioritized_member",
                "finish_constraints",
            ],
            Self::ProtocolRelationNominalRecursiveNext => &[
                "checkpoint",
                "advance_prioritized_member",
                "imply_constraints",
                "is_always_satisfied",
            ],
            Self::ProtocolRelationMemberResume => {
                &["checkpoint", "push_constraints", "combine_constraints"]
            }
            Self::ProtocolRelationMetaBindingsResume => &["checkpoint", "bindings_return_type"],
            Self::ProtocolRelationMetaMembersResume => &["checkpoint", "combine_constraints"],
            Self::ProtocolRelationObjectCompare => {
                &["check_type_satisfies_protocol", "is_always_satisfied"]
            }
            Self::ProtocolObject => &["checkpoint", "protocol_interface", "compare_object"],
            Self::ProtocolMembersDefined => &[
                "checkpoint",
                "protocol_interface",
                "next_interface_member",
                "non_object_member_count",
                "includes_member_or_object_fallback",
                "restricted_member",
                "member",
            ],
        };
        methods.contains(&method.to_string().as_str())
    }

    fn is_fact_method(self, method: &Ident) -> bool {
        let methods: &[&str] = match self {
            Self::Lookup(_) => &[],
            Self::Sequent(_) => &["fields"],
            Self::ConstraintConcrete => &[],
            Self::ConstraintDepth => &[],
            Self::ConstraintCachedDepth => &[],
            Self::SatisfactionRoot
            | Self::SatisfactionConjunction
            | Self::SatisfactionAssignments
            | Self::PathVisitOwned
            | Self::PathVisitBody
            | Self::PathEnterEdge
            | Self::PathDrainAssignments
            | Self::PathAddAssignment
            | Self::PathDiscoverConstraint
            | Self::PathImportSequents
            | Self::PathImportSlice
            | Self::PathImportSequent
            | Self::PathCheckSequent
            | Self::PathCheckPairImplication
            | Self::PathCheckSingleImplication => &[],
            Self::OwnMember => &[],
            Self::MroClass => &[],
            Self::SynthesizedMember => &[],
            Self::OptionalClassSpecialization
            | Self::MroFirst
            | Self::MroTailRequest
            | Self::StaticMro
            | Self::StaticMroCycle
            | Self::BaseMroStart
            | Self::BaseCursorNext
            | Self::CollectStart
            | Self::CollectStartWithRoot
            | Self::CollectClassLiterals
            | Self::FinalizeClass
            | Self::MroNext
            | Self::MaybeAddGeneric
            | Self::BaseHasCyclicMro
            | Self::ClassMroStart
            | Self::CollectBaseMro
            | Self::CollectSingleBaseMro
            | Self::C3Merge
            | Self::ClassTypeOwnMember
            | Self::PublicPromotion
            | Self::SingletonPromotion
            | Self::ProtocolCandidates
            | Self::ProtocolInterfaceCandidate
            | Self::ProtocolInterfaceBuild
            | Self::ProtocolInterfaceNormalize
            | Self::ProtocolObject
            | Self::ProtocolMembersDefined
            | Self::ProtocolRelationDirect
            | Self::ProtocolRelationMeta
            | Self::ProtocolRelationRun
            | Self::ProtocolRelationStart
            | Self::ProtocolRelationStartMeta
            | Self::ProtocolRelationPairResume
            | Self::ProtocolRelationAfterNominal
            | Self::ProtocolRelationNonRecursiveInterface
            | Self::ProtocolRelationStructural
            | Self::ProtocolRelationNominalRecursiveMembers
            | Self::ProtocolRelationFinish
            | Self::ProtocolRelationInterfaceResume
            | Self::ProtocolRelationStructuralNext
            | Self::ProtocolRelationNominalFiniteNext
            | Self::ProtocolRelationNominalRecursiveNext
            | Self::ProtocolRelationMemberResume
            | Self::ProtocolRelationMetaBindingsResume
            | Self::ProtocolRelationMetaMembersResume
            | Self::ProtocolRelationObjectCompare => &[],
        };
        methods.contains(&method.to_string().as_str())
    }

    fn helper(self, method: &Ident) -> Option<MemberHelper> {
        match self {
            Self::PublicPromotion if method == "promote_singletons_impl_with" => {
                Some(MemberHelper::SingletonPromotion)
            }
            _ => None,
        }
    }

    fn free_helper(self, name: &Ident) -> Option<FreeMemberHelper> {
        match self {
            Self::Lookup(entry) => entry.free_helper(name).map(FreeMemberHelper::Lookup),
            Self::Sequent(entry) => entry.free_helper(name).map(FreeMemberHelper::Sequent),
            Self::ConstraintCachedDepth if name == "constraint_bound_depth_with" => {
                Some(FreeMemberHelper::ConstraintDepth)
            }
            Self::SatisfactionRoot if name == "simple_conjunction_satisfiable_with" => {
                Some(FreeMemberHelper::SatisfactionConjunction)
            }
            Self::SatisfactionRoot if name == "path_assignments_with" => {
                Some(FreeMemberHelper::SatisfactionAssignments)
            }
            Self::SatisfactionRoot if name == "path_visit_owned_with" => {
                Some(FreeMemberHelper::PathVisitOwned)
            }
            Self::PathVisitOwned if name == "path_visit_body_with" => {
                Some(FreeMemberHelper::PathVisitBody)
            }
            Self::PathVisitBody if name == "path_enter_edge_with" => {
                Some(FreeMemberHelper::PathEnterEdge)
            }
            Self::PathEnterEdge if name == "path_drain_assignments_with" => {
                Some(FreeMemberHelper::PathDrainAssignments)
            }
            Self::PathDrainAssignments if name == "path_add_assignment_with" => {
                Some(FreeMemberHelper::PathAddAssignment)
            }
            Self::PathAddAssignment if name == "path_discover_constraint_with" => {
                Some(FreeMemberHelper::PathDiscoverConstraint)
            }
            Self::PathAddAssignment if name == "path_check_sequent_with" => {
                Some(FreeMemberHelper::PathCheckSequent)
            }
            Self::PathDiscoverConstraint if name == "path_import_sequents_with" => {
                Some(FreeMemberHelper::PathImportSequents)
            }
            Self::PathImportSequents if name == "path_import_slice_with" => {
                Some(FreeMemberHelper::PathImportSlice)
            }
            Self::PathImportSlice if name == "path_import_sequent_with" => {
                Some(FreeMemberHelper::PathImportSequent)
            }
            Self::PathCheckSequent if name == "path_check_pair_implication_with" => {
                Some(FreeMemberHelper::PathCheckPairImplication)
            }
            Self::PathCheckSequent if name == "path_check_single_implication_with" => {
                Some(FreeMemberHelper::PathCheckSingleImplication)
            }
            Self::BaseCursorNext | Self::CollectClassLiterals if name == "mro_next_with" => {
                Some(FreeMemberHelper::MroNext)
            }
            Self::CollectStart | Self::CollectStartWithRoot if name == "base_cursor_next_with" => {
                Some(FreeMemberHelper::BaseCursorNext)
            }
            Self::MroNext if name == "mro_first_with" => Some(FreeMemberHelper::MroFirst),
            Self::MroNext if name == "mro_tail_request_with" => {
                Some(FreeMemberHelper::MroTailRequest)
            }
            Self::MroFirst if name == "apply_optional_class_specialization_with" => {
                Some(FreeMemberHelper::OptionalClassSpecialization)
            }
            Self::StaticMro if name == "maybe_add_generic_with" => {
                Some(FreeMemberHelper::MaybeAddGeneric)
            }
            Self::StaticMro if name == "base_has_cyclic_mro_with" => {
                Some(FreeMemberHelper::BaseHasCyclicMro)
            }
            Self::BaseMroStart if name == "class_mro_start_with" => {
                Some(FreeMemberHelper::ClassMroStart)
            }
            Self::CollectBaseMro | Self::CollectSingleBaseMro if name == "base_mro_start_with" => {
                Some(FreeMemberHelper::BaseMroStart)
            }
            Self::ProtocolInterfaceBuild if name == "for_each_protocol_member_candidate_with" => {
                Some(FreeMemberHelper::ProtocolCandidates)
            }
            Self::ProtocolRelationDirect if name == "protocol_relation_start_with" => {
                Some(FreeMemberHelper::ProtocolRelationStart)
            }
            Self::ProtocolRelationDirect if name == "protocol_relation_run_with" => {
                Some(FreeMemberHelper::ProtocolRelationRun)
            }
            Self::ProtocolRelationMeta if name == "protocol_relation_start_meta_with" => {
                Some(FreeMemberHelper::ProtocolRelationStartMeta)
            }
            Self::ProtocolRelationMeta if name == "protocol_relation_run_with" => {
                Some(FreeMemberHelper::ProtocolRelationRun)
            }
            Self::ProtocolRelationRun if name == "protocol_relation_pair_resume_with" => {
                Some(FreeMemberHelper::ProtocolRelationPairResume)
            }
            Self::ProtocolRelationRun if name == "protocol_relation_interface_resume_with" => {
                Some(FreeMemberHelper::ProtocolRelationInterfaceResume)
            }
            Self::ProtocolRelationRun if name == "protocol_relation_member_resume_with" => {
                Some(FreeMemberHelper::ProtocolRelationMemberResume)
            }
            Self::ProtocolRelationRun if name == "protocol_relation_meta_bindings_resume_with" => {
                Some(FreeMemberHelper::ProtocolRelationMetaBindingsResume)
            }
            Self::ProtocolRelationRun if name == "protocol_relation_meta_members_resume_with" => {
                Some(FreeMemberHelper::ProtocolRelationMetaMembersResume)
            }
            Self::ProtocolRelationStart if name == "protocol_relation_structural_with" => {
                Some(FreeMemberHelper::ProtocolRelationStructural)
            }
            Self::ProtocolRelationPairResume if name == "protocol_relation_after_nominal_with" => {
                Some(FreeMemberHelper::ProtocolRelationAfterNominal)
            }
            Self::ProtocolRelationAfterNominal
                if name == "protocol_relation_non_recursive_interface_with" =>
            {
                Some(FreeMemberHelper::ProtocolRelationNonRecursiveInterface)
            }
            Self::ProtocolRelationAfterNominal if name == "protocol_relation_structural_with" => {
                Some(FreeMemberHelper::ProtocolRelationStructural)
            }
            Self::ProtocolRelationStructural
                if name == "protocol_relation_nominal_recursive_members_with" =>
            {
                Some(FreeMemberHelper::ProtocolRelationNominalRecursiveMembers)
            }
            Self::ProtocolRelationStructural
                if name == "protocol_relation_nominal_finite_next_with" =>
            {
                Some(FreeMemberHelper::ProtocolRelationNominalFiniteNext)
            }
            Self::ProtocolRelationStructural
                if name == "protocol_relation_structural_next_with" =>
            {
                Some(FreeMemberHelper::ProtocolRelationStructuralNext)
            }
            Self::ProtocolRelationInterfaceResume if name == "protocol_relation_finish_with" => {
                Some(FreeMemberHelper::ProtocolRelationFinish)
            }
            Self::ProtocolRelationInterfaceResume
                if name == "protocol_relation_structural_with" =>
            {
                Some(FreeMemberHelper::ProtocolRelationStructural)
            }
            Self::ProtocolRelationStructuralNext if name == "protocol_relation_finish_with" => {
                Some(FreeMemberHelper::ProtocolRelationFinish)
            }
            Self::ProtocolRelationNominalFiniteNext
                if name == "protocol_relation_nominal_recursive_next_with" =>
            {
                Some(FreeMemberHelper::ProtocolRelationNominalRecursiveNext)
            }
            Self::ProtocolRelationNominalRecursiveNext
                if name == "protocol_relation_finish_with" =>
            {
                Some(FreeMemberHelper::ProtocolRelationFinish)
            }
            Self::ProtocolRelationMemberResume if name == "protocol_relation_finish_with" => {
                Some(FreeMemberHelper::ProtocolRelationFinish)
            }
            Self::ProtocolRelationMemberResume
                if name == "protocol_relation_structural_next_with" =>
            {
                Some(FreeMemberHelper::ProtocolRelationStructuralNext)
            }
            Self::ProtocolRelationMemberResume
                if name == "protocol_relation_nominal_finite_next_with" =>
            {
                Some(FreeMemberHelper::ProtocolRelationNominalFiniteNext)
            }
            Self::ProtocolRelationMemberResume
                if name == "protocol_relation_nominal_recursive_next_with" =>
            {
                Some(FreeMemberHelper::ProtocolRelationNominalRecursiveNext)
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum MemberHelper {
    SingletonPromotion,
}

#[derive(Clone, Copy)]
enum FreeMemberHelper {
    Lookup(LookupManifest),
    Sequent(SequentManifest),
    ConstraintDepth,

    SatisfactionConjunction,
    SatisfactionAssignments,
    PathVisitOwned,
    PathVisitBody,
    PathEnterEdge,
    PathDrainAssignments,
    PathAddAssignment,
    PathDiscoverConstraint,
    PathCheckSequent,
    PathImportSequents,
    PathImportSlice,
    PathImportSequent,
    PathCheckPairImplication,
    PathCheckSingleImplication,
    MroNext,
    BaseCursorNext,
    MroFirst,
    MroTailRequest,
    OptionalClassSpecialization,
    MaybeAddGeneric,
    BaseHasCyclicMro,
    ClassMroStart,
    BaseMroStart,
    ProtocolCandidates,
    ProtocolRelationRun,
    ProtocolRelationStart,
    ProtocolRelationStartMeta,
    ProtocolRelationPairResume,
    ProtocolRelationAfterNominal,
    ProtocolRelationNonRecursiveInterface,
    ProtocolRelationStructural,
    ProtocolRelationNominalRecursiveMembers,
    ProtocolRelationFinish,
    ProtocolRelationInterfaceResume,
    ProtocolRelationStructuralNext,
    ProtocolRelationNominalFiniteNext,
    ProtocolRelationNominalRecursiveNext,
    ProtocolRelationMemberResume,
    ProtocolRelationMetaBindingsResume,
    ProtocolRelationMetaMembersResume,
}

impl FreeMemberHelper {
    fn argument_count(self) -> usize {
        match self {
            Self::Lookup(entry) => entry.argument_count(),
            Self::Sequent(entry) => entry.argument_count(),
            Self::ConstraintDepth => 2,
            Self::SatisfactionConjunction => 2,
            Self::SatisfactionAssignments => 3,
            Self::PathVisitOwned => 5,
            Self::PathVisitBody => 3,
            Self::PathEnterEdge => 3,
            Self::PathDrainAssignments => 3,
            Self::PathAddAssignment => 5,
            Self::PathDiscoverConstraint => 3,
            Self::PathCheckSequent => 3,
            Self::PathImportSequents => 3,
            Self::PathImportSlice => 3,
            Self::PathImportSequent => 2,
            Self::PathCheckPairImplication => 6,
            Self::PathCheckSingleImplication => 5,
            Self::ProtocolRelationRun => 3,
            Self::ProtocolRelationStart => 5,
            Self::ProtocolRelationStartMeta => 5,
            Self::ProtocolRelationPairResume => 4,
            Self::ProtocolRelationAfterNominal => 6,
            Self::ProtocolRelationNonRecursiveInterface => 5,
            Self::ProtocolRelationStructural => 4,
            Self::ProtocolRelationNominalRecursiveMembers => 4,
            Self::ProtocolRelationFinish => 5,
            Self::ProtocolRelationInterfaceResume => 4,
            Self::ProtocolRelationStructuralNext => 3,
            Self::ProtocolRelationNominalFiniteNext => 3,
            Self::ProtocolRelationNominalRecursiveNext => 4,
            Self::ProtocolRelationMemberResume => 4,
            Self::ProtocolRelationMetaBindingsResume => 4,
            Self::ProtocolRelationMetaMembersResume => 4,
            Self::MroNext => 4,
            Self::BaseCursorNext => 3,
            Self::OptionalClassSpecialization
            | Self::MaybeAddGeneric
            | Self::ClassMroStart
            | Self::MroFirst
            | Self::MroTailRequest
            | Self::ProtocolCandidates => 4,
            Self::BaseHasCyclicMro => 3,
            Self::BaseMroStart => 5,
        }
    }

    fn synchronous_name(self) -> &'static str {
        match self {
            Self::Lookup(entry) => entry.synchronous_name(),
            Self::Sequent(entry) => entry.synchronous_name(),
            Self::ConstraintDepth => "constraint_bound_depth_sync",
            Self::SatisfactionConjunction => "simple_conjunction_satisfiable_sync",
            Self::SatisfactionAssignments => "path_assignments_sync",
            Self::PathVisitOwned => "path_visit_owned_sync",
            Self::PathVisitBody => "path_visit_body_sync",
            Self::PathEnterEdge => "path_enter_edge_sync",
            Self::PathDrainAssignments => "path_drain_assignments_sync",
            Self::PathAddAssignment => "path_add_assignment_sync",
            Self::PathDiscoverConstraint => "path_discover_constraint_sync",
            Self::PathCheckSequent => "path_check_sequent_sync",
            Self::PathImportSequents => "path_import_sequents_sync",
            Self::PathImportSlice => "path_import_slice_sync",
            Self::PathImportSequent => "path_import_sequent_sync",
            Self::PathCheckPairImplication => "path_check_pair_implication_sync",
            Self::PathCheckSingleImplication => "path_check_single_implication_sync",
            Self::ProtocolRelationRun => "protocol_relation_run_sync",
            Self::ProtocolRelationStart => "protocol_relation_start_sync",
            Self::ProtocolRelationStartMeta => "protocol_relation_start_meta_sync",
            Self::ProtocolRelationPairResume => "protocol_relation_pair_resume_sync",
            Self::ProtocolRelationAfterNominal => "protocol_relation_after_nominal_sync",
            Self::ProtocolRelationNonRecursiveInterface => {
                "protocol_relation_non_recursive_interface_sync"
            }
            Self::ProtocolRelationStructural => "protocol_relation_structural_sync",
            Self::ProtocolRelationNominalRecursiveMembers => {
                "protocol_relation_nominal_recursive_members_sync"
            }
            Self::ProtocolRelationFinish => "protocol_relation_finish_sync",
            Self::ProtocolRelationInterfaceResume => "protocol_relation_interface_resume_sync",
            Self::ProtocolRelationStructuralNext => "protocol_relation_structural_next_sync",
            Self::ProtocolRelationNominalFiniteNext => "protocol_relation_nominal_finite_next_sync",
            Self::ProtocolRelationNominalRecursiveNext => {
                "protocol_relation_nominal_recursive_next_sync"
            }
            Self::ProtocolRelationMemberResume => "protocol_relation_member_resume_sync",
            Self::ProtocolRelationMetaBindingsResume => {
                "protocol_relation_meta_bindings_resume_sync"
            }
            Self::ProtocolRelationMetaMembersResume => "protocol_relation_meta_members_resume_sync",
            Self::MroNext => "mro_next_sync",
            Self::BaseCursorNext => "base_cursor_next_sync",
            Self::MroFirst => "mro_first_sync",
            Self::MroTailRequest => "mro_tail_request_sync",
            Self::OptionalClassSpecialization => "apply_optional_class_specialization_sync",
            Self::MaybeAddGeneric => "maybe_add_generic_sync",
            Self::BaseHasCyclicMro => "base_has_cyclic_mro_sync",
            Self::ClassMroStart => "class_mro_start_sync",
            Self::BaseMroStart => "base_mro_start_sync",
            Self::ProtocolCandidates => "for_each_protocol_member_candidate_sync",
        }
    }
}

fn is_declared_free_helper(name: &Ident) -> bool {
    LookupManifest::storage_from_name(name, false).is_some()
        || LookupManifest::storage_from_name(name, true).is_some()
        || SequentManifest::from_name(name).is_some()
        || SequentManifest::is_raw_name(name)
        || name == "constraint_as_concrete_with"
        || name == "r#constraint_as_concrete_with"
        || name == "constraint_bound_depth_with"
        || name == "r#constraint_bound_depth_with"
        || name == "cached_constraint_bound_depth_with"
        || name == "r#cached_constraint_bound_depth_with"
        || name == "node_satisfaction_with"
        || name == "r#node_satisfaction_with"
        || name == "simple_conjunction_satisfiable_with"
        || name == "r#simple_conjunction_satisfiable_with"
        || name == "path_assignments_with"
        || name == "r#path_assignments_with"
        || name == "path_visit_owned_with"
        || name == "r#path_visit_owned_with"
        || name == "path_visit_body_with"
        || name == "r#path_visit_body_with"
        || name == "path_enter_edge_with"
        || name == "r#path_enter_edge_with"
        || name == "path_drain_assignments_with"
        || name == "r#path_drain_assignments_with"
        || name == "path_add_assignment_with"
        || name == "r#path_add_assignment_with"
        || name == "path_discover_constraint_with"
        || name == "r#path_discover_constraint_with"
        || name == "path_import_sequents_with"
        || name == "r#path_import_sequents_with"
        || name == "path_import_slice_with"
        || name == "r#path_import_slice_with"
        || name == "path_import_sequent_with"
        || name == "r#path_import_sequent_with"
        || name == "path_check_sequent_with"
        || name == "r#path_check_sequent_with"
        || name == "path_check_pair_implication_with"
        || name == "r#path_check_pair_implication_with"
        || name == "path_check_single_implication_with"
        || name == "r#path_check_single_implication_with"
        || name == "mro_next_with"
        || name == "r#mro_next_with"
        || name == "base_cursor_next_with"
        || name == "r#base_cursor_next_with"
        || name == "mro_first_with"
        || name == "r#mro_first_with"
        || name == "mro_tail_request_with"
        || name == "r#mro_tail_request_with"
        || name == "apply_optional_class_specialization_with"
        || name == "r#apply_optional_class_specialization_with"
        || name == "maybe_add_generic_with"
        || name == "r#maybe_add_generic_with"
        || name == "base_has_cyclic_mro_with"
        || name == "r#base_has_cyclic_mro_with"
        || name == "class_mro_start_with"
        || name == "r#class_mro_start_with"
        || name == "base_mro_start_with"
        || name == "r#base_mro_start_with"
        || name == "for_each_protocol_member_candidate_with"
        || name == "r#for_each_protocol_member_candidate_with"
        || name == "protocol_relation_run_with"
        || name == "r#protocol_relation_run_with"
        || name == "protocol_relation_start_with"
        || name == "r#protocol_relation_start_with"
        || name == "protocol_relation_start_meta_with"
        || name == "r#protocol_relation_start_meta_with"
        || name == "protocol_relation_pair_resume_with"
        || name == "r#protocol_relation_pair_resume_with"
        || name == "protocol_relation_after_nominal_with"
        || name == "r#protocol_relation_after_nominal_with"
        || name == "protocol_relation_non_recursive_interface_with"
        || name == "r#protocol_relation_non_recursive_interface_with"
        || name == "protocol_relation_structural_with"
        || name == "r#protocol_relation_structural_with"
        || name == "protocol_relation_nominal_recursive_members_with"
        || name == "r#protocol_relation_nominal_recursive_members_with"
        || name == "protocol_relation_finish_with"
        || name == "r#protocol_relation_finish_with"
        || name == "protocol_relation_interface_resume_with"
        || name == "r#protocol_relation_interface_resume_with"
        || name == "protocol_relation_structural_next_with"
        || name == "r#protocol_relation_structural_next_with"
        || name == "protocol_relation_nominal_finite_next_with"
        || name == "r#protocol_relation_nominal_finite_next_with"
        || name == "protocol_relation_nominal_recursive_next_with"
        || name == "r#protocol_relation_nominal_recursive_next_with"
        || name == "protocol_relation_member_resume_with"
        || name == "r#protocol_relation_member_resume_with"
        || name == "protocol_relation_meta_bindings_resume_with"
        || name == "r#protocol_relation_meta_bindings_resume_with"
        || name == "protocol_relation_meta_members_resume_with"
        || name == "r#protocol_relation_meta_members_resume_with"
}

fn validate_signature(signature: &Signature, manifest: MemberManifest) -> Result<()> {
    if signature.asyncness.is_none() {
        return Err(Error::new_spanned(
            &signature.ident,
            format!("{} requires an async function", manifest.attribute().name()),
        ));
    }

    let expected = manifest.expected_signature();
    let where_clause_matches = match (
        &signature.generics.where_clause,
        &expected.generics.where_clause,
    ) {
        (None, None) => true,
        (Some(actual), Some(expected)) => actual.predicates.iter().eq(expected.predicates.iter()),
        _ => false,
    };
    let inputs_match = signature.inputs.len() == expected.inputs.len()
        && signature
            .inputs
            .iter()
            .zip(&expected.inputs)
            .all(|(actual, expected)| match (actual, expected) {
                (FnArg::Typed(actual), FnArg::Typed(expected)) => {
                    actual.pat == expected.pat && actual.ty == expected.ty
                }
                (FnArg::Receiver(actual), FnArg::Receiver(expected)) => actual == expected,
                _ => false,
            });
    if signature.constness.is_some()
        || signature.safety != expected.safety
        || signature.abi.is_some()
        || signature.variadic.is_some()
        || !signature
            .generics
            .params
            .iter()
            .eq(expected.generics.params.iter())
        || !where_clause_matches
        || !inputs_match
        || signature.output != expected.output
    {
        return Err(Error::new_spanned(signature, manifest.signature_error()));
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum SequentManifest {
    SingleSequents,
    PairSequents,
    PairCannotProduce,
    ConstraintSequents,
    ConstraintPairSequents,
    LowerSequents,
    UpperSequents,
    EquivalenceSequents,
    RangeSequents,
    TypevarEquivalenceSequents,
    LowerPairLower,
    LowerPairUpper,
    LowerPairEquivalence,
    LowerPairRange,
    LowerPairTypevarEquivalence,
    UpperPairUpper,
    UpperPairEquivalence,
    UpperPairRange,
    UpperPairTypevarEquivalence,
    EquivalencePairEquivalence,
    EquivalencePairRange,
    EquivalencePairTypevarEquivalence,
    RangePairRange,
    RangePairTypevarEquivalence,
    TypevarEquivalencePairTypevarEquivalence,
    AddSequentsForRange,
    AddSequentsForEquivalence,
    AddConstraintSetImplication,
    SubstituteIfNotRecursive,
    AddCovariantLowerTightenedSequent,
    AddCovariantUpperTightenedSequent,
    AddCovariantEquivalenceTightenedSequent,
    AddContravariantTightenedSequent,
    AddInvariantTightenedSequent,
    AddCovariantLowerWeakenedSequent,
    AddCovariantUpperWeakenedSequent,
    AddCovariantEquivalenceWeakenedSequent,
    AddContravariantLowerWeakenedSequent,
    AddContravariantUpperWeakenedSequent,
    AddContravariantEquivalenceWeakenedSequent,
    AddInvariantWeakenedSequent,
    PossiblyReversedIntersection,
    PossiblyReversedUnion,
    DeriveGroupDirection,
    DeriveGroup,
    ExtractPending,
    FlushPending,
    FinishSequents,
}

impl SequentManifest {
    fn from_name(name: &Ident) -> Option<Self> {
        if name == "single_sequents_with" {
            return Some(Self::SingleSequents);
        }
        if name == "pair_sequents_with" {
            return Some(Self::PairSequents);
        }
        if name == "pair_cannot_produce_with" {
            return Some(Self::PairCannotProduce);
        }
        if name == "constraint_sequents_with" {
            return Some(Self::ConstraintSequents);
        }
        if name == "constraint_pair_sequents_with" {
            return Some(Self::ConstraintPairSequents);
        }
        if name == "lower_sequents_with" {
            return Some(Self::LowerSequents);
        }
        if name == "upper_sequents_with" {
            return Some(Self::UpperSequents);
        }
        if name == "equivalence_sequents_with" {
            return Some(Self::EquivalenceSequents);
        }
        if name == "range_sequents_with" {
            return Some(Self::RangeSequents);
        }
        if name == "typevar_equivalence_sequents_with" {
            return Some(Self::TypevarEquivalenceSequents);
        }
        if name == "lower_pair_lower_with" {
            return Some(Self::LowerPairLower);
        }
        if name == "lower_pair_upper_with" {
            return Some(Self::LowerPairUpper);
        }
        if name == "lower_pair_equivalence_with" {
            return Some(Self::LowerPairEquivalence);
        }
        if name == "lower_pair_range_with" {
            return Some(Self::LowerPairRange);
        }
        if name == "lower_pair_typevar_equivalence_with" {
            return Some(Self::LowerPairTypevarEquivalence);
        }
        if name == "upper_pair_upper_with" {
            return Some(Self::UpperPairUpper);
        }
        if name == "upper_pair_equivalence_with" {
            return Some(Self::UpperPairEquivalence);
        }
        if name == "upper_pair_range_with" {
            return Some(Self::UpperPairRange);
        }
        if name == "upper_pair_typevar_equivalence_with" {
            return Some(Self::UpperPairTypevarEquivalence);
        }
        if name == "equivalence_pair_equivalence_with" {
            return Some(Self::EquivalencePairEquivalence);
        }
        if name == "equivalence_pair_range_with" {
            return Some(Self::EquivalencePairRange);
        }
        if name == "equivalence_pair_typevar_equivalence_with" {
            return Some(Self::EquivalencePairTypevarEquivalence);
        }
        if name == "range_pair_range_with" {
            return Some(Self::RangePairRange);
        }
        if name == "range_pair_typevar_equivalence_with" {
            return Some(Self::RangePairTypevarEquivalence);
        }
        if name == "typevar_equivalence_pair_typevar_equivalence_with" {
            return Some(Self::TypevarEquivalencePairTypevarEquivalence);
        }
        if name == "add_sequents_for_range_with" {
            return Some(Self::AddSequentsForRange);
        }
        if name == "add_sequents_for_equivalence_with" {
            return Some(Self::AddSequentsForEquivalence);
        }
        if name == "add_constraint_set_implication_with" {
            return Some(Self::AddConstraintSetImplication);
        }
        if name == "substitute_if_not_recursive_with" {
            return Some(Self::SubstituteIfNotRecursive);
        }
        if name == "add_covariant_lower_tightened_sequent_with" {
            return Some(Self::AddCovariantLowerTightenedSequent);
        }
        if name == "add_covariant_upper_tightened_sequent_with" {
            return Some(Self::AddCovariantUpperTightenedSequent);
        }
        if name == "add_covariant_equivalence_tightened_sequent_with" {
            return Some(Self::AddCovariantEquivalenceTightenedSequent);
        }
        if name == "add_contravariant_tightened_sequent_with" {
            return Some(Self::AddContravariantTightenedSequent);
        }
        if name == "add_invariant_tightened_sequent_with" {
            return Some(Self::AddInvariantTightenedSequent);
        }
        if name == "add_covariant_lower_weakened_sequent_with" {
            return Some(Self::AddCovariantLowerWeakenedSequent);
        }
        if name == "add_covariant_upper_weakened_sequent_with" {
            return Some(Self::AddCovariantUpperWeakenedSequent);
        }
        if name == "add_covariant_equivalence_weakened_sequent_with" {
            return Some(Self::AddCovariantEquivalenceWeakenedSequent);
        }
        if name == "add_contravariant_lower_weakened_sequent_with" {
            return Some(Self::AddContravariantLowerWeakenedSequent);
        }
        if name == "add_contravariant_upper_weakened_sequent_with" {
            return Some(Self::AddContravariantUpperWeakenedSequent);
        }
        if name == "add_contravariant_equivalence_weakened_sequent_with" {
            return Some(Self::AddContravariantEquivalenceWeakenedSequent);
        }
        if name == "add_invariant_weakened_sequent_with" {
            return Some(Self::AddInvariantWeakenedSequent);
        }
        if name == "possibly_reversed_intersection_with" {
            return Some(Self::PossiblyReversedIntersection);
        }
        if name == "possibly_reversed_union_with" {
            return Some(Self::PossiblyReversedUnion);
        }
        if name == "derive_group_direction_with" {
            return Some(Self::DeriveGroupDirection);
        }
        if name == "derive_group_with" {
            return Some(Self::DeriveGroup);
        }
        if name == "extract_pending_with" {
            return Some(Self::ExtractPending);
        }
        if name == "flush_pending_with" {
            return Some(Self::FlushPending);
        }
        if name == "finish_sequents_with" {
            return Some(Self::FinishSequents);
        }
        None
    }
    fn is_raw_name(name: &Ident) -> bool {
        name == "r#single_sequents_with"
            || name == "r#pair_sequents_with"
            || name == "r#pair_cannot_produce_with"
            || name == "r#constraint_sequents_with"
            || name == "r#constraint_pair_sequents_with"
            || name == "r#lower_sequents_with"
            || name == "r#upper_sequents_with"
            || name == "r#equivalence_sequents_with"
            || name == "r#range_sequents_with"
            || name == "r#typevar_equivalence_sequents_with"
            || name == "r#lower_pair_lower_with"
            || name == "r#lower_pair_upper_with"
            || name == "r#lower_pair_equivalence_with"
            || name == "r#lower_pair_range_with"
            || name == "r#lower_pair_typevar_equivalence_with"
            || name == "r#upper_pair_upper_with"
            || name == "r#upper_pair_equivalence_with"
            || name == "r#upper_pair_range_with"
            || name == "r#upper_pair_typevar_equivalence_with"
            || name == "r#equivalence_pair_equivalence_with"
            || name == "r#equivalence_pair_range_with"
            || name == "r#equivalence_pair_typevar_equivalence_with"
            || name == "r#range_pair_range_with"
            || name == "r#range_pair_typevar_equivalence_with"
            || name == "r#typevar_equivalence_pair_typevar_equivalence_with"
            || name == "r#add_sequents_for_range_with"
            || name == "r#add_sequents_for_equivalence_with"
            || name == "r#add_constraint_set_implication_with"
            || name == "r#substitute_if_not_recursive_with"
            || name == "r#add_covariant_lower_tightened_sequent_with"
            || name == "r#add_covariant_upper_tightened_sequent_with"
            || name == "r#add_covariant_equivalence_tightened_sequent_with"
            || name == "r#add_contravariant_tightened_sequent_with"
            || name == "r#add_invariant_tightened_sequent_with"
            || name == "r#add_covariant_lower_weakened_sequent_with"
            || name == "r#add_covariant_upper_weakened_sequent_with"
            || name == "r#add_covariant_equivalence_weakened_sequent_with"
            || name == "r#add_contravariant_lower_weakened_sequent_with"
            || name == "r#add_contravariant_upper_weakened_sequent_with"
            || name == "r#add_contravariant_equivalence_weakened_sequent_with"
            || name == "r#add_invariant_weakened_sequent_with"
            || name == "r#possibly_reversed_intersection_with"
            || name == "r#possibly_reversed_union_with"
            || name == "r#derive_group_direction_with"
            || name == "r#derive_group_with"
            || name == "r#extract_pending_with"
            || name == "r#flush_pending_with"
            || name == "r#finish_sequents_with"
    }
    fn body_name(self) -> &'static str {
        match self {
            Self::SingleSequents => "single_sequents_with",
            Self::PairSequents => "pair_sequents_with",
            Self::PairCannotProduce => "pair_cannot_produce_with",
            Self::ConstraintSequents => "constraint_sequents_with",
            Self::ConstraintPairSequents => "constraint_pair_sequents_with",
            Self::LowerSequents => "lower_sequents_with",
            Self::UpperSequents => "upper_sequents_with",
            Self::EquivalenceSequents => "equivalence_sequents_with",
            Self::RangeSequents => "range_sequents_with",
            Self::TypevarEquivalenceSequents => "typevar_equivalence_sequents_with",
            Self::LowerPairLower => "lower_pair_lower_with",
            Self::LowerPairUpper => "lower_pair_upper_with",
            Self::LowerPairEquivalence => "lower_pair_equivalence_with",
            Self::LowerPairRange => "lower_pair_range_with",
            Self::LowerPairTypevarEquivalence => "lower_pair_typevar_equivalence_with",
            Self::UpperPairUpper => "upper_pair_upper_with",
            Self::UpperPairEquivalence => "upper_pair_equivalence_with",
            Self::UpperPairRange => "upper_pair_range_with",
            Self::UpperPairTypevarEquivalence => "upper_pair_typevar_equivalence_with",
            Self::EquivalencePairEquivalence => "equivalence_pair_equivalence_with",
            Self::EquivalencePairRange => "equivalence_pair_range_with",
            Self::EquivalencePairTypevarEquivalence => "equivalence_pair_typevar_equivalence_with",
            Self::RangePairRange => "range_pair_range_with",
            Self::RangePairTypevarEquivalence => "range_pair_typevar_equivalence_with",
            Self::TypevarEquivalencePairTypevarEquivalence => {
                "typevar_equivalence_pair_typevar_equivalence_with"
            }
            Self::AddSequentsForRange => "add_sequents_for_range_with",
            Self::AddSequentsForEquivalence => "add_sequents_for_equivalence_with",
            Self::AddConstraintSetImplication => "add_constraint_set_implication_with",
            Self::SubstituteIfNotRecursive => "substitute_if_not_recursive_with",
            Self::AddCovariantLowerTightenedSequent => "add_covariant_lower_tightened_sequent_with",
            Self::AddCovariantUpperTightenedSequent => "add_covariant_upper_tightened_sequent_with",
            Self::AddCovariantEquivalenceTightenedSequent => {
                "add_covariant_equivalence_tightened_sequent_with"
            }
            Self::AddContravariantTightenedSequent => "add_contravariant_tightened_sequent_with",
            Self::AddInvariantTightenedSequent => "add_invariant_tightened_sequent_with",
            Self::AddCovariantLowerWeakenedSequent => "add_covariant_lower_weakened_sequent_with",
            Self::AddCovariantUpperWeakenedSequent => "add_covariant_upper_weakened_sequent_with",
            Self::AddCovariantEquivalenceWeakenedSequent => {
                "add_covariant_equivalence_weakened_sequent_with"
            }
            Self::AddContravariantLowerWeakenedSequent => {
                "add_contravariant_lower_weakened_sequent_with"
            }
            Self::AddContravariantUpperWeakenedSequent => {
                "add_contravariant_upper_weakened_sequent_with"
            }
            Self::AddContravariantEquivalenceWeakenedSequent => {
                "add_contravariant_equivalence_weakened_sequent_with"
            }
            Self::AddInvariantWeakenedSequent => "add_invariant_weakened_sequent_with",
            Self::PossiblyReversedIntersection => "possibly_reversed_intersection_with",
            Self::PossiblyReversedUnion => "possibly_reversed_union_with",
            Self::DeriveGroupDirection => "derive_group_direction_with",
            Self::DeriveGroup => "derive_group_with",
            Self::ExtractPending => "extract_pending_with",
            Self::FlushPending => "flush_pending_with",
            Self::FinishSequents => "finish_sequents_with",
        }
    }
    fn synchronous_name(self) -> &'static str {
        match self {
            Self::SingleSequents => "single_sequents_sync",
            Self::PairSequents => "pair_sequents_sync",
            Self::PairCannotProduce => "pair_cannot_produce_sync",
            Self::ConstraintSequents => "constraint_sequents_sync",
            Self::ConstraintPairSequents => "constraint_pair_sequents_sync",
            Self::LowerSequents => "lower_sequents_sync",
            Self::UpperSequents => "upper_sequents_sync",
            Self::EquivalenceSequents => "equivalence_sequents_sync",
            Self::RangeSequents => "range_sequents_sync",
            Self::TypevarEquivalenceSequents => "typevar_equivalence_sequents_sync",
            Self::LowerPairLower => "lower_pair_lower_sync",
            Self::LowerPairUpper => "lower_pair_upper_sync",
            Self::LowerPairEquivalence => "lower_pair_equivalence_sync",
            Self::LowerPairRange => "lower_pair_range_sync",
            Self::LowerPairTypevarEquivalence => "lower_pair_typevar_equivalence_sync",
            Self::UpperPairUpper => "upper_pair_upper_sync",
            Self::UpperPairEquivalence => "upper_pair_equivalence_sync",
            Self::UpperPairRange => "upper_pair_range_sync",
            Self::UpperPairTypevarEquivalence => "upper_pair_typevar_equivalence_sync",
            Self::EquivalencePairEquivalence => "equivalence_pair_equivalence_sync",
            Self::EquivalencePairRange => "equivalence_pair_range_sync",
            Self::EquivalencePairTypevarEquivalence => "equivalence_pair_typevar_equivalence_sync",
            Self::RangePairRange => "range_pair_range_sync",
            Self::RangePairTypevarEquivalence => "range_pair_typevar_equivalence_sync",
            Self::TypevarEquivalencePairTypevarEquivalence => {
                "typevar_equivalence_pair_typevar_equivalence_sync"
            }
            Self::AddSequentsForRange => "add_sequents_for_range_sync",
            Self::AddSequentsForEquivalence => "add_sequents_for_equivalence_sync",
            Self::AddConstraintSetImplication => "add_constraint_set_implication_sync",
            Self::SubstituteIfNotRecursive => "substitute_if_not_recursive_sync",
            Self::AddCovariantLowerTightenedSequent => "add_covariant_lower_tightened_sequent_sync",
            Self::AddCovariantUpperTightenedSequent => "add_covariant_upper_tightened_sequent_sync",
            Self::AddCovariantEquivalenceTightenedSequent => {
                "add_covariant_equivalence_tightened_sequent_sync"
            }
            Self::AddContravariantTightenedSequent => "add_contravariant_tightened_sequent_sync",
            Self::AddInvariantTightenedSequent => "add_invariant_tightened_sequent_sync",
            Self::AddCovariantLowerWeakenedSequent => "add_covariant_lower_weakened_sequent_sync",
            Self::AddCovariantUpperWeakenedSequent => "add_covariant_upper_weakened_sequent_sync",
            Self::AddCovariantEquivalenceWeakenedSequent => {
                "add_covariant_equivalence_weakened_sequent_sync"
            }
            Self::AddContravariantLowerWeakenedSequent => {
                "add_contravariant_lower_weakened_sequent_sync"
            }
            Self::AddContravariantUpperWeakenedSequent => {
                "add_contravariant_upper_weakened_sequent_sync"
            }
            Self::AddContravariantEquivalenceWeakenedSequent => {
                "add_contravariant_equivalence_weakened_sequent_sync"
            }
            Self::AddInvariantWeakenedSequent => "add_invariant_weakened_sequent_sync",
            Self::PossiblyReversedIntersection => "possibly_reversed_intersection_sync",
            Self::PossiblyReversedUnion => "possibly_reversed_union_sync",
            Self::DeriveGroupDirection => "derive_group_direction_sync",
            Self::DeriveGroup => "derive_group_sync",
            Self::ExtractPending => "extract_pending_sync",
            Self::FlushPending => "flush_pending_sync",
            Self::FinishSequents => "finish_sequents_sync",
        }
    }
    fn argument_count(self) -> usize {
        match self {
            Self::SingleSequents => 2,
            Self::PairSequents => 3,
            Self::PairCannotProduce => 3,
            Self::ConstraintSequents => 3,
            Self::ConstraintPairSequents => 4,
            Self::LowerSequents => 3,
            Self::UpperSequents => 3,
            Self::EquivalenceSequents => 3,
            Self::RangeSequents => 3,
            Self::TypevarEquivalenceSequents => 3,
            Self::LowerPairLower => 5,
            Self::LowerPairUpper => 5,
            Self::LowerPairEquivalence => 5,
            Self::LowerPairRange => 5,
            Self::LowerPairTypevarEquivalence => 5,
            Self::UpperPairUpper => 5,
            Self::UpperPairEquivalence => 5,
            Self::UpperPairRange => 5,
            Self::UpperPairTypevarEquivalence => 5,
            Self::EquivalencePairEquivalence => 5,
            Self::EquivalencePairRange => 5,
            Self::EquivalencePairTypevarEquivalence => 5,
            Self::RangePairRange => 5,
            Self::RangePairTypevarEquivalence => 5,
            Self::TypevarEquivalencePairTypevarEquivalence => 5,
            Self::AddSequentsForRange => 4,
            Self::AddSequentsForEquivalence => 4,
            Self::AddConstraintSetImplication => 5,
            Self::SubstituteIfNotRecursive => 6,
            Self::AddCovariantLowerTightenedSequent => 4,
            Self::AddCovariantUpperTightenedSequent => 4,
            Self::AddCovariantEquivalenceTightenedSequent => 4,
            Self::AddContravariantTightenedSequent => 4,
            Self::AddInvariantTightenedSequent => 4,
            Self::AddCovariantLowerWeakenedSequent => 4,
            Self::AddCovariantUpperWeakenedSequent => 4,
            Self::AddCovariantEquivalenceWeakenedSequent => 4,
            Self::AddContravariantLowerWeakenedSequent => 4,
            Self::AddContravariantUpperWeakenedSequent => 4,
            Self::AddContravariantEquivalenceWeakenedSequent => 4,
            Self::AddInvariantWeakenedSequent => 4,
            Self::PossiblyReversedIntersection => 4,
            Self::PossiblyReversedUnion => 4,
            Self::DeriveGroupDirection => 4,
            Self::DeriveGroup => 4,
            Self::ExtractPending => 2,
            Self::FlushPending => 2,
            Self::FinishSequents => 2,
        }
    }
    fn expected_signature(self) -> Signature {
        match self {
            Self::SingleSequents => {
                syn::parse_quote! { async fn single_sequents_with<'db, E: SequentEffects<'db>>(constraint: Constraint<'db>, effects: &mut E) -> Result<SequentMap<'db>, E::Error> }
            }
            Self::PairSequents => {
                syn::parse_quote! { async fn pair_sequents_with<'db, E: SequentEffects<'db>>(left: Constraint<'db>, right: Constraint<'db>, effects: &mut E) -> Result<SequentMap<'db>, E::Error> }
            }
            Self::PairCannotProduce => {
                syn::parse_quote! { async fn pair_cannot_produce_with<'db, E: SequentEffects<'db>>(left: Constraint<'db>, right: Constraint<'db>, effects: &mut E) -> Result<bool, E::Error> }
            }
            Self::ConstraintSequents => {
                syn::parse_quote! { async fn constraint_sequents_with<'db, E: SequentEffects<'db>>(constraint: Constraint<'db>, map: &mut SequentMap<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::ConstraintPairSequents => {
                syn::parse_quote! { async fn constraint_pair_sequents_with<'db, E: SequentEffects<'db>>(left: Constraint<'db>, map: &mut SequentMap<'db>, right: Constraint<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::LowerSequents => {
                syn::parse_quote! { async fn lower_sequents_with<'db, E: SequentEffects<'db>>(this: ConcreteLowerBound<'db>, map: &mut SequentMap<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::UpperSequents => {
                syn::parse_quote! { async fn upper_sequents_with<'db, E: SequentEffects<'db>>(this: ConcreteUpperBound<'db>, map: &mut SequentMap<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::EquivalenceSequents => {
                syn::parse_quote! { async fn equivalence_sequents_with<'db, E: SequentEffects<'db>>(this: ConcreteEquivalenceBound<'db>, map: &mut SequentMap<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::RangeSequents => {
                syn::parse_quote! { async fn range_sequents_with<'db, E: SequentEffects<'db>>(this: TypeVarRangeBound<'db>, map: &mut SequentMap<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::TypevarEquivalenceSequents => {
                syn::parse_quote! { async fn typevar_equivalence_sequents_with<'db, E: SequentEffects<'db>>(this: TypeVarEquivalenceBound<'db>, map: &mut SequentMap<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::LowerPairLower => {
                syn::parse_quote! { async fn lower_pair_lower_with<'db, E: SequentEffects<'db>>(this: ConcreteLowerBound<'db>, map: &mut SequentMap<'db>, other: ConcreteLowerBound<'db>, reversed: bool, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::LowerPairUpper => {
                syn::parse_quote! { async fn lower_pair_upper_with<'db, E: SequentEffects<'db>>(this: ConcreteLowerBound<'db>, map: &mut SequentMap<'db>, other: ConcreteUpperBound<'db>, _reversed: bool, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::LowerPairEquivalence => {
                syn::parse_quote! { async fn lower_pair_equivalence_with<'db, E: SequentEffects<'db>>(this: ConcreteLowerBound<'db>, map: &mut SequentMap<'db>, other: ConcreteEquivalenceBound<'db>, _reversed: bool, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::LowerPairRange => {
                syn::parse_quote! { async fn lower_pair_range_with<'db, E: SequentEffects<'db>>(this: ConcreteLowerBound<'db>, map: &mut SequentMap<'db>, other: TypeVarRangeBound<'db>, _reversed: bool, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::LowerPairTypevarEquivalence => {
                syn::parse_quote! { async fn lower_pair_typevar_equivalence_with<'db, E: SequentEffects<'db>>(this: ConcreteLowerBound<'db>, map: &mut SequentMap<'db>, other: TypeVarEquivalenceBound<'db>, _reversed: bool, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::UpperPairUpper => {
                syn::parse_quote! { async fn upper_pair_upper_with<'db, E: SequentEffects<'db>>(this: ConcreteUpperBound<'db>, map: &mut SequentMap<'db>, other: ConcreteUpperBound<'db>, reversed: bool, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::UpperPairEquivalence => {
                syn::parse_quote! { async fn upper_pair_equivalence_with<'db, E: SequentEffects<'db>>(this: ConcreteUpperBound<'db>, map: &mut SequentMap<'db>, other: ConcreteEquivalenceBound<'db>, _reversed: bool, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::UpperPairRange => {
                syn::parse_quote! { async fn upper_pair_range_with<'db, E: SequentEffects<'db>>(this: ConcreteUpperBound<'db>, map: &mut SequentMap<'db>, other: TypeVarRangeBound<'db>, _reversed: bool, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::UpperPairTypevarEquivalence => {
                syn::parse_quote! { async fn upper_pair_typevar_equivalence_with<'db, E: SequentEffects<'db>>(this: ConcreteUpperBound<'db>, map: &mut SequentMap<'db>, other: TypeVarEquivalenceBound<'db>, _reversed: bool, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::EquivalencePairEquivalence => {
                syn::parse_quote! { async fn equivalence_pair_equivalence_with<'db, E: SequentEffects<'db>>(this: ConcreteEquivalenceBound<'db>, map: &mut SequentMap<'db>, other: ConcreteEquivalenceBound<'db>, _reversed: bool, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::EquivalencePairRange => {
                syn::parse_quote! { async fn equivalence_pair_range_with<'db, E: SequentEffects<'db>>(this: ConcreteEquivalenceBound<'db>, map: &mut SequentMap<'db>, other: TypeVarRangeBound<'db>, _reversed: bool, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::EquivalencePairTypevarEquivalence => {
                syn::parse_quote! { async fn equivalence_pair_typevar_equivalence_with<'db, E: SequentEffects<'db>>(this: ConcreteEquivalenceBound<'db>, map: &mut SequentMap<'db>, other: TypeVarEquivalenceBound<'db>, _reversed: bool, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::RangePairRange => {
                syn::parse_quote! { async fn range_pair_range_with<'db, E: SequentEffects<'db>>(this: TypeVarRangeBound<'db>, map: &mut SequentMap<'db>, other: TypeVarRangeBound<'db>, _reversed: bool, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::RangePairTypevarEquivalence => {
                syn::parse_quote! { async fn range_pair_typevar_equivalence_with<'db, E: SequentEffects<'db>>(this: TypeVarRangeBound<'db>, map: &mut SequentMap<'db>, other: TypeVarEquivalenceBound<'db>, _reversed: bool, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::TypevarEquivalencePairTypevarEquivalence => {
                syn::parse_quote! { async fn typevar_equivalence_pair_typevar_equivalence_with<'db, E: SequentEffects<'db>>(this: TypeVarEquivalenceBound<'db>, map: &mut SequentMap<'db>, other: TypeVarEquivalenceBound<'db>, _reversed: bool, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::AddSequentsForRange => {
                syn::parse_quote! { async fn add_sequents_for_range_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, lower: impl ProvidesConcreteLowerBound<'db>, upper: impl ProvidesConcreteUpperBound<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::AddSequentsForEquivalence => {
                syn::parse_quote! { async fn add_sequents_for_equivalence_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, lower: impl ProvidesConcreteLowerBound<'db>, upper: impl ProvidesConcreteUpperBound<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::AddConstraintSetImplication => {
                syn::parse_quote! { async fn add_constraint_set_implication_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, lower_constraint: Constraint<'db>, upper_constraint: Constraint<'db>, when: &OwnedConstraintSet<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::SubstituteIfNotRecursive => {
                syn::parse_quote! { async fn substitute_if_not_recursive_with<'db, E: SequentEffects<'db>>(needle_typevar: BoundTypeVarInstance<'db>, needle_bound: Type<'db>, replacement_typevar: BoundTypeVarInstance<'db>, replacement_bound: Type<'db>, replacement_is: ReplacementIs, effects: &mut E) -> Result<Option<Type<'db>>, E::Error> }
            }
            Self::AddCovariantLowerTightenedSequent => {
                syn::parse_quote! { async fn add_covariant_lower_tightened_sequent_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, left: impl ProvidesConcreteLowerBound<'db>, right: impl ProvidesConcreteLowerBound<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::AddCovariantUpperTightenedSequent => {
                syn::parse_quote! { async fn add_covariant_upper_tightened_sequent_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, left: impl ProvidesConcreteUpperBound<'db>, right: impl ProvidesConcreteUpperBound<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::AddCovariantEquivalenceTightenedSequent => {
                syn::parse_quote! { async fn add_covariant_equivalence_tightened_sequent_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, left: ConcreteEquivalenceBound<'db>, right: ConcreteEquivalenceBound<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::AddContravariantTightenedSequent => {
                syn::parse_quote! { async fn add_contravariant_tightened_sequent_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, lower: impl ProvidesConcreteLowerBound<'db>, upper: impl ProvidesConcreteUpperBound<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::AddInvariantTightenedSequent => {
                syn::parse_quote! { async fn add_invariant_tightened_sequent_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, left: impl ProvidesConcreteBound<'db>, right: ConcreteEquivalenceBound<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::AddCovariantLowerWeakenedSequent => {
                syn::parse_quote! { async fn add_covariant_lower_weakened_sequent_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, left: impl ProvidesConcreteLowerBound<'db>, right: impl ProvidesTypeVarRangeBound<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::AddCovariantUpperWeakenedSequent => {
                syn::parse_quote! { async fn add_covariant_upper_weakened_sequent_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, left: impl ProvidesConcreteUpperBound<'db>, right: impl ProvidesTypeVarRangeBound<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::AddCovariantEquivalenceWeakenedSequent => {
                syn::parse_quote! { async fn add_covariant_equivalence_weakened_sequent_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, left: ConcreteEquivalenceBound<'db>, right: impl ProvidesTypeVarEquivalenceBound<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::AddContravariantLowerWeakenedSequent => {
                syn::parse_quote! { async fn add_contravariant_lower_weakened_sequent_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, left: impl ProvidesConcreteLowerBound<'db>, right: impl ProvidesTypeVarRangeBound<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::AddContravariantUpperWeakenedSequent => {
                syn::parse_quote! { async fn add_contravariant_upper_weakened_sequent_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, left: impl ProvidesConcreteUpperBound<'db>, right: impl ProvidesTypeVarRangeBound<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::AddContravariantEquivalenceWeakenedSequent => {
                syn::parse_quote! { async fn add_contravariant_equivalence_weakened_sequent_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, left: ConcreteEquivalenceBound<'db>, right: impl ProvidesTypeVarEquivalenceBound<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::AddInvariantWeakenedSequent => {
                syn::parse_quote! { async fn add_invariant_weakened_sequent_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, left: impl ProvidesConcreteBound<'db>, right: impl ProvidesTypeVarEquivalenceBound<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::PossiblyReversedIntersection => {
                syn::parse_quote! { async fn possibly_reversed_intersection_with<'db, E: SequentEffects<'db>>(reversed: bool, left: Type<'db>, right: Type<'db>, effects: &mut E) -> Result<Type<'db>, E::Error> }
            }
            Self::PossiblyReversedUnion => {
                syn::parse_quote! { async fn possibly_reversed_union_with<'db, E: SequentEffects<'db>>(reversed: bool, left: Type<'db>, right: Type<'db>, effects: &mut E) -> Result<Type<'db>, E::Error> }
            }
            Self::DeriveGroupDirection => {
                syn::parse_quote! { async fn derive_group_direction_with<'db, E: SequentEffects<'db>>(source: GroupedSequentSource<'db>, direction: TypeVarEquivalenceDirectedView<'db>, map: &mut SequentMap<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::DeriveGroup => {
                syn::parse_quote! { async fn derive_group_with<'db, E: SequentEffects<'db>>(source: GroupedSequentSource<'db>, equivalence: TypeVarEquivalenceBound<'db>, map: &mut SequentMap<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::ExtractPending => {
                syn::parse_quote! { async fn extract_pending_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, effects: &mut E) -> Result<Box<[Sequent<Constraint<'db>>]>, E::Error> }
            }
            Self::FlushPending => {
                syn::parse_quote! { async fn flush_pending_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
            Self::FinishSequents => {
                syn::parse_quote! { async fn finish_sequents_with<'db, E: SequentEffects<'db>>(map: &mut SequentMap<'db>, effects: &mut E) -> Result<(), E::Error> }
            }
        }
    }
    fn effect_methods(self) -> &'static [&'static str] {
        match self {
            Self::SingleSequents => &["checkpoint"],
            Self::PairSequents => &["checkpoint"],
            Self::PairCannotProduce => &["checkpoint", "trivially_disjoint"],
            Self::ConstraintSequents => &["checkpoint"],
            Self::ConstraintPairSequents => &["checkpoint"],
            Self::LowerSequents => &["checkpoint", "domain_endpoint", "emit"],
            Self::UpperSequents => &["checkpoint", "domain_endpoint", "emit"],
            Self::EquivalenceSequents => &["checkpoint"],
            Self::RangeSequents => &["checkpoint", "emit"],
            Self::TypevarEquivalenceSequents => &["checkpoint", "emit"],
            Self::LowerPairLower => &["assignable", "checkpoint", "emit"],
            Self::LowerPairUpper => &[
                "checkpoint",
                "domain_endpoint",
                "emit",
                "equivalent",
                "materialize",
                "static_eligible",
            ],
            Self::LowerPairEquivalence => &[
                "assignable",
                "checkpoint",
                "emit",
                "equivalent",
                "static_eligible",
            ],
            Self::LowerPairRange => &["checkpoint", "emit"],
            Self::LowerPairTypevarEquivalence => &["checkpoint", "emit"],
            Self::UpperPairUpper => &["assignable", "checkpoint", "emit"],
            Self::UpperPairEquivalence => &[
                "assignable",
                "checkpoint",
                "emit",
                "equivalent",
                "static_eligible",
            ],
            Self::UpperPairRange => &["checkpoint", "emit"],
            Self::UpperPairTypevarEquivalence => &["checkpoint", "emit"],
            Self::EquivalencePairEquivalence => &["checkpoint", "emit", "equivalent"],
            Self::EquivalencePairRange => &["checkpoint", "emit"],
            Self::EquivalencePairTypevarEquivalence => &["checkpoint", "emit"],
            Self::RangePairRange => &["checkpoint", "emit"],
            Self::RangePairTypevarEquivalence => &["checkpoint", "emit"],
            Self::TypevarEquivalencePairTypevarEquivalence => &["checkpoint", "emit"],
            Self::AddSequentsForRange => &["checkpoint", "domain_endpoint", "owned_assignable"],
            Self::AddSequentsForEquivalence => {
                &["checkpoint", "owned_equivalent", "static_eligible"]
            }
            Self::AddConstraintSetImplication => &["checkpoint", "conjunction_step", "emit"],
            Self::SubstituteIfNotRecursive => {
                &["checkpoint", "static_eligible", "substitute", "variance"]
            }
            Self::AddCovariantLowerTightenedSequent => &["checkpoint", "emit", "variance"],
            Self::AddCovariantUpperTightenedSequent => &["checkpoint", "emit", "variance"],
            Self::AddCovariantEquivalenceTightenedSequent => &["checkpoint", "emit", "variance"],
            Self::AddContravariantTightenedSequent => &["checkpoint", "emit", "variance"],
            Self::AddInvariantTightenedSequent => &["checkpoint", "emit", "variance"],
            Self::AddCovariantLowerWeakenedSequent => &["checkpoint", "emit", "variance"],
            Self::AddCovariantUpperWeakenedSequent => &["checkpoint", "emit", "variance"],
            Self::AddCovariantEquivalenceWeakenedSequent => &["checkpoint", "emit", "variance"],
            Self::AddContravariantLowerWeakenedSequent => &["checkpoint", "emit", "variance"],
            Self::AddContravariantUpperWeakenedSequent => &["checkpoint", "emit", "variance"],
            Self::AddContravariantEquivalenceWeakenedSequent => &["checkpoint", "emit", "variance"],
            Self::AddInvariantWeakenedSequent => &["checkpoint", "emit", "variance"],
            Self::PossiblyReversedIntersection => &["checkpoint", "intersection"],
            Self::PossiblyReversedUnion => &["checkpoint", "union"],
            Self::DeriveGroupDirection => &["checkpoint"],
            Self::DeriveGroup => &["checkpoint", "reserve_group"],
            Self::ExtractPending => &["checkpoint", "prepare_extract"],
            Self::FlushPending => &["checkpoint", "reserve_group"],
            Self::FinishSequents => &["checkpoint", "prepare_shrink"],
        }
    }
    fn free_helper(self, name: &Ident) -> Option<Self> {
        match self {
            Self::SingleSequents if name == "constraint_sequents_with" => {
                Some(Self::ConstraintSequents)
            }
            Self::SingleSequents if name == "finish_sequents_with" => Some(Self::FinishSequents),
            Self::PairSequents if name == "constraint_pair_sequents_with" => {
                Some(Self::ConstraintPairSequents)
            }
            Self::PairSequents if name == "finish_sequents_with" => Some(Self::FinishSequents),
            Self::ConstraintSequents if name == "equivalence_sequents_with" => {
                Some(Self::EquivalenceSequents)
            }
            Self::ConstraintSequents if name == "lower_sequents_with" => Some(Self::LowerSequents),
            Self::ConstraintSequents if name == "range_sequents_with" => Some(Self::RangeSequents),
            Self::ConstraintSequents if name == "typevar_equivalence_sequents_with" => {
                Some(Self::TypevarEquivalenceSequents)
            }
            Self::ConstraintSequents if name == "upper_sequents_with" => Some(Self::UpperSequents),
            Self::ConstraintPairSequents if name == "equivalence_pair_equivalence_with" => {
                Some(Self::EquivalencePairEquivalence)
            }
            Self::ConstraintPairSequents if name == "equivalence_pair_range_with" => {
                Some(Self::EquivalencePairRange)
            }
            Self::ConstraintPairSequents if name == "equivalence_pair_typevar_equivalence_with" => {
                Some(Self::EquivalencePairTypevarEquivalence)
            }
            Self::ConstraintPairSequents if name == "lower_pair_equivalence_with" => {
                Some(Self::LowerPairEquivalence)
            }
            Self::ConstraintPairSequents if name == "lower_pair_lower_with" => {
                Some(Self::LowerPairLower)
            }
            Self::ConstraintPairSequents if name == "lower_pair_range_with" => {
                Some(Self::LowerPairRange)
            }
            Self::ConstraintPairSequents if name == "lower_pair_typevar_equivalence_with" => {
                Some(Self::LowerPairTypevarEquivalence)
            }
            Self::ConstraintPairSequents if name == "lower_pair_upper_with" => {
                Some(Self::LowerPairUpper)
            }
            Self::ConstraintPairSequents if name == "range_pair_range_with" => {
                Some(Self::RangePairRange)
            }
            Self::ConstraintPairSequents if name == "range_pair_typevar_equivalence_with" => {
                Some(Self::RangePairTypevarEquivalence)
            }
            Self::ConstraintPairSequents
                if name == "typevar_equivalence_pair_typevar_equivalence_with" =>
            {
                Some(Self::TypevarEquivalencePairTypevarEquivalence)
            }
            Self::ConstraintPairSequents if name == "upper_pair_equivalence_with" => {
                Some(Self::UpperPairEquivalence)
            }
            Self::ConstraintPairSequents if name == "upper_pair_range_with" => {
                Some(Self::UpperPairRange)
            }
            Self::ConstraintPairSequents if name == "upper_pair_typevar_equivalence_with" => {
                Some(Self::UpperPairTypevarEquivalence)
            }
            Self::ConstraintPairSequents if name == "upper_pair_upper_with" => {
                Some(Self::UpperPairUpper)
            }
            Self::LowerPairLower if name == "add_covariant_lower_tightened_sequent_with" => {
                Some(Self::AddCovariantLowerTightenedSequent)
            }
            Self::LowerPairLower if name == "possibly_reversed_union_with" => {
                Some(Self::PossiblyReversedUnion)
            }
            Self::LowerPairUpper if name == "add_contravariant_tightened_sequent_with" => {
                Some(Self::AddContravariantTightenedSequent)
            }
            Self::LowerPairUpper if name == "add_sequents_for_range_with" => {
                Some(Self::AddSequentsForRange)
            }
            Self::LowerPairEquivalence if name == "add_contravariant_tightened_sequent_with" => {
                Some(Self::AddContravariantTightenedSequent)
            }
            Self::LowerPairEquivalence if name == "add_covariant_lower_tightened_sequent_with" => {
                Some(Self::AddCovariantLowerTightenedSequent)
            }
            Self::LowerPairEquivalence if name == "add_invariant_tightened_sequent_with" => {
                Some(Self::AddInvariantTightenedSequent)
            }
            Self::LowerPairEquivalence if name == "add_sequents_for_range_with" => {
                Some(Self::AddSequentsForRange)
            }
            Self::LowerPairRange if name == "add_contravariant_lower_weakened_sequent_with" => {
                Some(Self::AddContravariantLowerWeakenedSequent)
            }
            Self::LowerPairRange if name == "add_covariant_lower_weakened_sequent_with" => {
                Some(Self::AddCovariantLowerWeakenedSequent)
            }
            Self::LowerPairTypevarEquivalence if name == "derive_group_with" => {
                Some(Self::DeriveGroup)
            }
            Self::UpperPairUpper if name == "add_covariant_upper_tightened_sequent_with" => {
                Some(Self::AddCovariantUpperTightenedSequent)
            }
            Self::UpperPairUpper if name == "possibly_reversed_intersection_with" => {
                Some(Self::PossiblyReversedIntersection)
            }
            Self::UpperPairEquivalence if name == "add_contravariant_tightened_sequent_with" => {
                Some(Self::AddContravariantTightenedSequent)
            }
            Self::UpperPairEquivalence if name == "add_covariant_upper_tightened_sequent_with" => {
                Some(Self::AddCovariantUpperTightenedSequent)
            }
            Self::UpperPairEquivalence if name == "add_invariant_tightened_sequent_with" => {
                Some(Self::AddInvariantTightenedSequent)
            }
            Self::UpperPairEquivalence if name == "add_sequents_for_range_with" => {
                Some(Self::AddSequentsForRange)
            }
            Self::UpperPairRange if name == "add_contravariant_upper_weakened_sequent_with" => {
                Some(Self::AddContravariantUpperWeakenedSequent)
            }
            Self::UpperPairRange if name == "add_covariant_upper_weakened_sequent_with" => {
                Some(Self::AddCovariantUpperWeakenedSequent)
            }
            Self::UpperPairTypevarEquivalence if name == "derive_group_with" => {
                Some(Self::DeriveGroup)
            }
            Self::EquivalencePairEquivalence
                if name == "add_contravariant_tightened_sequent_with" =>
            {
                Some(Self::AddContravariantTightenedSequent)
            }
            Self::EquivalencePairEquivalence
                if name == "add_covariant_equivalence_tightened_sequent_with" =>
            {
                Some(Self::AddCovariantEquivalenceTightenedSequent)
            }
            Self::EquivalencePairEquivalence if name == "add_invariant_tightened_sequent_with" => {
                Some(Self::AddInvariantTightenedSequent)
            }
            Self::EquivalencePairEquivalence if name == "add_sequents_for_equivalence_with" => {
                Some(Self::AddSequentsForEquivalence)
            }
            Self::EquivalencePairRange
                if name == "add_contravariant_lower_weakened_sequent_with" =>
            {
                Some(Self::AddContravariantLowerWeakenedSequent)
            }
            Self::EquivalencePairRange
                if name == "add_contravariant_upper_weakened_sequent_with" =>
            {
                Some(Self::AddContravariantUpperWeakenedSequent)
            }
            Self::EquivalencePairRange if name == "add_covariant_lower_weakened_sequent_with" => {
                Some(Self::AddCovariantLowerWeakenedSequent)
            }
            Self::EquivalencePairRange if name == "add_covariant_upper_weakened_sequent_with" => {
                Some(Self::AddCovariantUpperWeakenedSequent)
            }
            Self::EquivalencePairTypevarEquivalence if name == "derive_group_with" => {
                Some(Self::DeriveGroup)
            }
            Self::AddSequentsForRange if name == "add_constraint_set_implication_with" => {
                Some(Self::AddConstraintSetImplication)
            }
            Self::AddSequentsForEquivalence if name == "add_constraint_set_implication_with" => {
                Some(Self::AddConstraintSetImplication)
            }
            Self::AddCovariantLowerTightenedSequent
                if name == "substitute_if_not_recursive_with" =>
            {
                Some(Self::SubstituteIfNotRecursive)
            }
            Self::AddCovariantUpperTightenedSequent
                if name == "substitute_if_not_recursive_with" =>
            {
                Some(Self::SubstituteIfNotRecursive)
            }
            Self::AddCovariantEquivalenceTightenedSequent
                if name == "substitute_if_not_recursive_with" =>
            {
                Some(Self::SubstituteIfNotRecursive)
            }
            Self::AddContravariantTightenedSequent
                if name == "substitute_if_not_recursive_with" =>
            {
                Some(Self::SubstituteIfNotRecursive)
            }
            Self::AddInvariantTightenedSequent if name == "substitute_if_not_recursive_with" => {
                Some(Self::SubstituteIfNotRecursive)
            }
            Self::AddCovariantLowerWeakenedSequent
                if name == "substitute_if_not_recursive_with" =>
            {
                Some(Self::SubstituteIfNotRecursive)
            }
            Self::AddCovariantUpperWeakenedSequent
                if name == "substitute_if_not_recursive_with" =>
            {
                Some(Self::SubstituteIfNotRecursive)
            }
            Self::AddCovariantEquivalenceWeakenedSequent
                if name == "substitute_if_not_recursive_with" =>
            {
                Some(Self::SubstituteIfNotRecursive)
            }
            Self::AddContravariantLowerWeakenedSequent
                if name == "substitute_if_not_recursive_with" =>
            {
                Some(Self::SubstituteIfNotRecursive)
            }
            Self::AddContravariantUpperWeakenedSequent
                if name == "substitute_if_not_recursive_with" =>
            {
                Some(Self::SubstituteIfNotRecursive)
            }
            Self::AddContravariantEquivalenceWeakenedSequent
                if name == "substitute_if_not_recursive_with" =>
            {
                Some(Self::SubstituteIfNotRecursive)
            }
            Self::AddInvariantWeakenedSequent if name == "substitute_if_not_recursive_with" => {
                Some(Self::SubstituteIfNotRecursive)
            }
            Self::DeriveGroupDirection
                if name == "add_contravariant_equivalence_weakened_sequent_with" =>
            {
                Some(Self::AddContravariantEquivalenceWeakenedSequent)
            }
            Self::DeriveGroupDirection
                if name == "add_contravariant_lower_weakened_sequent_with" =>
            {
                Some(Self::AddContravariantLowerWeakenedSequent)
            }
            Self::DeriveGroupDirection
                if name == "add_contravariant_upper_weakened_sequent_with" =>
            {
                Some(Self::AddContravariantUpperWeakenedSequent)
            }
            Self::DeriveGroupDirection
                if name == "add_covariant_equivalence_weakened_sequent_with" =>
            {
                Some(Self::AddCovariantEquivalenceWeakenedSequent)
            }
            Self::DeriveGroupDirection if name == "add_covariant_lower_weakened_sequent_with" => {
                Some(Self::AddCovariantLowerWeakenedSequent)
            }
            Self::DeriveGroupDirection if name == "add_covariant_upper_weakened_sequent_with" => {
                Some(Self::AddCovariantUpperWeakenedSequent)
            }
            Self::DeriveGroupDirection if name == "add_invariant_weakened_sequent_with" => {
                Some(Self::AddInvariantWeakenedSequent)
            }
            Self::DeriveGroup if name == "derive_group_direction_with" => {
                Some(Self::DeriveGroupDirection)
            }
            Self::DeriveGroup if name == "extract_pending_with" => Some(Self::ExtractPending),
            Self::DeriveGroup if name == "flush_pending_with" => Some(Self::FlushPending),
            Self::FlushPending if name == "extract_pending_with" => Some(Self::ExtractPending),
            Self::FinishSequents if name == "flush_pending_with" => Some(Self::FlushPending),
            _ => None,
        }
    }
}

struct MemberLowerer {
    manifest: MemberManifest,
    scope: Scope,
    in_matches: bool,
    error: Option<Error>,
}

impl MemberLowerer {
    fn reject(&mut self, span: Span, message: &str) {
        if self.error.is_none() {
            self.error = Some(Error::new(span, message));
        }
    }

    fn lower_await(&mut self, awaited: &syn::ExprAwait) -> Result<Expr> {
        if !matches!(self.scope, Scope::Body) || self.in_matches {
            return Err(Error::new_spanned(
                awaited,
                format!(
                    "await is only supported directly in the {} body",
                    self.manifest.body_name()
                ),
            ));
        }
        let mut call = match *awaited.base.clone() {
            Expr::Call(mut call)
                if matches!(
                    self.manifest,
                    MemberManifest::Lookup(_)
                        | MemberManifest::Sequent(_)
                        | MemberManifest::ConstraintCachedDepth
                        | MemberManifest::SatisfactionRoot
                        | MemberManifest::PathVisitOwned
                        | MemberManifest::PathVisitBody
                        | MemberManifest::PathEnterEdge
                        | MemberManifest::PathDrainAssignments
                        | MemberManifest::PathAddAssignment
                        | MemberManifest::PathDiscoverConstraint
                        | MemberManifest::PathImportSequents
                        | MemberManifest::PathImportSlice
                        | MemberManifest::PathCheckSequent
                        | MemberManifest::BaseCursorNext
                        | MemberManifest::CollectStart
                        | MemberManifest::CollectStartWithRoot
                        | MemberManifest::CollectClassLiterals
                        | MemberManifest::MroFirst
                        | MemberManifest::MroNext
                        | MemberManifest::StaticMro
                        | MemberManifest::BaseMroStart
                        | MemberManifest::CollectBaseMro
                        | MemberManifest::CollectSingleBaseMro
                        | MemberManifest::ProtocolInterfaceBuild
                        | MemberManifest::ProtocolRelationDirect
                        | MemberManifest::ProtocolRelationMeta
                        | MemberManifest::ProtocolRelationRun
                        | MemberManifest::ProtocolRelationStart
                        | MemberManifest::ProtocolRelationPairResume
                        | MemberManifest::ProtocolRelationAfterNominal
                        | MemberManifest::ProtocolRelationStructural
                        | MemberManifest::ProtocolRelationInterfaceResume
                        | MemberManifest::ProtocolRelationStructuralNext
                        | MemberManifest::ProtocolRelationNominalFiniteNext
                        | MemberManifest::ProtocolRelationNominalRecursiveNext
                        | MemberManifest::ProtocolRelationMemberResume
                ) =>
            {
                call.attrs.splice(0..0, awaited.attrs.iter().cloned());
                self.lower_free_helper(&mut call)?;
                return Ok(Expr::Call(call));
            }
            Expr::MethodCall(call) => call,
            _ => {
                return Err(Error::new_spanned(
                    awaited,
                    format!(
                        "await requires a declared {} effect method on effects",
                        self.manifest.body_name()
                    ),
                ));
            }
        };
        if let Some(helper) = self.manifest.helper(&call.method) {
            call.attrs.splice(0..0, awaited.attrs.iter().cloned());
            self.visit_helper_arguments(&mut call, helper)?;
            call.method = Ident::new("promote_singletons_impl_sync", call.method.span());
            return Ok(Expr::MethodCall(call));
        }
        if !is_identifier(&call.receiver, "effects") {
            return Err(Error::new_spanned(
                call,
                format!(
                    "await requires a declared {} effect method on effects",
                    self.manifest.body_name()
                ),
            ));
        }
        if self.manifest.is_fact_method(&call.method) {
            return Err(Error::new_spanned(
                call,
                format!(
                    "{} fact methods must be called without await",
                    self.manifest.body_name()
                ),
            ));
        }
        if !self.manifest.is_effect_method(&call.method) {
            return Err(Error::new_spanned(
                call,
                format!(
                    "await requires a declared {} effect method on effects",
                    self.manifest.body_name()
                ),
            ));
        }
        call.attrs.splice(0..0, awaited.attrs.iter().cloned());
        self.visit_approved_call_arguments(&mut call);
        Ok(Expr::MethodCall(call))
    }

    fn lower_free_helper(&mut self, call: &mut syn::ExprCall) -> Result<()> {
        let Expr::Path(path) = call.func.as_mut() else {
            return Err(Error::new_spanned(
                &call.func,
                "awaited free calls require a declared unqualified helper",
            ));
        };
        if path.qself.is_some()
            || path.path.leading_colon.is_some()
            || path.path.segments.len() != 1
        {
            return Err(Error::new_spanned(
                path,
                "awaited free calls require a declared unqualified helper",
            ));
        }
        let Some(segment) = path.path.segments.first_mut() else {
            return Err(Error::new_spanned(
                path,
                "awaited free calls require a declared unqualified helper",
            ));
        };
        let Some(helper) = self.manifest.free_helper(&segment.ident) else {
            return Err(Error::new_spanned(
                &segment.ident,
                "awaited free calls require a declared unqualified helper",
            ));
        };
        let count = helper.argument_count();
        if call.args.len() != count
            || call
                .args
                .last()
                .is_none_or(|last| !is_identifier(last, "effects"))
        {
            return Err(Error::new_spanned(
                &call.args,
                "free helpers require their declared arguments ending with bare effects",
            ));
        }
        for attribute in &mut call.attrs {
            self.visit_attribute_mut(attribute);
        }
        for attribute in &mut path.attrs {
            self.visit_attribute_mut(attribute);
        }
        self.visit_path_arguments_mut(&mut segment.arguments);
        if self.manifest.uses_mro_fields() && !matches!(helper, FreeMemberHelper::MaybeAddGeneric) {
            let Some(first) = call.args.first_mut() else {
                return Err(Error::new_spanned(
                    &call.args,
                    "MRO helpers require bare fields first",
                ));
            };
            if !is_identifier(first, "fields") {
                return Err(Error::new_spanned(
                    first,
                    "MRO helpers require bare fields first",
                ));
            }
            *first = syn::parse_quote!(db);
        }
        for argument in call.args.iter_mut().take(count - 1) {
            self.visit_expr_mut(argument);
        }
        segment.ident = Ident::new(helper.synchronous_name(), segment.ident.span());
        Ok(())
    }

    fn visit_approved_call_arguments(&mut self, call: &mut syn::ExprMethodCall) {
        // The receiver is the exact validated parameter. Other uses of that parameter are
        // forbidden, so an alias cannot hide a deferred fact read or effect future.
        for attribute in &mut call.attrs {
            self.visit_attribute_mut(attribute);
        }
        if let Some(arguments) = &mut call.turbofish {
            self.visit_angle_bracketed_generic_arguments_mut(arguments);
        }
        for argument in &mut call.args {
            self.visit_expr_mut(argument);
        }
    }

    fn visit_helper_arguments(
        &mut self,
        call: &mut syn::ExprMethodCall,
        helper: MemberHelper,
    ) -> Result<()> {
        if !matches!(self.scope, Scope::Body) {
            return Err(Error::new_spanned(
                call,
                "promotion helpers are only supported directly in the body",
            ));
        }
        let count = match helper {
            MemberHelper::SingletonPromotion => 3,
        };
        if call.args.len() != count
            || call
                .args
                .last()
                .is_none_or(|last| !is_identifier(last, "effects"))
        {
            return Err(Error::new_spanned(
                call,
                "promotion helpers require their declared arguments ending with effects",
            ));
        }
        self.visit_expr_mut(&mut call.receiver);
        for attribute in &mut call.attrs {
            self.visit_attribute_mut(attribute);
        }
        if let Some(arguments) = &mut call.turbofish {
            self.visit_angle_bracketed_generic_arguments_mut(arguments);
        }
        for argument in call.args.iter_mut().take(count - 1) {
            self.visit_expr_mut(argument);
        }
        Ok(())
    }
}

impl VisitMut for MemberLowerer {
    fn visit_pat_ident_mut(&mut self, pattern: &mut syn::PatIdent) {
        // Syn also represents a bare unit variant as PatIdent. The unmodified None pattern
        // matches Option's variant; ref, mut, and @ forms still use forbidden binding syntax.
        let is_none_variant = pattern.ident.to_string() == "None"
            && pattern.attrs.is_empty()
            && pattern.by_ref.is_none()
            && pattern.mutability.is_none()
            && pattern.subpat.is_none();
        if !matches!(self.scope, Scope::Signature)
            && let MemberManifest::Lookup(entry) = self.manifest
            && entry.protects_name(&pattern.ident)
            && !is_none_variant
        {
            self.reject(
                pattern.ident.span(),
                "body bindings cannot shadow declared storage capabilities or helpers",
            );
        }

        if !matches!(self.scope, Scope::Signature)
            && (pattern.ident == "effects" || pattern.ident == "r#effects")
        {
            self.reject(pattern.ident.span(), "body bindings cannot shadow effects");
        }
        if !matches!(self.scope, Scope::Signature) && is_declared_free_helper(&pattern.ident) {
            self.reject(
                pattern.ident.span(),
                "body bindings cannot shadow a declared free helper",
            );
        }
        visit_mut::visit_pat_ident_mut(self, pattern);
    }

    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        if let Expr::Await(awaited) = expression {
            match self.lower_await(awaited) {
                Ok(lowered) => *expression = lowered,
                Err(error) if self.error.is_none() => self.error = Some(error),
                Err(_) => {}
            }
        } else if let Expr::Verbatim(tokens) = expression {
            self.reject(
                tokens.span(),
                &format!(
                    "unsupported expression syntax in {}",
                    self.manifest.attribute().name()
                ),
            );
        } else {
            visit_mut::visit_expr_mut(self, expression);
        }
    }

    fn visit_expr_method_call_mut(&mut self, call: &mut syn::ExprMethodCall) {
        let moved_field = match self.manifest {
            MemberManifest::ClassTypeOwnMember => matches!(
                call.method.to_string().as_str(),
                "origin"
                    | "specialization"
                    | "is_tuple"
                    | "tuple"
                    | "alias_origin"
                    | "alias_specialization"
                    | "specialization_tuple"
            ),
            MemberManifest::SynthesizedMember => call.method == "total_ordering",
            MemberManifest::CollectClassLiterals => call.method == "class_literal",
            _ => false,
        };
        if moved_field && !is_identifier(&call.receiver, "effects") {
            self.reject(
                call.span(),
                "member field reads require their declared effects",
            );
            return;
        }
        if matches!(self.manifest, MemberManifest::MroClass)
            && matches!(call.method.to_string().as_str(), "push" | "clear")
        {
            self.reject(
                call.span(),
                "MRO pending mutations require their declared effects",
            );
            return;
        }
        if is_declared_free_helper(&call.method) {
            self.reject(
                call.span(),
                "declared free helpers require a direct awaited free call",
            );
            return;
        }
        if self.manifest.helper(&call.method).is_some() {
            self.reject(
                call.span(),
                "promotion reduction helpers must be awaited directly",
            );
            return;
        }
        if is_identifier(&call.receiver, "effects") {
            if !matches!(self.scope, Scope::Body) {
                self.reject(
                    call.span(),
                    &format!(
                        "fact calls are only supported directly in the {} body",
                        self.manifest.body_name()
                    ),
                );
            } else if self.manifest.is_effect_method(&call.method) {
                self.reject(
                    call.span(),
                    &format!(
                        "{} effect methods must be awaited directly",
                        self.manifest.body_name()
                    ),
                );
            } else if !self.manifest.is_fact_method(&call.method) {
                self.reject(
                    call.span(),
                    &format!(
                        "unawaited calls on effects require a declared {} fact method",
                        self.manifest.body_name()
                    ),
                );
            } else {
                self.visit_approved_call_arguments(call);
            }
            return;
        }
        visit_mut::visit_expr_method_call_mut(self, call);
    }

    fn visit_expr_path_mut(&mut self, path: &mut syn::ExprPath) {
        if path
            .path
            .segments
            .iter()
            .any(|segment| is_declared_free_helper(&segment.ident))
        {
            self.reject(
                path.span(),
                "declared free helpers must be awaited directly and cannot escape",
            );
        }
        if path.qself.is_none()
            && (path.path.is_ident("effects") || path.path.is_ident("r#effects"))
        {
            self.reject(
                path.span(),
                "effects may only be the receiver of a declared fact method or awaited effect method",
            );
        }
        visit_mut::visit_expr_path_mut(self, path);
    }

    fn visit_expr_async_mut(&mut self, expression: &mut syn::ExprAsync) {
        self.reject(
            expression.span(),
            &format!(
                "async blocks are not supported by {}",
                self.manifest.attribute().name()
            ),
        );
    }

    fn visit_expr_closure_mut(&mut self, closure: &mut syn::ExprClosure) {
        if closure.asyncness.is_some() {
            self.reject(
                closure.span(),
                &format!(
                    "async closures are not supported by {}",
                    self.manifest.attribute().name()
                ),
            );
            return;
        }
        let scope = self.scope;
        self.scope = Scope::DeferredBody;
        visit_mut::visit_expr_closure_mut(self, closure);
        self.scope = scope;
    }

    fn visit_expr_const_mut(&mut self, expression: &mut syn::ExprConst) {
        let scope = self.scope;
        self.scope = Scope::DeferredBody;
        visit_mut::visit_expr_const_mut(self, expression);
        self.scope = scope;
    }

    fn visit_type_mut(&mut self, ty: &mut syn::Type) {
        let scope = self.scope;
        self.scope = Scope::DeferredBody;
        visit_mut::visit_type_mut(self, ty);
        self.scope = scope;
    }

    fn visit_generic_argument_mut(&mut self, argument: &mut syn::GenericArgument) {
        let scope = self.scope;
        self.scope = Scope::DeferredBody;
        visit_mut::visit_generic_argument_mut(self, argument);
        self.scope = scope;
    }

    fn visit_item_mut(&mut self, item: &mut syn::Item) {
        // Local items can introduce value bindings and macro definitions that this closed
        // function does not need. Rejecting them also excludes awaits in deferred bodies.
        self.reject(
            item.span(),
            &format!(
                "nested items are not supported by {}",
                self.manifest.attribute().name()
            ),
        );
    }

    fn visit_macro_mut(&mut self, invocation: &mut syn::Macro) {
        if self.in_matches {
            self.reject(invocation.span(), "matches! cannot contain nested macros");
        } else if matches!(self.manifest, MemberManifest::PathEnterEdge)
            && matches!(self.scope, Scope::Body)
            && invocation.path.is_ident("debug_assert")
        {
            match syn::parse2::<Expr>(invocation.tokens.clone()) {
                Ok(expression)
                    if expression == syn::parse_quote!(path.assignment_queue.is_empty()) => {}
                Ok(_) => self.reject(
                    invocation.span(),
                    "the path edge debug assertion requires path.assignment_queue.is_empty()",
                ),
                Err(error) if self.error.is_none() => self.error = Some(error),
                Err(_) => {}
            }
        } else if matches!(self.manifest, MemberManifest::ProtocolRelationStartMeta)
            && matches!(self.scope, Scope::Body)
            && invocation.path.is_ident("debug_assert")
        {
            match syn::parse2::<Expr>(invocation.tokens.clone()) {
                Ok(Expr::Macro(mut checked))
                    if checked.attrs.is_empty() && checked.mac.path.is_ident("matches") =>
                {
                    self.visit_macro_mut(&mut checked.mac);
                }
                Ok(_) => self.reject(
                    invocation.span(),
                    "the meta-entry debug assertion requires one checked matches! expression",
                ),
                Err(error) if self.error.is_none() => self.error = Some(error),
                Err(_) => {}
            }
        } else if !invocation.path.is_ident("matches") {
            self.reject(
                invocation.span(),
                &format!(
                    "only checked matches! expressions are supported by {}",
                    self.manifest.attribute().name()
                ),
            );
        } else {
            match syn::parse2::<MatchesInput>(invocation.tokens.clone()) {
                Ok(mut input) => {
                    self.in_matches = true;
                    self.visit_expr_mut(&mut input.expression);
                    self.visit_pat_mut(&mut input.pattern);
                    if let Some(guard) = &mut input.guard {
                        self.visit_expr_mut(guard);
                    }
                    self.in_matches = false;
                }
                Err(error) if self.error.is_none() => self.error = Some(error),
                Err(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests;

pub(super) fn expand_instance_storage(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::InstanceStorage)
}

pub(super) fn expand_slot_selector(
    arguments: TokenStream,
    original: &TokenStream,
) -> Result<TokenStream> {
    expand_member(arguments, original, MemberAttribute::SlotSelector)
}
