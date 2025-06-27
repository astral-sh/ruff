use std::fmt::Debug;
use std::marker::PhantomData;

use ruff_db::parsed::ParsedModuleRef;
use ruff_python_ast::{AnyNodeRef, NodeIndex};
use ruff_python_ast::{AnyRootNodeRef, HasNodeIndex};
use ruff_text_size::Ranged;

/// Reference to an AST node.
///
/// This type acts as a reference to an AST node within a given module that remains
/// stable regardless of whether the AST is garbage collected. As such, accessing a
/// node through the [`AstNodeRef`] requires a reference to the current [`ParsedModuleRef`]
/// for the module containing the node.
///
/// ## Usage in salsa tracked structs
/// It's important that [`AstNodeRef`] fields in salsa tracked structs are tracked fields
/// (attributed with `#[tracked`]). It prevents that the tracked struct gets a new ID
/// every time the AST changes, which in turn, invalidates the result of any query
/// that takes said tracked struct as a query argument or returns the tracked struct as part of its result.
///
/// For example, marking the [`AstNodeRef`] as tracked on `Expression`
/// has the effect that salsa will consider the expression as "unchanged" for as long as it:
///
/// * belongs to the same file
/// * belongs to the same scope
/// * has the same kind
/// * was created in the same order
///
/// This means that changes to expressions in other scopes don't invalidate the expression's id, giving
/// us some form of scope-stable identity for expressions. Only queries accessing the node field
/// run on every AST change. All other queries only run when the expression's identity changes.
#[derive(Clone)]
pub struct AstNodeRef<T> {
    /// A pointer to the [`ruff_db::parsed::ParsedModule`] that this node was created from.
    module_ptr: *const (),

    /// Debug information.
    #[cfg(debug_assertions)]
    kind: ruff_python_ast::NodeKind,
    #[cfg(debug_assertions)]
    range: ruff_text_size::TextRange,

    /// The index of the node in the AST.
    index: NodeIndex,

    _node: PhantomData<T>,
}

impl<T> AstNodeRef<T>
where
    T: HasNodeIndex + Ranged + PartialEq + Debug,
    for<'ast> AnyNodeRef<'ast>: From<&'ast T>,
    for<'ast> &'ast T: TryFrom<AnyRootNodeRef<'ast>>,
{
    /// Creates a new `AstNodeRef` that references `node`.
    ///
    /// This method may panic or produce unspecified results if the provided module is from a
    /// different file or Salsa revision than the module to which the node belongs.
    pub(super) fn new(module_ref: &ParsedModuleRef, node: &T) -> Self {
        let index = node.node_index().load();
        debug_assert_eq!(module_ref.get_by_index(index).try_into().ok(), Some(node));

        Self {
            index,
            module_ptr: module_ref.module().as_ptr(),
            #[cfg(debug_assertions)]
            kind: AnyNodeRef::from(node).kind(),
            #[cfg(debug_assertions)]
            range: node.range(),
            _node: PhantomData,
        }
    }

    /// Returns a reference to the wrapped node.
    ///
    /// This method may panic or produce unspecified results if the provided module is from a
    /// different file or Salsa revision than the module to which the node belongs.
    pub fn node<'ast>(&self, module_ref: &'ast ParsedModuleRef) -> &'ast T {
        debug_assert_eq!(module_ref.module().as_ptr(), self.module_ptr);

        // Note that the module pointer is guaranteed to be stable within the Salsa
        // revision, so the file contents cannot have changed by the above assertion.
        module_ref
            .get_by_index(self.index)
            .try_into()
            .ok()
            .expect("AST indices should never change within the same revision")
    }
}

impl<T> get_size2::GetSize for AstNodeRef<T> {}

#[allow(clippy::missing_fields_in_debug)]
impl<T> Debug for AstNodeRef<T>
where
    T: Debug,
    for<'ast> &'ast T: TryFrom<AnyRootNodeRef<'ast>>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(debug_assertions)]
        {
            f.debug_struct("AstNodeRef")
                .field("kind", &self.kind)
                .field("range", &self.range)
                .finish()
        }

        #[cfg(not(debug_assertions))]
        {
            // Unfortunately we have no access to the AST here.
            f.debug_tuple("AstNodeRef").finish_non_exhaustive()
        }
    }
}

#[expect(unsafe_code)]
unsafe impl<T> salsa::Update for AstNodeRef<T> {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        let old_ref = unsafe { &mut (*old_pointer) };

        // Two nodes are guaranteed to be equal as long as they refer to the same node index
        // within the same module. Note that the module pointer is guaranteed to be stable
        // within the Salsa revision, so the file contents cannot have changed.
        if old_ref.module_ptr == new_value.module_ptr && old_ref.index == new_value.index {
            false
        } else {
            *old_ref = new_value;
            true
        }
    }
}

// SAFETY: The `module_ptr` is only used for pointer equality and never accessed directly.
#[expect(unsafe_code)]
unsafe impl<T> Send for AstNodeRef<T> where T: Send {}
#[expect(unsafe_code)]
unsafe impl<T> Sync for AstNodeRef<T> where T: Sync {}
