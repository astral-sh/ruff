use proc_macro2::Span;
use quote::ToTokens;
use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{Error, Expr, Path, Result, Signature};

use super::{MatchesInput, MemberAttribute, is_identifier};

#[derive(Clone, Copy)]
pub(super) enum LookupManifest {
    ClassStorage,
    ClassOwnStorage,
    StaticStorage,
    TypedDictClassification,
    OwnClassBinding,
    GeneratedSlots,
    NamedTupleSlots,
    SlotNames,
    InstanceSlot,
    InstanceDictionary,
    LacksInstanceStorage,
    OwnSlotDescriptor,
    SlotNameContains,
    SlotNamedTupleBase,
    Namespace,
    InstanceMro,
    StaticInstance,
    RuntimeBinding,
    ImplicitAttribute,
    StaticCodeGenerator,
}

impl LookupManifest {
    pub(super) fn storage_from_name(name: &syn::Ident, storage: bool) -> Option<Self> {
        match name.to_string().as_str() {
            "class_instance_member_with" if storage => Some(Self::ClassStorage),
            "class_own_instance_member_with" if storage => Some(Self::ClassOwnStorage),
            "static_instance_member_with" if storage => Some(Self::StaticStorage),
            "static_is_typed_dict_with" if storage => Some(Self::TypedDictClassification),
            "own_class_binding_with" if !storage => Some(Self::OwnClassBinding),
            "generated_slots_with" if !storage => Some(Self::GeneratedSlots),
            "named_tuple_slots_with" if !storage => Some(Self::NamedTupleSlots),
            "slot_names_with" if !storage => Some(Self::SlotNames),
            "instance_slot_with" if !storage => Some(Self::InstanceSlot),
            "instance_dictionary_with" if !storage => Some(Self::InstanceDictionary),
            "lacks_instance_storage_with" if !storage => Some(Self::LacksInstanceStorage),
            "own_slot_descriptor_with" if !storage => Some(Self::OwnSlotDescriptor),
            "slot_name_contains_with" if !storage => Some(Self::SlotNameContains),
            "slot_named_tuple_base_with" if !storage => Some(Self::SlotNamedTupleBase),
            _ => None,
        }
    }
    pub(super) fn is_storage_or_slot(self) -> bool {
        matches!(
            self.attribute(),
            MemberAttribute::InstanceStorage | MemberAttribute::SlotSelector
        )
    }
    pub(super) fn free_helper(self, name: &syn::Ident) -> Option<Self> {
        let helper = Self::storage_from_name(name, false)?;
        let approved = match self {
            Self::GeneratedSlots => matches!(helper, Self::NamedTupleSlots),
            Self::NamedTupleSlots => matches!(helper, Self::SlotNamedTupleBase),
            Self::SlotNames => matches!(helper, Self::OwnClassBinding | Self::GeneratedSlots),
            Self::InstanceSlot => matches!(helper, Self::SlotNameContains),
            Self::InstanceDictionary => {
                matches!(helper, Self::OwnClassBinding | Self::GeneratedSlots)
            }
            Self::LacksInstanceStorage => matches!(
                helper,
                Self::SlotNames | Self::InstanceSlot | Self::InstanceDictionary
            ),
            Self::OwnSlotDescriptor => matches!(
                helper,
                Self::SlotNames
                    | Self::SlotNameContains
                    | Self::GeneratedSlots
                    | Self::OwnClassBinding
                    | Self::InstanceSlot
            ),
            _ => false,
        };
        approved.then_some(helper)
    }
    pub(super) fn argument_count(self) -> usize {
        self.expected_signature().inputs.len()
    }
    fn pure_calls(self) -> &'static [(&'static str, usize)] {
        match self {
            Self::ClassStorage => &[("PlaceAndQualifiers :: default", 0)],
            Self::ClassOwnStorage => &[("Member :: default", 0)],
            Self::TypedDictClassification => &[
                ("ClassInstanceFlags :: contains", 2),
                ("KnownClass :: is_typed_dict_subclass", 1),
            ],
            Self::OwnClassBinding => &[("slot_bindings", 2)],
            Self::GeneratedSlots => &[("DataclassFlags :: contains", 2)],
            Self::SlotNames => &[("slot_definition_names", 1)],
            Self::InstanceSlot => &[("slot_layout_names", 1)],
            Self::InstanceDictionary => &[("slot_layout_has_dictionary", 1)],
            Self::OwnSlotDescriptor => &[("slot_is_dictionary_name", 1), ("str :: len", 1)],
            Self::SlotNameContains => &[
                ("slot_name_at", 2),
                ("slot_name_equal", 2),
                ("str :: len", 1),
                ("Name :: as_str", 1),
            ],
            Self::SlotNamedTupleBase => &[
                ("slot_base_at", 2),
                ("slot_is_named_tuple_base", 1),
                ("Type :: inline_payload_bytes", 1),
            ],
            _ => &[],
        }
    }
    pub(super) fn protects_name(self, name: &syn::Ident) -> bool {
        self.is_storage_or_slot()
            && matches!(
                name.to_string().trim_start_matches("r#"),
                "fields"
                    | "ClassInstanceFlags"
                    | "KnownClass"
                    | "DataclassFlags"
                    | "ClassType"
                    | "ClassLiteral"
                    | "InstanceMemberResult"
                    | "Place"
                    | "Member"
                    | "PlaceAndQualifiers"
                    | "Type"
                    | "Name"
                    | "str"
                    | "PythonVersion"
                    | "InstanceStorageWork"
                    | "SlotSelectorWork"
                    | "Ok"
                    | "Some"
                    | "None"
                    | "slot_bindings"
                    | "slot_definition_names"
                    | "slot_layout_names"
                    | "slot_layout_has_dictionary"
                    | "slot_name_at"
                    | "slot_base_at"
                    | "slot_name_equal"
                    | "slot_is_dictionary_name"
                    | "slot_is_named_tuple_base"
            )
    }

    pub(super) fn source_from_name(name: &syn::Ident) -> Option<Self> {
        match name.to_string().as_str() {
            "static_own_instance_member_with" => Some(Self::StaticInstance),
            "runtime_binding_absent_with" => Some(Self::RuntimeBinding),
            "implicit_attribute_bindings_with" => Some(Self::ImplicitAttribute),
            "static_code_generator_with" => Some(Self::StaticCodeGenerator),
            _ => None,
        }
    }
    fn is_source(self) -> bool {
        matches!(
            self,
            Self::StaticInstance
                | Self::RuntimeBinding
                | Self::ImplicitAttribute
                | Self::StaticCodeGenerator
        )
    }
    pub(super) fn attribute(self) -> MemberAttribute {
        match self {
            Self::ClassStorage => MemberAttribute::InstanceStorage,
            Self::ClassOwnStorage => MemberAttribute::InstanceStorage,
            Self::StaticStorage => MemberAttribute::InstanceStorage,
            Self::TypedDictClassification => MemberAttribute::InstanceStorage,
            Self::OwnClassBinding => MemberAttribute::SlotSelector,
            Self::GeneratedSlots => MemberAttribute::SlotSelector,
            Self::NamedTupleSlots => MemberAttribute::SlotSelector,
            Self::SlotNames => MemberAttribute::SlotSelector,
            Self::InstanceSlot => MemberAttribute::SlotSelector,
            Self::InstanceDictionary => MemberAttribute::SlotSelector,
            Self::LacksInstanceStorage => MemberAttribute::SlotSelector,
            Self::OwnSlotDescriptor => MemberAttribute::SlotSelector,
            Self::SlotNameContains => MemberAttribute::SlotSelector,
            Self::SlotNamedTupleBase => MemberAttribute::SlotSelector,
            Self::StaticInstance
            | Self::RuntimeBinding
            | Self::ImplicitAttribute
            | Self::StaticCodeGenerator => MemberAttribute::MemberSource,
            Self::Namespace => MemberAttribute::NamespaceLookup,
            Self::InstanceMro => MemberAttribute::InstanceMro,
        }
    }
    pub(super) fn body_name(self) -> &'static str {
        match self {
            Self::ClassStorage => "class_instance_member",
            Self::ClassOwnStorage => "class_own_instance_member",
            Self::StaticStorage => "static_instance_member",
            Self::TypedDictClassification => "static_is_typed_dict",
            Self::OwnClassBinding => "own_class_binding",
            Self::GeneratedSlots => "generated_slots",
            Self::NamedTupleSlots => "named_tuple_slots",
            Self::SlotNames => "slot_names",
            Self::InstanceSlot => "instance_slot",
            Self::InstanceDictionary => "instance_dictionary",
            Self::LacksInstanceStorage => "lacks_instance_storage",
            Self::OwnSlotDescriptor => "own_slot_descriptor",
            Self::SlotNameContains => "slot_name_contains",
            Self::SlotNamedTupleBase => "slot_named_tuple_base",
            Self::StaticInstance => "static_own_instance_member",
            Self::RuntimeBinding => "runtime_binding_absent",
            Self::ImplicitAttribute => "implicit_attribute_bindings",
            Self::StaticCodeGenerator => "static_code_generator",
            Self::Namespace => "namespace lookup",
            Self::InstanceMro => "instance MRO",
        }
    }
    pub(super) fn synchronous_name(self) -> &'static str {
        match self {
            Self::ClassStorage => "class_instance_member_sync",
            Self::ClassOwnStorage => "class_own_instance_member_sync",
            Self::StaticStorage => "static_instance_member_sync",
            Self::TypedDictClassification => "static_is_typed_dict_sync",
            Self::OwnClassBinding => "own_class_binding_sync",
            Self::GeneratedSlots => "generated_slots_sync",
            Self::NamedTupleSlots => "named_tuple_slots_sync",
            Self::SlotNames => "slot_names_sync",
            Self::InstanceSlot => "instance_slot_sync",
            Self::InstanceDictionary => "instance_dictionary_sync",
            Self::LacksInstanceStorage => "lacks_instance_storage_sync",
            Self::OwnSlotDescriptor => "own_slot_descriptor_sync",
            Self::SlotNameContains => "slot_name_contains_sync",
            Self::SlotNamedTupleBase => "slot_named_tuple_base_sync",
            Self::StaticInstance => "static_own_instance_member_sync",
            Self::RuntimeBinding => "runtime_binding_absent_sync",
            Self::ImplicitAttribute => "implicit_attribute_bindings_sync",
            Self::StaticCodeGenerator => "static_code_generator_sync",
            Self::Namespace => "namespace_lookup_sync",
            Self::InstanceMro => "mro_instance_member_sync",
        }
    }
    pub(super) fn synchronous_bound(self, span: Span) -> Path {
        match self {
            Self::ClassStorage => {
                syn::parse_quote_spanned!(span => crate::types::class::instance_storage::SynchronousClassInstanceStorageEffects<'db>)
            }
            Self::ClassOwnStorage => {
                syn::parse_quote_spanned!(span => crate::types::class::instance_storage::SynchronousClassInstanceStorageEffects<'db>)
            }
            Self::StaticStorage => {
                syn::parse_quote_spanned!(span => crate::types::class::instance_storage::SynchronousStaticInstanceStorageEffects<'db>)
            }
            Self::TypedDictClassification => {
                syn::parse_quote_spanned!(span => crate::types::class::instance_storage::SynchronousInstanceClassificationEffects<'db>)
            }
            Self::OwnClassBinding => {
                syn::parse_quote_spanned!(span => crate::types::class::SynchronousSlotSelectorEffects<'db>)
            }
            Self::GeneratedSlots => {
                syn::parse_quote_spanned!(span => crate::types::class::SynchronousSlotSelectorEffects<'db>)
            }
            Self::NamedTupleSlots => {
                syn::parse_quote_spanned!(span => crate::types::class::SynchronousSlotSelectorEffects<'db>)
            }
            Self::SlotNames => {
                syn::parse_quote_spanned!(span => crate::types::class::SynchronousSlotSelectorEffects<'db>)
            }
            Self::InstanceSlot => {
                syn::parse_quote_spanned!(span => crate::types::class::SynchronousSlotSelectorEffects<'db>)
            }
            Self::InstanceDictionary => {
                syn::parse_quote_spanned!(span => crate::types::class::SynchronousSlotSelectorEffects<'db>)
            }
            Self::LacksInstanceStorage => {
                syn::parse_quote_spanned!(span => crate::types::class::SynchronousSlotSelectorEffects<'db>)
            }
            Self::OwnSlotDescriptor => {
                syn::parse_quote_spanned!(span => crate::types::class::SynchronousSlotSelectorEffects<'db>)
            }
            Self::SlotNameContains => {
                syn::parse_quote_spanned!(span => crate::types::class::SynchronousSlotSelectorEffects<'db>)
            }
            Self::SlotNamedTupleBase => {
                syn::parse_quote_spanned!(span => crate::types::class::SynchronousSlotSelectorEffects<'db>)
            }
            Self::StaticInstance => {
                syn::parse_quote_spanned!(span => crate::types::class::member_source::SynchronousStaticInstanceMemberEffects<'db>)
            }
            Self::RuntimeBinding => {
                syn::parse_quote_spanned!(span => crate::types::class::member_source::SynchronousMemberSourceEffects<'db>)
            }
            Self::ImplicitAttribute => {
                syn::parse_quote_spanned!(span => crate::types::class::member_source::SynchronousImplicitAttributeEffects<'db>)
            }
            Self::StaticCodeGenerator => {
                syn::parse_quote_spanned!(span => crate::types::class::member_source::SynchronousStaticCodeGeneratorEffects<'db>)
            }
            Self::Namespace => {
                syn::parse_quote_spanned!(span => crate::types::class::namespace::SynchronousNamespaceLookupEffects<'db>)
            }
            Self::InstanceMro => {
                syn::parse_quote_spanned!(span => crate::types::class::member_lookup::SynchronousInstanceMroEffects<'db, C>)
            }
        }
    }
    pub(super) fn expected_signature(self) -> Signature {
        match self {
            Self::ClassStorage => {
                syn::parse_quote! { async fn class_instance_member_with<'a, 'db, E: ClassInstanceStorageEffects<'db>>(
                    env: &ProgramEnvironment<'db>,
                    class: ClassType<'db>,
                    name: &'a str,
                    effects: &E,
                ) -> Result<PlaceAndQualifiers<'db>, E::Error> }
            }
            Self::ClassOwnStorage => {
                syn::parse_quote! { async fn class_own_instance_member_with<'a, 'db, E: ClassInstanceStorageEffects<'db>>(
                    env: &ProgramEnvironment<'db>,
                    class: ClassType<'db>,
                    name: &'a str,
                    effects: &E,
                ) -> Result<Member<'db>, E::Error> }
            }
            Self::StaticStorage => {
                syn::parse_quote! { async fn static_instance_member_with<'a, 'db, E: StaticInstanceStorageEffects<'db>>(
                    env: &ProgramEnvironment<'db>,
                    class: StaticClassLiteral<'db>,
                    specialization: Option<Specialization<'db>>,
                    name: &'a str,
                    effects: &E,
                ) -> Result<PlaceAndQualifiers<'db>, E::Error> }
            }
            Self::TypedDictClassification => {
                syn::parse_quote! { async fn static_is_typed_dict_with<'db, E: InstanceClassificationEffects<'db>>(
                    class: StaticClassLiteral<'db>,
                    effects: &E,
                ) -> Result<bool, E::Error> }
            }
            Self::OwnClassBinding => {
                syn::parse_quote! { async fn own_class_binding_with<'a, 'db, E: SlotSelectorEffects<'db>>(
                    class: StaticClassLiteral<'db>, name: &'a str, effects: &E,
                ) -> Result<bool, E::Error> }
            }
            Self::GeneratedSlots => {
                syn::parse_quote! { async fn generated_slots_with<'db, E: SlotSelectorEffects<'db>>(
                    class: StaticClassLiteral<'db>, effects: &E,
                ) -> Result<bool, E::Error> }
            }
            Self::NamedTupleSlots => {
                syn::parse_quote! { async fn named_tuple_slots_with<'db, E: SlotSelectorEffects<'db>>(
                    class: StaticClassLiteral<'db>, effects: &E,
                ) -> Result<bool, E::Error> }
            }
            Self::SlotNames => {
                syn::parse_quote! { async fn slot_names_with<'db, E: SlotSelectorEffects<'db>>(
                    class: StaticClassLiteral<'db>, effects: &E,
                ) -> Result<Option<&'db [Name]>, E::Error> }
            }
            Self::InstanceSlot => {
                syn::parse_quote! { async fn instance_slot_with<'a, 'db, E: SlotSelectorEffects<'db>>(
                    class: StaticClassLiteral<'db>, name: &'a str, effects: &E,
                ) -> Result<bool, E::Error> }
            }
            Self::InstanceDictionary => {
                syn::parse_quote! { async fn instance_dictionary_with<'db, E: SlotSelectorEffects<'db>>(
                    class: StaticClassLiteral<'db>, effects: &E,
                ) -> Result<bool, E::Error> }
            }
            Self::LacksInstanceStorage => {
                syn::parse_quote! { async fn lacks_instance_storage_with<'a, 'db, E: SlotSelectorEffects<'db>>(
                    class: StaticClassLiteral<'db>, name: &'a str, effects: &E,
                ) -> Result<bool, E::Error> }
            }
            Self::OwnSlotDescriptor => {
                syn::parse_quote! { async fn own_slot_descriptor_with<'a, 'db, E: SlotSelectorEffects<'db>>(
                    class: StaticClassLiteral<'db>, name: &'a str, effects: &E,
                ) -> Result<bool, E::Error> }
            }
            Self::SlotNameContains => {
                syn::parse_quote! { async fn slot_name_contains_with<'a, 'b, 'db, E: SlotSelectorEffects<'db>>(
                    slots: &'a [Name], name: &'b str, effects: &E,
                ) -> Result<bool, E::Error> }
            }
            Self::SlotNamedTupleBase => {
                syn::parse_quote! { async fn slot_named_tuple_base_with<'a, 'db, E: SlotSelectorEffects<'db>>(
                    bases: &'a [Type<'db>], effects: &E,
                ) -> Result<bool, E::Error> }
            }
            Self::StaticInstance => {
                syn::parse_quote! { async fn static_own_instance_member_with<'a, 'db, E: StaticInstanceMemberEffects<'db>>( env: &ProgramEnvironment<'db>, class: StaticClassLiteral<'db>, name: &'a str, effects: &E,) -> Result<Member<'db>, E::Error> }
            }
            Self::RuntimeBinding => {
                syn::parse_quote! { async fn runtime_binding_absent_with<'a, 'db, E: MemberSourceEffects<'db>>(env: &ProgramEnvironment<'db>, scope: ScopeId<'db>, name: &'a str, effects: &E,) -> Result<bool, E::Error> }
            }
            Self::ImplicitAttribute => {
                syn::parse_quote! { async fn implicit_attribute_bindings_with<'a, 'db, E: ImplicitAttributeEffects<'db>>( class: StaticClassLiteral<'db>, name: &'a str, target: MethodDecorator, effects: &E,) -> Result<ImplicitAttribute<'db>, E::Error> }
            }
            Self::StaticCodeGenerator => {
                syn::parse_quote! { async fn static_code_generator_with<'db, E: StaticCodeGeneratorEffects<'db>>(class: StaticClassLiteral<'db>, effects: &E,) -> Result<Option<CodeGeneratorKind<'db>>, E::Error> }
            }
            Self::Namespace => syn::parse_quote! {
                async fn namespace_lookup_with<'a, 'db, E: NamespaceLookupEffects<'db>>(
                    lookup_ty: Type<'db>, request: NamespaceLookupRequest<'a, 'db>, effects: &E,
                ) -> Result<PlaceAndQualifiers<'db>, E::Error>
            },
            Self::InstanceMro => syn::parse_quote! {
                async fn mro_instance_member_with<'a, 'db, C, E: InstanceMroEffects<'db, C>>(
                    name: &'a str, mut cursor: C, effects: &E,
                ) -> Result<InstanceMemberResult<'db>, E::Error>
            },
        }
    }
    pub(super) fn effect_methods(self) -> &'static [&'static str] {
        match self {
            Self::ClassStorage => &[
                "alias_origin",
                "alias_specialization",
                "checkpoint",
                "dynamic_instance_member",
                "named_tuple_instance_member",
                "enum_instance_member",
                "is_typed_dict",
                "static_instance_member",
                "specialize_place",
            ],
            Self::ClassOwnStorage => &[
                "alias_origin",
                "alias_specialization",
                "checkpoint",
                "dynamic_own_instance_member",
                "named_tuple_own_instance_member",
                "enum_own_instance_member",
                "static_own_instance_member",
                "specialize_member",
            ],
            Self::StaticStorage => &[
                "storage_checkpoint",
                "is_typed_dict",
                "lacks_instance_storage",
                "mro_instance_member",
                "typed_dict_fallback",
            ],
            Self::TypedDictClassification => &[
                "checkpoint",
                "known",
                "has_explicit_bases",
                "instance_flags",
            ],
            Self::OwnClassBinding => &[
                "body_scope",
                "slot_checkpoint",
                "place_table",
                "symbol_id",
                "use_def_map",
                "next_binding_has_definition",
            ],
            Self::GeneratedSlots => &[
                "slot_checkpoint",
                "dataclass_params",
                "dataclass_flags",
                "body_scope",
                "source_python_version",
            ],
            Self::NamedTupleSlots => &["slot_checkpoint", "has_explicit_bases", "explicit_bases"],
            Self::SlotNames => &["slot_checkpoint", "slot_definition"],
            Self::InstanceSlot => &["slot_checkpoint", "instance_layout"],
            Self::InstanceDictionary => &["slot_checkpoint", "known", "instance_layout"],
            Self::LacksInstanceStorage => &["slot_checkpoint"],
            Self::OwnSlotDescriptor => &["slot_checkpoint", "is_stub"],
            Self::SlotNameContains => &["slot_checkpoint"],
            Self::SlotNamedTupleBase => &["slot_checkpoint"],
            Self::StaticInstance => &[
                "body_scope",
                "checkpoint",
                "place_table",
                "use_def_map",
                "symbol_id",
                "binding_place",
                "code_generator",
                "has_own_named_tuple_field",
                "declaration_place",
                "imported_final",
                "implicit_member",
                "is_kw_only",
                "is_stub",
                "has_instance_slot",
                "is_own_dataclass_instance_field",
                "getter_member",
                "union_two",
            ],
            Self::RuntimeBinding => &[
                "checkpoint",
                "place_table",
                "use_def_map",
                "symbol_id",
                "binding_place",
            ],
            Self::ImplicitAttribute => {
                &["body_scope", "checkpoint", "names", "find_name", "infer_named_attribute"]
            }
            Self::StaticCodeGenerator => &[
                "checkpoint",
                "dataclass_params",
                "known",
                "has_explicit_bases",
                "has_explicit_metaclass",
                "code_generator_query",
            ],
            Self::Namespace => &[
                "checkpoint",
                "find_in_mro",
                "inferred_metaclass",
                "for_inheritance",
                "instance_approximation",
                "nominal_class",
                "instance_member",
                "own_member",
                "runtime_binding_absent",
                "inherited_member",
                "fall_back_to",
                "start_dynamic_mro",
                "next_dynamic_base",
                "may_be_data_descriptor",
                "filter_possible_data_descriptors",
            ],
            Self::InstanceMro => &[
                "checkpoint",
                "new_union",
                "advance",
                "own_instance_member",
                "implicit_attribute",
                "push_pending",
                "clear_pending",
                "finish_pending",
                "infer_augmented",
                "union_add",
                "own_class_member",
                "is_definitely_non_data_descriptor",
                "union_build",
            ],
        }
    }
    pub(super) fn validate_body(self, body: &syn::Block) -> Result<()> {
        let mut validator = LookupBody {
            manifest: self,
            error: None,
        };
        validator.visit_block_mut(&mut body.clone());
        match validator.error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

struct LookupBody {
    manifest: LookupManifest,
    error: Option<Error>,
}

impl LookupBody {
    fn reject(&mut self, span: Span, message: &str) {
        if self.error.is_none() {
            self.error = Some(Error::new(span, message));
        }
    }
}

impl VisitMut for LookupBody {
    fn visit_expr_loop_mut(&mut self, expression: &mut syn::ExprLoop) {
        if self.manifest.is_storage_or_slot()
            && !matches!(
                self.manifest,
                LookupManifest::OwnClassBinding
                    | LookupManifest::SlotNameContains
                    | LookupManifest::SlotNamedTupleBase
            )
        {
            self.reject(
                expression.span(),
                "only declared binding and indexed scans may loop",
            );
        }
        visit_mut::visit_expr_loop_mut(self, expression);
    }
    fn visit_expr_while_mut(&mut self, expression: &mut syn::ExprWhile) {
        if self.manifest.is_storage_or_slot() {
            self.reject(
                expression.span(),
                "slot scans use the declared admitted loop",
            );
        }
        visit_mut::visit_expr_while_mut(self, expression);
    }
    fn visit_expr_struct_mut(&mut self, expression: &mut syn::ExprStruct) {
        if self.manifest.is_storage_or_slot() {
            self.validate_work_path(&expression.path, true);
            if expression.qself.is_some() || expression.rest.is_some() {
                self.reject(expression.span(), "work quotes require their exact fields");
            }
        }
        visit_mut::visit_expr_struct_mut(self, expression);
    }

    fn visit_expr_path_mut(&mut self, path: &mut syn::ExprPath) {
        if self.manifest.is_storage_or_slot() {
            self.validate_storage_path(path);
            return;
        }

        if path.path.is_ident("db") || (!self.manifest.is_source() && path.path.is_ident("env")) {
            self.reject(path.span(), "shared lookup bodies cannot access db or env");
        }
        visit_mut::visit_expr_path_mut(self, path);
    }
    fn visit_expr_call_mut(&mut self, call: &mut syn::ExprCall) {
        if self.manifest.is_storage_or_slot() {
            self.validate_storage_call(call);
            return;
        }

        let approved = if let Expr::Path(path) = &*call.func {
            let name = path.path.to_token_stream().to_string();
            let common = matches!(name.as_str(), "Ok" | "Some" | "Place :: Defined");
            let specific = match self.manifest {
                LookupManifest::ClassStorage
                | LookupManifest::ClassOwnStorage
                | LookupManifest::StaticStorage
                | LookupManifest::TypedDictClassification
                | LookupManifest::OwnClassBinding
                | LookupManifest::GeneratedSlots
                | LookupManifest::NamedTupleSlots
                | LookupManifest::SlotNames
                | LookupManifest::InstanceSlot
                | LookupManifest::InstanceDictionary
                | LookupManifest::LacksInstanceStorage
                | LookupManifest::OwnSlotDescriptor
                | LookupManifest::SlotNameContains
                | LookupManifest::SlotNamedTupleBase => false,
                LookupManifest::StaticInstance
                | LookupManifest::RuntimeBinding
                | LookupManifest::ImplicitAttribute
                | LookupManifest::StaticCodeGenerator => name == "Member :: unbound",
                LookupManifest::Namespace => matches!(
                    name.as_str(),
                    "NamespaceLookupStep :: start" | "Type :: from" | "Place :: bound"
                ),
                LookupManifest::InstanceMro => matches!(
                    name.as_str(),
                    "Vec :: new"
                        | "TypeQualifiers :: empty"
                        | "PlaceAndQualifiers :: unbound"
                        | "MroPendingBindings"
                        | "InstanceMemberResult :: Done"
                ),
            };
            path.qself.is_none() && (common || specific)
        } else {
            false
        };
        if !approved {
            self.reject(call.span(), "call is not a declared finite lookup helper");
        }
        visit_mut::visit_expr_call_mut(self, call);
    }
    fn visit_expr_method_call_mut(&mut self, call: &mut syn::ExprMethodCall) {
        if self.manifest.is_storage_or_slot() {
            self.validate_storage_method(call);
            return;
        }

        // This conversion is finite only inside its canonical, typed consumer. Allowing
        // `into` on any local named `symbol_id` would also admit allocating conversions.
        if matches!(self.manifest, LookupManifest::StaticInstance)
            && Expr::MethodCall(call.clone())
                == syn::parse_quote!(
                    use_def.end_of_scope_imported_final_candidates(symbol_id.into())
                )
        {
            return;
        }
        if is_identifier(&call.receiver, "effects") {
            if !self
                .manifest
                .effect_methods()
                .contains(&call.method.to_string().as_str())
            {
                self.reject(call.span(), "unknown lookup effect");
            }
        } else {
            let method = call.method.to_string();
            let approved = match self.manifest {
                LookupManifest::ClassStorage
                | LookupManifest::ClassOwnStorage
                | LookupManifest::StaticStorage
                | LookupManifest::TypedDictClassification
                | LookupManifest::OwnClassBinding
                | LookupManifest::GeneratedSlots
                | LookupManifest::NamedTupleSlots
                | LookupManifest::SlotNames
                | LookupManifest::InstanceSlot
                | LookupManifest::InstanceDictionary
                | LookupManifest::LacksInstanceStorage
                | LookupManifest::OwnSlotDescriptor
                | LookupManifest::SlotNameContains
                | LookupManifest::SlotNamedTupleBase => false,
                LookupManifest::StaticInstance
                | LookupManifest::RuntimeBinding
                | LookupManifest::ImplicitAttribute
                | LookupManifest::StaticCodeGenerator => match method.as_str() {
                    "dataclass_params"
                    | "static_known"
                    | "has_explicit_bases"
                    | "has_explicit_metaclass" => is_identifier(&call.receiver, "fields"),
                    "end_of_scope_symbol_declarations" | "end_of_scope_symbol_bindings" => {
                        is_identifier(&call.receiver, "use_def")
                    }
                    "ignore_conflicting_declarations" => is_identifier(&call.receiver, "result"),
                    "contains" => {
                        matches!(self.manifest, LookupManifest::StaticInstance)
                            && is_identifier(&call.receiver, "qualifiers")
                            && call.args.len() == 1
                            && (call.args.first()
                                == Some(&syn::parse_quote!(TypeQualifiers::CLASS_VAR))
                                || call.args.first()
                                    == Some(&syn::parse_quote!(TypeQualifiers::INIT_VAR)))
                    }
                    "is_init_var" => is_identifier(&call.receiver, "place_and_quals"),
                    "is_undefined" => {
                        is_identifier(&call.receiver, "inferred")
                            || is_identifier(&call.receiver, "place_and_quals")
                            || *call.receiver == syn::parse_quote!(binding.place)
                            || *call.receiver
                                == syn::parse_quote!({
                                    effects
                                        .checkpoint(MemberSourceWork::ImplicitInference)
                                        .await?;
                                    effects.implicit_member(class, name).await?
                                })
                            || *call.receiver
                                == syn::parse_quote!(
                                    {
                                        effects.checkpoint(MemberSourceWork::Getter).await?;
                                        effects.getter_member(env, declared_ty).await?
                                    }
                                    .place
                                )
                    }
                    "is_some_and" => {
                        *call.receiver
                            == syn::parse_quote!({
                                effects.checkpoint(MemberSourceWork::CodeGenerator).await?;
                                effects.code_generator(class).await?
                            })
                            && call.args.len() == 1
                            && call.args.first()
                                == Some(&syn::parse_quote!(CodeGeneratorKind::is_dataclass_like))
                    }
                    "is_none" => {
                        *call.receiver == syn::parse_quote!(effects.dataclass_params(class).await?)
                            || *call.receiver == syn::parse_quote!(effects.known(class).await?)
                    }
                    "or" => {
                        is_identifier(&call.receiver, "implicit_provenance")
                            || *call.receiver == syn::parse_quote!(place.provenance)
                    }
                    "with_qualifiers" => {
                        is_identifier(&call.receiver, "declared")
                            || *call.receiver == syn::parse_quote!(Place::Undefined)
                            || matches!(&*call.receiver, Expr::Call(receiver)
                            if matches!(&*receiver.func, Expr::Path(path) if path.qself.is_none() && path.path == syn::parse_quote!(Place::Defined))
                                && receiver.args.len() == 1
                                && matches!(receiver.args.first(), Some(Expr::Struct(value)) if value.qself.is_none() && value.path.is_ident("DefinedPlace")))
                    }
                    _ => false,
                },
                LookupManifest::Namespace => match method.as_str() {
                    "class" | "request" | "resume" => is_identifier(&call.receiver, "pending"),
                    "ignore_possibly_undefined" => is_identifier(&call.receiver, "class_member"),
                    "into" => {
                        *call.receiver == syn::parse_quote!(Place::bound(dynamic_instance_type))
                    }
                    "with_qualifiers" => matches!(&*call.receiver,
                        Expr::Call(receiver) if matches!(&*receiver.func,
                            Expr::Path(path) if path.qself.is_none() && path.path == syn::parse_quote!(Place::Defined))
                            && receiver.args.len() == 1
                            && matches!(receiver.args.first(), Some(Expr::Struct(value)) if value.qself.is_none() && value.path.is_ident("DefinedPlace"))),
                    "expect" => {
                        Expr::MethodCall(call.clone())
                            == syn::parse_quote! {
                                effects.find_in_mro(lookup_ty, name, policy).await?
                                    .expect("The meta-type of an instance-like type should always have an MRO")
                            }
                    }
                    _ => false,
                },
                LookupManifest::InstanceMro => match method.as_str() {
                    "len" => {
                        is_identifier(&call.receiver, "pending_augmented_bindings")
                    }
                    "is_empty" => {
                        is_identifier(&call.receiver, "pending_augmented_bindings")
                            || is_identifier(&call.receiver, "union")
                    }
                    "is_some" | "is_some_and" => {
                        is_identifier(&call.receiver, "definitely_bound_member")
                    }
                    "is_declared" => is_identifier(&call.receiver, "origin"),
                    "or" => is_identifier(&call.receiver, "provenance"),
                    "is_undefined" => {
                        is_identifier(&call.receiver, "member")
                            || *call.receiver == syn::parse_quote!(implicit.member)
                    }
                    "contains" => {
                        is_identifier(&call.receiver, "qualifiers")
                            || *call.receiver == syn::parse_quote!(member.qualifiers)
                    }
                    "with_qualifiers" => {
                        *call.receiver == syn::parse_quote!(Place::Undefined)
                            || matches!(&*call.receiver,
                            Expr::Call(receiver) if matches!(&*receiver.func,
                                Expr::Path(path) if path.qself.is_none() && path.path == syn::parse_quote!(Place::Defined))
                                && receiver.args.len() == 1
                                && matches!(receiver.args.first(), Some(Expr::Struct(value)) if value.qself.is_none() && value.path.is_ident("DefinedPlace")))
                    }
                    _ => false,
                },
            };
            if !approved || call.turbofish.is_some() {
                self.reject(
                    call.span(),
                    "method is not a declared finite lookup operation",
                );
            }
        }
        visit_mut::visit_expr_method_call_mut(self, call);
    }
    fn visit_expr_closure_mut(&mut self, closure: &mut syn::ExprClosure) {
        let approved = matches!(self.manifest, LookupManifest::InstanceMro)
            && Expr::Closure(closure.clone())
                == syn::parse_quote! {
                    |member| { !member.qualifiers.contains(TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE) }
                };
        if !approved {
            self.reject(
                closure.span(),
                "only the declared native qualifier predicate is supported",
            );
        }
        visit_mut::visit_expr_closure_mut(self, closure);
    }
    fn visit_expr_for_loop_mut(&mut self, loop_: &mut syn::ExprForLoop) {
        self.reject(
            loop_.span(),
            "lookup iteration requires the declared advance effect",
        );
    }
    fn visit_macro_mut(&mut self, invocation: &mut syn::Macro) {
        if invocation.path.is_ident("matches") {
            match syn::parse2::<MatchesInput>(invocation.tokens.clone()) {
                Ok(mut input) => {
                    // Macro tokens are otherwise opaque to this visitor. Apply the same
                    // helper checks before the lowerer validates effect use in `matches!`.
                    self.visit_expr_mut(&mut input.expression);
                    self.visit_pat_mut(&mut input.pattern);
                    if let Some(guard) = &mut input.guard {
                        self.visit_expr_mut(guard);
                    }
                }
                Err(error) if self.error.is_none() => self.error = Some(error),
                Err(_) => {}
            }
        }
    }
}

impl LookupBody {
    fn validate_storage_path(&mut self, path: &syn::ExprPath) {
        let spelling = path.path.to_token_stream().to_string();
        if matches!(spelling.as_str(), "db" | "r#db" | "fields" | "r#fields")
            || (spelling == "env"
                && !matches!(
                    self.manifest,
                    LookupManifest::ClassStorage
                        | LookupManifest::ClassOwnStorage
                        | LookupManifest::StaticStorage
                ))
        {
            self.reject(
                path.span(),
                "stored fields require their declared receiver and shared storage cannot access db",
            );
        }
        if let Some(first) = path.path.segments.first() {
            if first.ident == "InstanceStorageWork" || first.ident == "SlotSelectorWork" {
                self.validate_work_path(&path.path, false);
            }
            if self.manifest.protects_name(&first.ident)
                && matches!(
                    first.ident.to_string().as_str(),
                    "slot_bindings"
                        | "slot_definition_names"
                        | "slot_layout_names"
                        | "slot_layout_has_dictionary"
                        | "slot_name_at"
                        | "slot_base_at"
                        | "slot_name_equal"
                        | "slot_is_dictionary_name"
                        | "slot_is_named_tuple_base"
                )
            {
                self.reject(
                    path.span(),
                    "finite storage helpers cannot escape their direct call",
                );
            }
        }
    }

    fn validate_work_path(&mut self, path: &Path, structure: bool) {
        let spelling = path.to_token_stream().to_string();
        let approved = match self.manifest.attribute() {
            MemberAttribute::InstanceStorage => {
                !structure
                    && matches!(
                        spelling.as_str(),
                        "InstanceStorageWork :: Begin"
                            | "InstanceStorageWork :: Dispatch"
                            | "InstanceStorageWork :: AliasOrigin"
                            | "InstanceStorageWork :: AliasSpecialization"
                            | "InstanceStorageWork :: TypedDict"
                            | "InstanceStorageWork :: DynamicMember"
                            | "InstanceStorageWork :: OwnDynamicMember"
                            | "InstanceStorageWork :: StaticMember"
                            | "InstanceStorageWork :: OwnStaticMember"
                            | "InstanceStorageWork :: OwnerSpecialization"
                            | "InstanceStorageWork :: StorageCheck"
                            | "InstanceStorageWork :: Mro"
                            | "InstanceStorageWork :: TypedDictFallback"
                            | "InstanceStorageWork :: Header"
                            | "InstanceStorageWork :: InstanceFlags"
                            | "InstanceStorageWork :: Publish"
                    )
            }
            MemberAttribute::SlotSelector => {
                if structure {
                    matches!(
                        spelling.as_str(),
                        "SlotSelectorWork :: BaseCompare"
                            | "SlotSelectorWork :: NameCompare"
                            | "SlotSelectorWork :: DictionaryName"
                    )
                } else {
                    matches!(
                        spelling.as_str(),
                        "SlotSelectorWork :: Begin"
                            | "SlotSelectorWork :: Header"
                            | "SlotSelectorWork :: PlaceTable"
                            | "SlotSelectorWork :: Symbol"
                            | "SlotSelectorWork :: UseDef"
                            | "SlotSelectorWork :: Bindings"
                            | "SlotSelectorWork :: BindingAdvance"
                            | "SlotSelectorWork :: ExplicitBases"
                            | "SlotSelectorWork :: BaseAdvance"
                            | "SlotSelectorWork :: Version"
                            | "SlotSelectorWork :: SlotDefinition"
                            | "SlotSelectorWork :: Layout"
                            | "SlotSelectorWork :: NameAdvance"
                            | "SlotSelectorWork :: Stub"
                            | "SlotSelectorWork :: Publish"
                    )
                }
            }
            _ => false,
        };
        if !approved {
            self.reject(path.span(), "unknown storage work constructor");
        }
    }

    fn validate_storage_call(&mut self, call: &mut syn::ExprCall) {
        let Expr::Path(path) = &*call.func else {
            self.reject(
                call.span(),
                "storage helpers require a declared direct path",
            );
            return;
        };
        if path.qself.is_some()
            || path.path.leading_colon.is_some()
            || path
                .path
                .segments
                .iter()
                .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        {
            self.reject(
                call.span(),
                "storage helpers require their exact typed paths",
            );
        }
        if let Some(name) = path.path.get_ident()
            && let Some(helper) = self.manifest.free_helper(name)
        {
            if call.args.len() != helper.argument_count()
                || call
                    .args
                    .last()
                    .is_none_or(|arg| !is_identifier(arg, "effects"))
            {
                self.reject(
                    call.span(),
                    "storage helpers require exact arguments and capabilities",
                );
            }
            let count = call.args.len();
            for (index, argument) in call.args.iter_mut().enumerate() {
                if index + 1 != count {
                    self.visit_expr_mut(argument);
                }
            }
            return;
        }
        let spelling = path.path.to_token_stream().to_string();
        let approved = (matches!(spelling.as_str(), "Ok" | "Some") && call.args.len() == 1)
            || self
                .manifest
                .pure_calls()
                .contains(&(spelling.as_str(), call.args.len()));
        if !approved {
            self.reject(call.span(), "call is not a declared finite storage helper");
        }
        let exact = match spelling.as_str() {
            "ClassInstanceFlags :: contains" => {
                Expr::Call(call.clone())
                    == syn::parse_quote!(ClassInstanceFlags::contains(
                        &flags,
                        ClassInstanceFlags::TYPED_DICT
                    ))
            }
            "DataclassFlags :: contains" => {
                Expr::Call(call.clone())
                    == syn::parse_quote!(DataclassFlags::contains(&flags, DataclassFlags::SLOTS))
            }
            "Name :: as_str" => {
                Expr::Call(call.clone()) == syn::parse_quote!(Name::as_str(candidate))
            }
            "str :: len" => {
                Expr::Call(call.clone()) == syn::parse_quote!(str::len(name))
                    || (matches!(self.manifest, LookupManifest::SlotNameContains)
                        && Expr::Call(call.clone())
                            == syn::parse_quote!(str::len(Name::as_str(candidate))))
            }
            "Type :: inline_payload_bytes" => {
                Expr::Call(call.clone()) == syn::parse_quote!(Type::inline_payload_bytes(base))
            }
            _ => true,
        };
        if !exact {
            self.reject(
                call.span(),
                "finite storage operation requires its exact typed arguments",
            );
        }
        for argument in &mut call.args {
            self.visit_expr_mut(argument);
        }
    }

    fn validate_storage_method(&mut self, call: &mut syn::ExprMethodCall) {
        let approved = if is_identifier(&call.receiver, "effects") {
            self.manifest
                .effect_methods()
                .contains(&call.method.to_string().as_str())
        } else {
            matches!(
                self.manifest,
                LookupManifest::ClassStorage | LookupManifest::StaticStorage
            ) && Expr::MethodCall(call.clone()) == syn::parse_quote!(Place::Undefined.into())
        };
        if !approved || call.turbofish.is_some() {
            self.reject(
                call.span(),
                "method is not a declared finite storage operation",
            );
        }
        for argument in &mut call.args {
            self.visit_expr_mut(argument);
        }
    }
}
