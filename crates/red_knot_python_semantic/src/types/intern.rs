use crate::types::{
    ClassType, FileClassTypeId, FileFunctionTypeId, FileIntersectionTypeId, FileUnionTypeId,
    FunctionType, IntersectionType, ModuleType, UnionType,
};

use ruff_index::IndexVec;

/// Interned types for a file
#[derive(Debug, Eq, PartialEq, Clone)]
pub(crate) struct FileTypeStore {
    /// The type of the module.
    module_type: ModuleType,

    /// The types of the defined classes in this module.
    class_types: IndexVec<FileClassTypeId, ClassType>,

    /// The types of the defined functions in this module.
    function_types: IndexVec<FileFunctionTypeId, FunctionType>,

    union_types: IndexVec<FileUnionTypeId, UnionType>,
    intersection_types: IndexVec<FileIntersectionTypeId, IntersectionType>,
}

impl FileTypeStore {
    pub(super) fn new(module_type: ModuleType) -> Self {
        Self {
            module_type,
            class_types: IndexVec::default(),
            function_types: IndexVec::default(),
            union_types: IndexVec::default(),
            intersection_types: IndexVec::default(),
        }
    }

    pub(super) fn module_ty(&self) -> &ModuleType {
        &self.module_type
    }

    pub(super) fn class_ty(&self, id: FileClassTypeId) -> &ClassType {
        &self.class_types[id]
    }

    pub(super) fn function_ty(&self, id: FileFunctionTypeId) -> &FunctionType {
        &self.function_types[id]
    }

    pub(super) fn union_ty(&self, id: FileUnionTypeId) -> &UnionType {
        &self.union_types[id]
    }

    pub(super) fn intersection_ty(&self, id: FileIntersectionTypeId) -> &IntersectionType {
        &self.intersection_types[id]
    }

    pub(super) fn add_class(&mut self, ty: ClassType) -> FileClassTypeId {
        self.class_types.push(ty)
    }

    pub(super) fn add_function(&mut self, ty: FunctionType) -> FileFunctionTypeId {
        self.function_types.push(ty)
    }

    pub(super) fn add_union(&mut self, ty: UnionType) -> FileUnionTypeId {
        self.union_types.push(ty)
    }

    #[allow(unused)]
    pub(super) fn add_intersection(&mut self, ty: IntersectionType) -> FileIntersectionTypeId {
        self.intersection_types.push(ty)
    }
}
