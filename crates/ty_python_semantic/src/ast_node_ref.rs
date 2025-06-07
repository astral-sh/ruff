use std::sync::Arc;

use ruff_db::parsed::ParsedModuleRef;

/// Ref-counted owned reference to an AST node.
///
/// The type holds an owned reference to the node's ref-counted [`ParsedModuleRef`].
/// Holding on to the node's [`ParsedModuleRef`] guarantees that the reference to the
/// node must still be valid.
///
/// Holding on to any [`AstNodeRef`] prevents the [`ParsedModuleRef`] from being released.
///
/// ## Equality
/// Two `AstNodeRef` are considered equal if their pointer addresses are equal.
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
    /// Owned reference to the node's [`ParsedModuleRef`].
    ///
    /// The node's reference is guaranteed to remain valid as long as it's enclosing
    /// [`ParsedModuleRef`] is alive.
    parsed: ParsedModuleRef,

    /// Pointer to the referenced node.
    node: std::ptr::NonNull<T>,
}

#[expect(unsafe_code)]
impl<T> AstNodeRef<T> {
    /// Creates a new `AstNodeRef` that references `node`. The `parsed` is the [`ParsedModuleRef`] to
    /// which the `AstNodeRef` belongs.
    ///
    /// ## Safety
    ///
    /// Dereferencing the `node` can result in undefined behavior if `parsed` isn't the
    /// [`ParsedModuleRef`] to which `node` belongs. It's the caller's responsibility to ensure that
    /// the invariant `node belongs to parsed` is upheld.
    pub(super) unsafe fn new(parsed: ParsedModuleRef, node: &T) -> Self {
        Self {
            parsed,
            node: std::ptr::NonNull::from(node),
        }
    }

    /// Returns a reference to the wrapped node.
    ///
    /// Note that this method will panic if the provided module is from a different file or Salsa revision
    /// than the module this node was created with.
    pub fn node<'ast>(&self, parsed: &'ast ParsedModuleRef) -> &'ast T {
        debug_assert!(Arc::ptr_eq(self.parsed.as_arc(), parsed.as_arc()));

        // SAFETY: Holding on to `parsed` ensures that the AST to which `node` belongs is still
        // alive and not moved.
        unsafe { self.node.as_ref() }
    }
}

impl<T> std::fmt::Debug for AstNodeRef<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AstNodeRef")
            .field(self.node(&self.parsed))
            .finish()
    }
}

#[expect(unsafe_code)]
unsafe impl<T> salsa::Update for AstNodeRef<T> {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        let old_ref = unsafe { &mut (*old_pointer) };

        if Arc::ptr_eq(old_ref.parsed.as_arc(), new_value.parsed.as_arc())
            && old_ref.node.eq(&new_value.node)
        {
            false
        } else {
            *old_ref = new_value;
            true
        }
    }
}

#[expect(unsafe_code)]
unsafe impl<T> Send for AstNodeRef<T> where T: Send {}
#[expect(unsafe_code)]
unsafe impl<T> Sync for AstNodeRef<T> where T: Sync {}
