use crate::types::class::{
    ClassLiteral, DynamicClassAnchor, DynamicClassLiteral, DynamicClassMember,
    DynamicMetaclassConflict,
};
use crate::types::diagnostic::{
    INVALID_ARGUMENT_TYPE, NO_MATCHING_OVERLOAD, report_conflicting_metaclass_from_bases,
    report_instance_layout_conflict,
};
use crate::types::infer::builder::{
    TypeInferenceBuilder,
    dynamic_class::{DynamicClassKind, report_dynamic_mro_errors},
};
use crate::types::{
    KnownClass, SubclassOfType, Type, TypeContext, definition_expression_type, overrides,
    typed_dict::extract_unpacked_typed_dict_keys_from_value_type,
};
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, HasNodeIndex, NodeIndex};
use ruff_text_size::Ranged;
use rustc_hash::FxHashMap;
use ty_python_core::definition::Definition;

/// Tracks namespace members that are guaranteed to survive until class creation.
///
/// The order matches the dictionary's insertion order, but later writes to the same string key
/// replace earlier writes. That matches Python's dictionary semantics:
///
/// ```python
/// C = type("C", (), {"method": first, "method": second})
/// ```
///
/// In this example, override diagnostics must consider only `second`.
#[derive(Default)]
struct GuaranteedDynamicClassMembers<'db> {
    members: Vec<DynamicClassMember<'db>>,
    member_indices: FxHashMap<Name, usize>,
}

impl<'db> GuaranteedDynamicClassMembers<'db> {
    /// Inserts a guaranteed member, replacing an earlier member with the same name.
    fn insert(&mut self, member: DynamicClassMember<'db>) {
        if let Some(index) = self.member_indices.get(&member.name) {
            self.members[*index] = member;
        } else {
            self.member_indices
                .insert(member.name.clone(), self.members.len());
            self.members.push(member);
        }
    }

    /// Removes all currently guaranteed members after an uncertain namespace write.
    ///
    /// For example, a dynamic key could overwrite any previous string-named member:
    ///
    /// ```python
    /// C = type("C", (), {"method": good, maybe_name: bad})
    /// ```
    fn clear(&mut self) {
        self.members.clear();
        self.member_indices.clear();
    }

    /// Returns the surviving members in dictionary insertion order.
    fn into_vec(self) -> Vec<DynamicClassMember<'db>> {
        self.members
    }
}

/// Members extracted from a dynamic class namespace, plus uncertainty metadata.
///
/// `has_dynamic_namespace` controls normal attribute lookup for unknown members, while
/// `has_uncertain_writes` controls whether earlier guaranteed override members must be discarded.
struct ExtractedDynamicNamespace<'db> {
    members: Vec<DynamicClassMember<'db>>,
    has_dynamic_namespace: bool,
    has_uncertain_writes: bool,
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Extracts namespace members that are safe to use for dynamic override diagnostics.
    ///
    /// This is more conservative than normal dynamic member extraction: only entries that are
    /// guaranteed to be present at class creation are returned.
    ///
    /// ```python
    /// class Base:
    ///     def method(self, x: int) -> None: ...
    ///
    /// def bad(self, x: str) -> None: ...
    ///
    /// C = type("C", (Base,), {"method": bad})
    /// ```
    ///
    /// The `method` entry is guaranteed, so it can produce an `invalid-method-override`
    /// diagnostic.
    fn extract_dynamic_override_members(
        &self,
        namespace_arg: &ast::Expr,
        namespace_type: Type<'db>,
    ) -> (Box<[DynamicClassMember<'db>]>, bool) {
        if let ast::Expr::Dict(dict) = namespace_arg {
            let extracted = self.extract_dynamic_override_members_from_dict(dict);
            return (
                extracted.members.into_boxed_slice(),
                extracted.has_dynamic_namespace,
            );
        }

        if let Type::TypedDict(typed_dict) = namespace_type {
            // `namespace` is a TypedDict instance. Only required keys are guaranteed to be
            // present at runtime; optional keys may be omitted.
            let members: Box<[DynamicClassMember<'db>]> = typed_dict
                .items(self.db())
                .iter()
                .filter_map(|(name, field)| {
                    field.is_required().then_some(DynamicClassMember {
                        name: name.clone(),
                        ty: field.declared_ty,
                        range: namespace_arg.range(),
                    })
                })
                .collect();

            // TypedDicts are "open" (can have additional string keys), so this is still a
            // dynamic namespace for unknown attributes.
            return (members, true);
        }

        // `namespace` is not a dict literal, so it's dynamic.
        (Box::new([]), true)
    }

    /// Extracts guaranteed override members from a literal dictionary namespace.
    ///
    /// Spread entries and non-literal keys are treated conservatively because they can overwrite
    /// earlier values:
    ///
    /// ```python
    /// C = type("C", (), {"method": first, **namespace, "other": value})
    /// ```
    ///
    /// After `**namespace`, only later entries with known string keys are guaranteed.
    fn extract_dynamic_override_members_from_dict(
        &self,
        dict: &ast::ExprDict,
    ) -> ExtractedDynamicNamespace<'db> {
        let mut guaranteed_members = GuaranteedDynamicClassMembers::default();
        let mut has_dynamic_namespace = false;
        let mut has_uncertain_writes = false;

        for item in &dict.items {
            if let Some(key_expr) = item.key.as_ref() {
                let key_ty = self.expression_type(key_expr);
                let Some(key_name) = key_ty.as_string_literal() else {
                    has_dynamic_namespace = true;

                    // Keys that cannot be strings still make the namespace dynamic, but they
                    // cannot shadow string-named members and therefore should not erase
                    // guaranteed override diagnostics for earlier entries.
                    if !key_ty.is_disjoint_from(self.db(), KnownClass::Str.to_instance(self.db())) {
                        guaranteed_members.clear();
                        has_uncertain_writes = true;
                    }
                    continue;
                };

                guaranteed_members.insert(DynamicClassMember {
                    name: ast::name::Name::new(key_name.value(self.db())),
                    ty: self.expression_type(&item.value),
                    range: item.value.range(),
                });
                continue;
            }

            let unpacked = self.extract_unpacked_dynamic_override_members(&item.value);
            if unpacked.has_uncertain_writes {
                guaranteed_members.clear();
            }
            for member in unpacked.members {
                guaranteed_members.insert(member);
            }
            has_dynamic_namespace |= unpacked.has_dynamic_namespace;
            has_uncertain_writes |= unpacked.has_uncertain_writes;
        }

        ExtractedDynamicNamespace {
            members: guaranteed_members.into_vec(),
            has_dynamic_namespace,
            has_uncertain_writes,
        }
    }

    /// Extracts guaranteed override members from a dictionary unpack in a namespace literal.
    ///
    /// Literal unpacked dictionaries preserve their own guaranteed keys, while a typed-dict unpack
    /// only guarantees required keys:
    ///
    /// ```python
    /// namespace = {"method": bad}
    /// C = type("C", (Base,), {**namespace})
    /// ```
    ///
    /// If the unpacked value is not statically understood, the namespace is treated as dynamic and
    /// as an uncertain write.
    fn extract_unpacked_dynamic_override_members(
        &self,
        unpacked: &ast::Expr,
    ) -> ExtractedDynamicNamespace<'db> {
        if let ast::Expr::Dict(dict) = unpacked {
            return self.extract_dynamic_override_members_from_dict(dict);
        }

        let unpacked_type = self.expression_type(unpacked);
        if let Some(unpacked_keys) =
            extract_unpacked_typed_dict_keys_from_value_type(self.db(), unpacked_type)
        {
            let members = unpacked_keys
                .into_iter()
                .filter_map(|(name, unpacked_key)| {
                    unpacked_key.is_required.then_some(DynamicClassMember {
                        name,
                        ty: unpacked_key.value_ty,
                        range: unpacked.range(),
                    })
                })
                .collect();

            return ExtractedDynamicNamespace {
                members,
                has_dynamic_namespace: true,
                has_uncertain_writes: true,
            };
        }

        ExtractedDynamicNamespace {
            members: Vec::new(),
            has_dynamic_namespace: true,
            has_uncertain_writes: true,
        }
    }

    /// Infer a call to `builtins.type()`.
    ///
    /// `builtins.type` has two overloads: a single-argument overload (e.g. `type("foo")`,
    /// and a 3-argument `type(name, bases, dict)` overload. Both are handled here.
    /// The `definition` parameter should be `Some()` if this call to `builtins.type()`
    /// occurs on the right-hand side of an assignment statement that has a [`Definition`]
    /// associated with it in the semantic index.
    ///
    /// If it's unclear which overload we should pick, we return `type[Unknown]`,
    /// to avoid cascading errors later on.
    pub(super) fn infer_builtins_type_call(
        &mut self,
        call_expr: &ast::ExprCall,
        definition: Option<Definition<'db>>,
    ) -> Type<'db> {
        let db = self.db();

        let ast::Arguments {
            args,
            keywords,
            range: _,
            node_index: _,
        } = &call_expr.arguments;

        for keyword in keywords {
            self.infer_expression(&keyword.value, TypeContext::default());
        }

        let [name_arg, bases_arg, namespace_arg] = match &**args {
            [single] => {
                let arg_type = self.infer_expression(single, TypeContext::default());

                return if keywords.is_empty() {
                    arg_type.dunder_class(db)
                } else {
                    if keywords.iter().any(|keyword| keyword.arg.is_some())
                        && let Some(builder) =
                            self.context.report_lint(&NO_MATCHING_OVERLOAD, call_expr)
                    {
                        let mut diagnostic = builder
                            .into_diagnostic("No overload of class `type` matches arguments");
                        diagnostic.help(format_args!(
                            "`builtins.type()` expects no keyword arguments",
                        ));
                    }
                    SubclassOfType::subclass_of_unknown()
                };
            }

            [first, second] if second.is_starred_expr() => {
                self.infer_expression(first, TypeContext::default());
                self.infer_expression(second, TypeContext::default());

                match &**keywords {
                    [single] if single.arg.is_none() => {
                        return SubclassOfType::subclass_of_unknown();
                    }
                    _ => {
                        if let Some(builder) =
                            self.context.report_lint(&NO_MATCHING_OVERLOAD, call_expr)
                        {
                            let mut diagnostic = builder
                                .into_diagnostic("No overload of class `type` matches arguments");
                            diagnostic.help(format_args!(
                                "`builtins.type()` expects no keyword arguments",
                            ));
                        }

                        return SubclassOfType::subclass_of_unknown();
                    }
                }
            }

            [name, bases, namespace] => [name, bases, namespace],

            _ => {
                for arg in args {
                    self.infer_expression(arg, TypeContext::default());
                }

                if let Some(builder) = self.context.report_lint(&NO_MATCHING_OVERLOAD, call_expr) {
                    let mut diagnostic =
                        builder.into_diagnostic("No overload of class `type` matches arguments");
                    diagnostic.help(format_args!(
                        "`builtins.type()` can either be called with one or three \
                        positional arguments (got {})",
                        args.len()
                    ));
                }

                return SubclassOfType::subclass_of_unknown();
            }
        };

        let name_type = self.infer_expression(name_arg, TypeContext::default());

        let namespace_type = self.infer_expression(namespace_arg, TypeContext::default());

        // TODO: validate other keywords against `__init_subclass__` methods of superclasses
        if keywords
            .iter()
            .any(|keyword| keyword.arg.as_deref() == Some("metaclass"))
        {
            if let Some(builder) = self.context.report_lint(&NO_MATCHING_OVERLOAD, call_expr) {
                let mut diagnostic =
                    builder.into_diagnostic("No overload of class `type` matches arguments");
                diagnostic
                    .help("The `metaclass` keyword argument is not supported in `type()` calls");
            }
        }

        // If any argument is a starred expression, we can't know how many positional arguments
        // we're receiving, so fall back to `type[Unknown]` to avoid false-positive errors.
        if args.iter().any(ast::Expr::is_starred_expr) {
            return SubclassOfType::subclass_of_unknown();
        }

        // Extract members from the namespace dict (third argument).
        let (members, has_dynamic_namespace): (Box<[DynamicClassMember<'db>]>, bool) =
            if let ast::Expr::Dict(dict) = namespace_arg {
                // Check if all keys are string literal types. If any key is not a string literal
                // type or is missing (spread), the namespace is considered dynamic.
                let all_keys_are_string_literals = dict.items.iter().all(|item| {
                    item.key
                        .as_ref()
                        .is_some_and(|k| self.expression_type(k).is_string_literal())
                });
                let members = dict
                    .items
                    .iter()
                    .filter_map(|item| {
                        // Only extract items with string literal keys.
                        let key_expr = item.key.as_ref()?;
                        let key_name = self.expression_type(key_expr).as_string_literal()?;
                        let key_name = ast::name::Name::new(key_name.value(db));
                        // Get the already-inferred type from when we inferred the dict above.
                        let value_ty = self.expression_type(&item.value);
                        Some(DynamicClassMember {
                            name: key_name,
                            ty: value_ty,
                            range: item.value.range(),
                        })
                    })
                    .collect();
                (members, !all_keys_are_string_literals)
            } else if let Type::TypedDict(typed_dict) = namespace_type {
                // `namespace` is a TypedDict instance. Extract known keys as members.
                // TypedDicts are "open" (can have additional string keys), so this
                // is still a dynamic namespace for unknown attributes.
                let members: Box<[DynamicClassMember<'db>]> = typed_dict
                    .items(db)
                    .iter()
                    .map(|(name, field)| DynamicClassMember {
                        name: name.clone(),
                        ty: field.declared_ty,
                        range: namespace_arg.range(),
                    })
                    .collect();
                (members, true)
            } else {
                // `namespace` is not a dict literal, so it's dynamic.
                (Box::new([]), true)
            };

        let (override_members, _has_dynamic_override_namespace) =
            self.extract_dynamic_override_members(namespace_arg, namespace_type);

        if !matches!(namespace_type, Type::TypedDict(_))
            && !namespace_type.is_assignable_to(
                db,
                KnownClass::Dict
                    .to_specialized_instance(db, &[KnownClass::Str.to_instance(db), Type::any()]),
            )
            && let Some(builder) = self
                .context
                .report_lint(&INVALID_ARGUMENT_TYPE, namespace_arg)
        {
            let mut diagnostic = builder
                .into_diagnostic("Invalid argument to parameter 3 (`namespace`) of `type()`");
            diagnostic.set_primary_message(format_args!(
                "Expected `dict[str, Any]`, found `{}`",
                namespace_type.display(db)
            ));
        }

        // Extract name and base classes.
        let name = if let Some(literal) = name_type.as_string_literal() {
            Name::new(literal.value(db))
        } else {
            if !name_type.is_assignable_to(db, KnownClass::Str.to_instance(db))
                && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, name_arg)
            {
                let mut diagnostic =
                    builder.into_diagnostic("Invalid argument to parameter 1 (`name`) of `type()`");
                diagnostic.set_primary_message(format_args!(
                    "Expected `str`, found `{}`",
                    name_type.display(db)
                ));
            }
            Name::new_static("<unknown>")
        };

        let scope = self.scope();

        // For assigned `type()` calls, bases inference is deferred to handle forward references
        // and recursive references (e.g., `X = type("X", (tuple["X | None"],), {})`).
        // This avoids expensive Salsa fixpoint iteration by deferring inference until the
        // class type is already bound. For dangling calls, infer and extract bases eagerly
        // (they'll be stored in the anchor and used for validation).
        let explicit_bases = if definition.is_none() {
            let bases_type = self.infer_expression(bases_arg, TypeContext::default());
            self.extract_explicit_bases(bases_arg, bases_type, DynamicClassKind::TypeCall)
        } else {
            None
        };

        // Create the anchor for identifying this dynamic class.
        // - For assigned `type()` calls, the Definition uniquely identifies the class,
        //   and bases inference is deferred.
        // - For dangling calls, compute a relative offset from the scope's node index,
        //   and store the explicit bases directly (since they were inferred eagerly).
        let anchor = if let Some(def) = definition {
            // Register for deferred inference to infer bases and validate later.
            self.deferred.insert(def);
            DynamicClassAnchor::Definition(def)
        } else {
            let call_node_index = call_expr.node_index().load();
            let scope_anchor = scope.node(db).node_index().unwrap_or(NodeIndex::from(0));
            let anchor_u32 = scope_anchor
                .as_u32()
                .expect("scope anchor should not be NodeIndex::NONE");
            let call_u32 = call_node_index
                .as_u32()
                .expect("call node should not be NodeIndex::NONE");

            // Use [Unknown] as fallback if bases extraction failed (e.g., not a tuple).
            let anchor_bases = explicit_bases
                .clone()
                .unwrap_or_else(|| Box::from([Type::unknown()]));

            DynamicClassAnchor::ScopeOffset {
                scope,
                offset: call_u32 - anchor_u32,
                explicit_bases: anchor_bases,
            }
        };

        let dynamic_class = DynamicClassLiteral::new(
            db,
            name.clone(),
            anchor,
            members,
            override_members,
            has_dynamic_namespace,
            None,
        );

        // For dangling calls, validate bases eagerly. For assigned calls, validation is
        // deferred along with bases inference.
        if let Some(explicit_bases) = &explicit_bases {
            // Validate bases and collect disjoint bases for diagnostics.
            let mut disjoint_bases = self.validate_dynamic_type_bases(
                bases_arg,
                explicit_bases,
                &name,
                DynamicClassKind::TypeCall,
            );

            // Check for MRO errors.
            if report_dynamic_mro_errors(&self.context, dynamic_class, call_expr, bases_arg) {
                // MRO succeeded, check for instance-layout-conflict.
                disjoint_bases.remove_redundant_entries(db);
                if disjoint_bases.len() > 1 {
                    report_instance_layout_conflict(
                        &self.context,
                        dynamic_class.header_range(db),
                        bases_arg.as_tuple_expr().map(|tuple| tuple.elts.as_slice()),
                        &disjoint_bases,
                    );
                }
            }

            // Check for metaclass conflicts.
            if let Err(DynamicMetaclassConflict {
                metaclass1,
                base1,
                metaclass2,
                base2,
            }) = dynamic_class.try_metaclass(db)
            {
                report_conflicting_metaclass_from_bases(
                    &self.context,
                    call_expr.into(),
                    dynamic_class.name(db),
                    metaclass1,
                    base1.display(db),
                    metaclass2,
                    base2.display(db),
                );
            }

            overrides::check_dynamic_class(&self.context, dynamic_class);
        }

        Type::ClassLiteral(ClassLiteral::Dynamic(dynamic_class))
    }

    /// Deferred inference for assigned `type()` calls.
    ///
    /// Infers the bases argument that was skipped during initial inference to handle
    /// forward references and recursive definitions.
    pub(super) fn infer_builtins_type_deferred(
        &mut self,
        definition: Definition<'db>,
        call_expr: &ast::Expr,
    ) {
        let db = self.db();

        let ast::Expr::Call(call) = call_expr else {
            return;
        };

        // Get the already-inferred class type from the initial pass.
        let inferred_type = definition_expression_type(db, definition, call_expr);
        let Type::ClassLiteral(ClassLiteral::Dynamic(dynamic_class)) = inferred_type else {
            return;
        };

        let [_name_arg, bases_arg, _namespace_arg] = &*call.arguments.args else {
            return;
        };

        // Set the typevar binding context to allow legacy typevar binding in expressions
        // like `Generic[T]`. This matches the context used during initial inference.
        let previous_context = self.typevar_binding_context.replace(definition);

        // Infer the bases argument (this was skipped during initial inference).
        let bases_type = self.infer_expression(bases_arg, TypeContext::default());

        // Restore the previous context.
        self.typevar_binding_context = previous_context;

        // Extract and validate bases.
        let Some(bases) =
            self.extract_explicit_bases(bases_arg, bases_type, DynamicClassKind::TypeCall)
        else {
            return;
        };

        // Validate individual bases for special types that aren't allowed in dynamic classes.
        let name = dynamic_class.name(db);
        self.validate_dynamic_type_bases(bases_arg, &bases, name, DynamicClassKind::TypeCall);
    }
}
