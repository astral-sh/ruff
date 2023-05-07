use std::mem::size_of_val;

#[derive(Clone, Debug)]
pub struct Export<'a> {
    /// The names of the bindings exported via `__all__`.
    pub names: Vec<&'a str>,
}

#[derive(Clone, Debug)]
pub struct Importation<'a> {
    /// The name to which the import is bound.
    /// Given `import foo`, `name` would be "foo".
    /// Given `import foo as bar`, `name` would be "bar".
    pub name: &'a str,
    /// The full name of the module being imported.
    /// Given `import foo`, `full_name` would be "foo".
    /// Given `import foo as bar`, `full_name` would be "foo".
    pub full_name: &'a str,
}

#[derive(Clone, Debug)]
pub struct FromImportation<'a> {
    /// The name to which the import is bound.
    /// Given `from foo import bar`, `name` would be "bar".
    /// Given `from foo import bar as baz`, `name` would be "baz".
    pub name: &'a str,
    /// The full name of the module being imported.
    /// Given `from foo import bar`, `full_name` would be "foo.bar".
    /// Given `from foo import bar as baz`, `full_name` would be "foo.bar".
    pub full_name: String,
}

#[derive(Clone, Debug)]
pub struct SubmoduleImportation<'a> {
    /// The parent module imported by the submodule import.
    /// Given `import foo.bar`, `module` would be "foo".
    pub name: &'a str,
    /// The full name of the submodule being imported.
    /// Given `import foo.bar`, `full_name` would be "foo.bar".
    pub full_name: &'a str,
}

#[derive(Clone, Debug)]
pub enum BindingKind<'a> {
    Annotation,
    Argument,
    Assignment,
    NamedExprAssignment,
    Binding,
    LoopVar,
    Global,
    Nonlocal,
    Builtin,
    ClassDefinition,
    FunctionDefinition,
    // 32
    Export(Export<'a>),
    FutureImportation,
    // 40
    Importation(Importation<'a>),
    // 48
    FromImportation(FromImportation<'a>),
    // 48
    SubmoduleImportation(SubmoduleImportation<'a>),
}

fn main() {
    let smart = BindingKind::Assignment;
    dbg!(size_of_val(&smart));
}
