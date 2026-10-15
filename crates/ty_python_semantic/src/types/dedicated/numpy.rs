use ty_module_resolver::file_to_module;
use ty_python_core::definition::Definition;

use crate::Db;
use crate::types::{ClassSpecialization, Type};

/// Recover the NumPy scalar class objects that its type definitions describe as generic aliases.
pub(in crate::types) fn scalar_class<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    name: &str,
    alias: ClassSpecialization<'db>,
) -> Option<Type<'db>> {
    let origin_name = match name {
        "int8" | "int16" | "int32" | "int64" | "byte" | "short" | "intc" | "intp" | "int_"
        | "long" | "longlong" => "signedinteger",
        "uint8" | "uint16" | "uint32" | "uint64" | "ubyte" | "ushort" | "uintc" | "uintp"
        | "uint" | "ulong" | "ulonglong" => "unsignedinteger",
        "float16" | "float32" | "float64" | "float96" | "float128" | "half" | "single"
        | "double" | "longdouble" | "float_" | "longfloat" => "floating",
        "complex64" | "complex128" | "complex192" | "complex256" | "csingle" | "cdouble"
        | "clongdouble" | "complex_" | "cfloat" | "clongfloat" | "longcomplex" => "complexfloating",
        _ => return None,
    };

    if !definition.file_scope(db).is_global() || alias.origin(db).name(db) != origin_name {
        return None;
    }

    let module = file_to_module(db, definition.program_file(db).resolver_file(db))?;
    match module.name(db).as_str() {
        "numpy" if definition.file(db).is_stub(db) => {}
        // NumPy keeps these platform-dependent definitions in a Python module so its mypy
        // plugin can remove the ones that are unavailable on the target platform.
        "numpy._typing._extended_precision"
            if matches!(name, "float96" | "float128" | "complex192" | "complex256") => {}
        _ => return None,
    }

    let origin_module = file_to_module(db, alias.origin(db).program_file(db).resolver_file(db))?;
    if origin_module.name(db).as_str() != "numpy" {
        return None;
    }

    // These names denote classes at runtime. Preserve the specialization that the stubs use
    // to distinguish scalar types until NumPy replaces the aliases with class definitions.
    // https://github.com/astral-sh/ty/issues/4727#issuecomment-6180744702
    Some(Type::SpecializedClass(alias))
}
