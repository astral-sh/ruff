use std::hash::Hash;
use std::ops::Deref;

use ruff_db::parsed::ParsedModule;
use ruff_python_ast as ast;

/// Ref-counted owned reference to an AST node.
///
/// The type holds an owned reference to the node's ref-counted [`ParsedModule`].
/// Holding on to the node's [`ParsedModule`] guarantees that the reference to the
/// node must still be valid.
///
/// Holding on to any [`AstNodeRef`] prevents the [`ParsedModule`] from being released.
///
/// ## Equality
/// Two `AstNodeRef` are considered equal if their wrapped nodes are equal.
#[derive(Clone)]
pub struct AstNodeRef<T> {
    /// Owned reference to the node's [`ParsedModule`].
    ///
    /// The node's reference is guaranteed to remain valid as long as it's enclosing
    /// [`ParsedModule`] is alive.
    parsed: ParsedModule,

    /// Pointer to the referenced node.
    node: std::ptr::NonNull<T>,
}

#[allow(unsafe_code)]
impl<T> AstNodeRef<T> {
    /// Creates a new `AstNodeRef` that reference `node`. The `parsed` is the [`ParsedModule`] to which
    /// the `AstNodeRef` belongs.
    ///
    /// ## Safety
    /// Dereferencing the `node` can result in undefined behavior if `parsed` isn't the [`ParsedModule`] to
    /// which `node` belongs. It's the caller's responsibility to ensure that the invariant `node belongs to parsed` is upheld.

    pub(super) unsafe fn new(parsed: ParsedModule, node: &T) -> Self {
        Self {
            parsed,
            node: std::ptr::NonNull::from(node),
        }
    }

    /// Returns a reference to the wrapped node.
    pub fn node(&self) -> &T {
        // SAFETY: Holding on to `parsed` ensures that the AST to which `node` belongs is still alive
        // and not moved.
        unsafe { self.node.as_ref() }
    }
}

impl<T> Deref for AstNodeRef<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.node()
    }
}

#[allow(unsafe_code)]
impl AstNodeRef<ast::Stmt> {
    #[inline]
    pub fn to_class_def(&self) -> Option<AstNodeRef<ast::StmtClassDef>> {
        self.node().as_class_def_stmt().map(|class_def| unsafe {
            // SAFETY: Casting `node` to a subtype doesn't change the fact that it's a child of `parsed`.
            AstNodeRef::new(self.parsed.clone(), class_def)
        })
    }

    #[inline]
    pub fn to_function_def(&self) -> Option<AstNodeRef<ast::StmtFunctionDef>> {
        self.node()
            .as_function_def_stmt()
            .map(|function_def| unsafe {
                // SAFETY: Casting `node` to a subtype doesn't change the fact that it's a child of `parsed`.
                AstNodeRef::new(self.parsed.clone(), function_def)
            })
    }

    #[inline]
    pub fn to_assign(&self) -> Option<AstNodeRef<ast::StmtAssign>> {
        self.node().as_assign_stmt().map(|assign| unsafe {
            // SAFETY: Casting `node` to a subtype doesn't change the fact that it's a child of `parsed`.
            AstNodeRef::new(self.parsed.clone(), assign)
        })
    }

    #[inline]
    pub fn to_ann_assign(&self) -> Option<AstNodeRef<ast::StmtAnnAssign>> {
        self.node().as_ann_assign_stmt().map(|assign| unsafe {
            // SAFETY: Casting `node` to a subtype doesn't change the fact that it's a child of `parsed`.
            AstNodeRef::new(self.parsed.clone(), assign)
        })
    }

    #[inline]
    pub fn to_import(&self) -> Option<AstNodeRef<ast::StmtImport>> {
        self.node().as_import_stmt().map(|import| unsafe {
            // SAFETY: Casting `node` to a subtype doesn't change the fact that it's a child of `parsed`.
            AstNodeRef::new(self.parsed.clone(), import)
        })
    }

    #[inline]
    pub fn to_import_from(&self) -> Option<AstNodeRef<ast::StmtImportFrom>> {
        self.node().as_import_from_stmt().map(|import| unsafe {
            // SAFETY: Casting `node` to a subtype doesn't change the fact that it's a child of `parsed`.
            AstNodeRef::new(self.parsed.clone(), import)
        })
    }
}

impl<T> std::fmt::Debug for AstNodeRef<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AstNodeRef").field(&self.node()).finish()
    }
}

impl<T> PartialEq for AstNodeRef<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.node().eq(other.node())
    }
}

impl<T> Eq for AstNodeRef<T> where T: Eq {}

impl<T> Hash for AstNodeRef<T>
where
    T: Hash,
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.node().hash(state)
    }
}

#[allow(unsafe_code)]
unsafe impl<T> Send for AstNodeRef<T> where T: Send {}
#[allow(unsafe_code)]
unsafe impl<T> Sync for AstNodeRef<T> where T: Sync {}
