use std::{
    any::{Any, TypeId},
    marker::PhantomData,
    mem,
    ptr::NonNull,
};

use crate::{Database, database::RawDatabase};

/// A `Views` struct is associated with some specific database type
/// (a `DatabaseImpl<U>` for some existential `U`). It contains functions
/// to downcast to `dyn DbView` for various traits `DbView` via this specific
/// database type.
/// None of these types are known at compilation time, they are all checked
/// dynamically through `TypeId` magic.
pub struct Views {
    source_type_id: TypeId,
    view_casters: boxcar::Vec<ViewCaster>,
}

#[derive(Copy, Clone)]
struct ViewCaster {
    /// The id of the target type `dyn DbView` that we can cast to.
    target_type_id: TypeId,

    /// The name of the target type `dyn DbView` that we can cast to.
    type_name: &'static str,

    /// Type-erased function pointer that downcasts to `dyn DbView`.
    cast: ErasedDatabaseDownCasterSig,
}

impl ViewCaster {
    fn new<DbView: ?Sized + Any>(func: DatabaseDownCasterSig<DbView>) -> ViewCaster {
        ViewCaster {
            target_type_id: TypeId::of::<DbView>(),
            type_name: std::any::type_name::<DbView>(),
            // SAFETY: We are type erasing for storage, taking care of unerasing before we call
            // the function pointer.
            cast: unsafe {
                mem::transmute::<DatabaseDownCasterSig<DbView>, ErasedDatabaseDownCasterSig>(func)
            },
        }
    }
}

type ErasedDatabaseDownCasterSig = unsafe fn(RawDatabase<'_>) -> NonNull<()>;
type DatabaseDownCasterSig<DbView> = unsafe fn(RawDatabase<'_>) -> NonNull<DbView>;

#[repr(transparent)]
pub struct DatabaseDownCaster<DbView: ?Sized>(ViewCaster, PhantomData<fn() -> DbView>);

impl<DbView: ?Sized> Copy for DatabaseDownCaster<DbView> {}
impl<DbView: ?Sized> Clone for DatabaseDownCaster<DbView> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<DbView: ?Sized + Any> DatabaseDownCaster<DbView> {
    /// Downcast `db` to `DbView`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `db` is of the correct type.
    #[inline]
    pub unsafe fn downcast_unchecked<'db>(&self, db: RawDatabase<'db>) -> &'db DbView {
        // SAFETY: The caller must ensure that `db` is of the correct type.
        // The returned pointer is live for `'db` due to construction of the downcaster functions.
        unsafe { (self.unerased_downcaster())(db).as_ref() }
    }
    /// Downcast `db` to `DbView`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `db` is of the correct type.
    #[inline]
    pub unsafe fn downcast_mut_unchecked<'db>(&self, db: RawDatabase<'db>) -> &'db mut DbView {
        // SAFETY: The caller must ensure that `db` is of the correct type.
        // The returned pointer is live for `'db` due to construction of the downcaster functions.
        unsafe { (self.unerased_downcaster())(db).as_mut() }
    }

    #[inline]
    fn unerased_downcaster(&self) -> DatabaseDownCasterSig<DbView> {
        // SAFETY: The type-erased function pointer is guaranteed to be ABI compatible for `DbView`
        unsafe {
            mem::transmute::<ErasedDatabaseDownCasterSig, DatabaseDownCasterSig<DbView>>(
                self.0.cast,
            )
        }
    }
}

impl Views {
    pub(crate) fn new<Db: Database>() -> Self {
        let source_type_id = TypeId::of::<Db>();
        let view_casters = boxcar::Vec::new();
        view_casters.push(ViewCaster::new::<dyn Database>(|db| db.ptr.cast::<Db>()));
        Self {
            source_type_id,
            view_casters,
        }
    }

    /// Add a new downcaster to `dyn DbView`.
    pub fn add<Concrete: 'static, DbView: ?Sized + Any>(
        &self,
        func: fn(NonNull<Concrete>) -> NonNull<DbView>,
    ) -> &DatabaseDownCaster<DbView> {
        assert_eq!(
            self.source_type_id,
            TypeId::of::<Concrete>(),
            "mismatched source type"
        );
        let target_type_id = TypeId::of::<DbView>();
        if let Some((_, caster)) = self
            .view_casters
            .iter()
            .find(|(_, u)| u.target_type_id == target_type_id)
        {
            // SAFETY: The type-erased function pointer is guaranteed to be valid for `DbView`
            return unsafe { &*(&raw const *caster).cast::<DatabaseDownCaster<DbView>>() };
        }

        // SAFETY: We are type erasing the function pointer for storage, and we will unerase it
        // before we call it.
        let caster = unsafe {
            mem::transmute::<fn(NonNull<Concrete>) -> NonNull<DbView>, DatabaseDownCasterSig<DbView>>(
                func,
            )
        };
        let caster = ViewCaster::new::<DbView>(caster);
        let idx = self.view_casters.push(caster);
        // SAFETY: The type-erased function pointer is guaranteed to be valid for `DbView`
        unsafe { &*(&raw const self.view_casters[idx]).cast::<DatabaseDownCaster<DbView>>() }
    }

    /// Retrieve an downcaster function to `dyn DbView`.
    ///
    /// # Panics
    ///
    /// If the underlying type of `db` is not the same as the database type this downcasts was created for.
    pub fn downcaster_for<DbView: ?Sized + Any>(&self) -> &DatabaseDownCaster<DbView> {
        let view_type_id = TypeId::of::<DbView>();
        for (_, view) in self.view_casters.iter() {
            if view.target_type_id == view_type_id {
                // SAFETY: We are unerasing the type erased function pointer having made sure the
                // TypeId matches.
                return unsafe {
                    &*((view as *const ViewCaster).cast::<DatabaseDownCaster<DbView>>())
                };
            }
        }

        panic!(
            "No downcaster registered for type `{}` in `Views`",
            std::any::type_name::<DbView>(),
        );
    }
}

impl std::fmt::Debug for Views {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Views")
            .field("view_casters", &self.view_casters)
            .finish()
    }
}

impl std::fmt::Debug for ViewCaster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("DynViewCaster")
            .field(&self.type_name)
            .finish()
    }
}
