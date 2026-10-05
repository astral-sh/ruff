//! Constructs binding-tree inputs and inspects entry identities for controlled matching tests.

use anyhow::Context;
use smallvec::{SmallVec, smallvec};

use super::super::constructor::ConstructorBinding;
use super::super::{
    BindingError, Bindings, BindingsElement, CallableBinding, CallableItem,
    PropertyAccessorCallError,
};
use crate::types::Type;
use crate::types::signatures::Signature;

/// Selects a constructed binding-tree shape independently of Python source inference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum TreeShape {
    Deep(usize),
    Wide(usize),
    Union,
    Intersection,
    PropertyErrors,
}

/// Clones the supplied shallow constructor and regular entries into a selected ownership shape.
/// Each entry receives a distinct callable marker and source-overload index. Signatures, bound
/// receivers and constructed instances retain the seed values, so tests choose their semantics.
pub(in crate::types) fn build_tree<'db>(
    constructor_seed: &Bindings<'db>,
    regular_seed: &Bindings<'db>,
    shape: TreeShape,
) -> anyhow::Result<Bindings<'db>> {
    let CallableItem::Constructor(constructor) = single_item(constructor_seed)? else {
        anyhow::bail!("the constructor seed must contain one constructor entry");
    };
    let CallableItem::Regular(regular) = single_item(regular_seed)? else {
        anyhow::bail!("the regular seed must contain one regular entry");
    };
    anyhow::ensure!(
        constructor.downstream_constructor.is_none(),
        "the constructor seed must not have a downstream child"
    );
    anyhow::ensure!(
        !constructor.entry.overloads.is_empty() && !regular.overloads.is_empty(),
        "both seeds must contain an overload"
    );
    let mut builder = FixtureBuilder {
        constructor,
        regular,
        next_id: 1,
    };
    match shape {
        TreeShape::Deep(depth) => builder.deep(depth),
        TreeShape::Wide(width) => builder.wide(width),
        TreeShape::Union => {
            let mut elements = SmallVec::new();
            for _ in 0..3 {
                let child = builder.deep(1)?;
                let constructor = builder.constructor(Some(child))?;
                let regular = builder.regular()?;
                elements.push(element(smallvec![constructor, regular]));
            }
            Ok(node(elements))
        }
        TreeShape::Intersection => {
            let first_child = builder.deep(2)?;
            let first = builder.constructor(Some(first_child))?;
            let regular = builder.regular()?;
            let last_child = builder.wide(2)?;
            let last = builder.constructor(Some(last_child))?;
            Ok(node(smallvec![element(smallvec![first, regular, last])]))
        }
        TreeShape::PropertyErrors => {
            let mut tree = builder.deep(3)?;
            let mut getter = builder.deep(4)?;
            let nested_setter = builder.deep(2)?;
            attach_property_error(&mut getter, nested_setter, PropertyEdge::Setter)?;
            let setter = builder.wide(3)?;
            attach_property_error(&mut tree, getter, PropertyEdge::Getter)?;
            attach_property_error(&mut tree, setter, PropertyEdge::Setter)?;
            Ok(tree)
        }
    }
}

/// Validates the single-item seed shape before a fixture builder borrows its entry.
fn single_item<'bindings, 'db>(
    bindings: &'bindings Bindings<'db>,
) -> anyhow::Result<&'bindings CallableItem<'db>> {
    let [element] = bindings.elements.as_slice() else {
        anyhow::bail!("a fixture seed must contain exactly one union element");
    };
    let [item] = element.items.as_slice() else {
        anyhow::bail!("a fixture seed must contain exactly one intersection item");
    };
    Ok(item)
}

/// Reuses shallow semantic seeds while assigning unique scalar markers to constructed entries.
#[derive(Debug)]
struct FixtureBuilder<'seed, 'db> {
    constructor: &'seed ConstructorBinding<'db>,
    regular: &'seed CallableBinding<'db>,
    next_id: usize,
}

impl<'db> FixtureBuilder<'_, 'db> {
    /// Distinguishes both callable entries and their overloads in complete-state comparisons.
    fn mark(&mut self, callable: &mut CallableBinding<'db>) -> anyhow::Result<()> {
        let marker = Type::int_literal(i64::try_from(self.next_id)?);
        callable.callable_type = marker;
        callable.signature_type = marker;
        for overload in &mut callable.overloads {
            overload.callable_type = marker;
            overload.signature_type = marker;
            overload.source_overload_index = self.next_id;
            self.next_id = self
                .next_id
                .checked_add(1)
                .context("fixture marker overflow")?;
        }
        Ok(())
    }

    /// Copies one constructor entry and installs its independently owned downstream tree.
    fn constructor(&mut self, child: Option<Bindings<'db>>) -> anyhow::Result<CallableItem<'db>> {
        let mut constructor = ConstructorBinding::clone(self.constructor);
        self.mark(&mut constructor.entry)?;
        constructor.downstream_constructor = child.map(Box::new);
        Ok(CallableItem::Constructor(constructor))
    }

    /// Copies one regular entry with a marker distinct from all constructor entries.
    fn regular(&mut self) -> anyhow::Result<CallableItem<'db>> {
        let mut regular = CallableBinding::clone(self.regular);
        self.mark(&mut regular)?;
        Ok(CallableItem::Regular(regular))
    }

    /// Builds a constructor chain whose downstream intersections contain regular siblings.
    fn deep(&mut self, depth: usize) -> anyhow::Result<Bindings<'db>> {
        let leaf = self.constructor(None)?;
        let mut tree = node(smallvec![element(smallvec![leaf])]);
        for _ in 0..depth {
            let before = self.regular()?;
            let after = self.regular()?;
            let mut items = smallvec![before];
            for element in std::mem::take(&mut tree.elements) {
                items.extend(element.items);
            }
            items.push(after);
            let child = node(smallvec![element(items)]);
            let parent = self.constructor(Some(child))?;
            tree = node(smallvec![element(smallvec![parent])]);
        }
        Ok(tree)
    }

    /// Builds a constructor with wide downstream union elements and mixed intersection entries.
    fn wide(&mut self, width: usize) -> anyhow::Result<Bindings<'db>> {
        let mut elements = SmallVec::new();
        for _ in 0..width {
            let tail = self.regular()?;
            let child = node(smallvec![element(smallvec![tail])]);
            let before = self.regular()?;
            let constructor = self.constructor(Some(child))?;
            let after = self.regular()?;
            elements.push(element(smallvec![before, constructor, after]));
        }
        let root = self.constructor(Some(node(elements)))?;
        Ok(node(smallvec![element(smallvec![root])]))
    }
}

/// Builds an intersection element whose type marker identifies its first callable.
fn element<'db>(items: SmallVec<[CallableItem<'db>; 1]>) -> BindingsElement<'db> {
    let callable_type = items
        .first()
        .map(|item| item.callable().callable_type)
        .unwrap_or(Type::unknown());
    BindingsElement {
        callable_type,
        items,
    }
}

/// Owns the supplied union elements without introducing lexical freshening hints.
fn node<'db>(elements: SmallVec<[BindingsElement<'db>; 1]>) -> Bindings<'db> {
    let callable_type = elements
        .first()
        .map(|element| element.callable_type)
        .unwrap_or(Type::unknown());
    Bindings {
        callable_type,
        implicit_dunder_new_is_possibly_unbound: false,
        implicit_dunder_init_is_possibly_unbound: false,
        elements,
        enclosing_binding_contexts: None,
        cleanup_next: None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PropertyEdge {
    Getter,
    Setter,
}

/// Adds an owned property-error child to the first overload without making it a matching edge.
fn attach_property_error<'db>(
    bindings: &mut Bindings<'db>,
    child: Bindings<'db>,
    edge: PropertyEdge,
) -> anyhow::Result<()> {
    let item = bindings
        .elements
        .first_mut()
        .and_then(|element| element.items.first_mut())
        .context("a property-error fixture needs an entry")?;
    let overload = item
        .callable_mut()
        .overloads
        .first_mut()
        .context("a property-error fixture needs an overload")?;
    let error = PropertyAccessorCallError {
        bindings: Box::new(child),
        argument_index_offset: 7,
    };
    overload.errors.push(match edge {
        PropertyEdge::Getter => BindingError::PropertyGetterCallError(error),
        PropertyEdge::Setter => BindingError::PropertySetterCallError(error),
    });
    Ok(())
}

/// Lists entry addresses in matching order, excluding all property-error cleanup children.
/// The result is meaningful while the inspected tree stays at the same addresses.
pub(in crate::types) fn matching_entry_order(bindings: &Bindings<'_>) -> Vec<usize> {
    let mut pending: Vec<_> = bindings
        .elements
        .iter()
        .rev()
        .flat_map(|element| element.items.iter().rev())
        .collect();
    let mut order = Vec::new();
    while let Some(item) = pending.pop() {
        order.push(std::ptr::from_ref(item.callable()).addr());
        if let CallableItem::Constructor(constructor) = item
            && let Some(downstream) = &constructor.downstream_constructor
        {
            pending.extend(
                downstream
                    .elements
                    .iter()
                    .rev()
                    .flat_map(|element| element.items.iter().rev()),
            );
        }
    }
    order
}

/// Copies constructor state for comparisons before and after a controlled matching attempt.
/// `root` identifies the top-level constructor; its descendants retain that ordinal.
#[derive(Debug)]
pub(in crate::types) struct ConstructorSnapshot<'db> {
    pub(in crate::types) root: usize,
    pub(in crate::types) depth: usize,
    pub(in crate::types) stored_instance: Type<'db>,
    pub(in crate::types) receiver: Option<Type<'db>>,
    pub(in crate::types) signatures: Vec<Signature<'db>>,
    pub(in crate::types) overload_instances: Vec<Option<Type<'db>>>,
}

/// Captures constructors in matching order, excluding regular entries and property-error children.
/// The copied signatures let tests compare semantic state after the original tree has been released.
pub(in crate::types) fn constructor_snapshots<'db>(
    bindings: &Bindings<'db>,
) -> Vec<ConstructorSnapshot<'db>> {
    let mut pending: Vec<_> = bindings
        .elements
        .iter()
        .flat_map(|element| &element.items)
        .filter_map(|item| match item {
            CallableItem::Constructor(constructor) => Some(constructor),
            CallableItem::Regular(_) => None,
        })
        .enumerate()
        .map(|(root, constructor)| (constructor, root, 0usize))
        .collect();
    pending.reverse();
    let mut snapshots = Vec::new();
    while let Some((constructor, root, depth)) = pending.pop() {
        snapshots.push(ConstructorSnapshot {
            root,
            depth,
            stored_instance: constructor.constructed_instance_type(),
            receiver: constructor.entry.bound_type,
            signatures: constructor
                .entry
                .overloads
                .iter()
                .map(|binding| binding.signature.clone())
                .collect(),
            overload_instances: constructor
                .entry
                .overloads
                .iter()
                .map(|binding| {
                    binding
                        .constructor_context
                        .map(|context| context.instance_type())
                })
                .collect(),
        });
        if let Some(downstream) = &constructor.downstream_constructor {
            pending.extend(downstream.elements.iter().rev().flat_map(|element| {
                element.items.iter().rev().filter_map(|item| match item {
                    CallableItem::Constructor(child) => Some((child, root, depth + 1)),
                    CallableItem::Regular(_) => None,
                })
            }));
        }
    }
    snapshots
}

/// Lists every entry owned below a getter or setter error, including nested downstream trees.
/// Matching must leave these addresses absent from its entry observations.
pub(in crate::types) fn property_entry_ids(bindings: &Bindings<'_>) -> Vec<usize> {
    let mut pending = vec![(bindings, None)];
    let mut entries = Vec::new();
    while let Some((bindings, property_edge)) = pending.pop() {
        for element in &bindings.elements {
            for item in &element.items {
                if property_edge.is_some() {
                    entries.push(std::ptr::from_ref(item.callable()).addr());
                }
                if let CallableItem::Constructor(constructor) = item
                    && let Some(downstream) = &constructor.downstream_constructor
                {
                    pending.push((downstream, property_edge));
                }
                for overload in &item.callable().overloads {
                    for error in &overload.errors {
                        match error {
                            BindingError::PropertyGetterCallError(error) => {
                                pending.push((&error.bindings, Some(PropertyEdge::Getter)));
                            }
                            BindingError::PropertySetterCallError(error) => {
                                pending.push((&error.bindings, Some(PropertyEdge::Setter)));
                            }
                            BindingError::InvalidArgumentType { .. }
                            | BindingError::InvalidKeyType { .. }
                            | BindingError::MissingArguments { .. }
                            | BindingError::UnknownArgument { .. }
                            | BindingError::UnknownKeywordVariadicArgument { .. }
                            | BindingError::PositionalOnlyParameterAsKwarg { .. }
                            | BindingError::TooManyPositionalArguments { .. }
                            | BindingError::ParameterAlreadyAssigned { .. }
                            | BindingError::SpecializationError { .. }
                            | BindingError::PropertyHasNoGetter(_)
                            | BindingError::PropertyHasNoSetter(_)
                            | BindingError::PropertyHasNoDeleter(_)
                            | BindingError::InternalCallError(_)
                            | BindingError::UnmatchedOverload
                            | BindingError::CalledTopCallable(_)
                            | BindingError::InvalidDataclassApplication(_)
                            | BindingError::InvalidDataclassArgument(_) => {}
                        }
                    }
                }
            }
        }
    }
    entries
}
