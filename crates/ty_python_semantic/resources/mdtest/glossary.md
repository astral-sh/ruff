# Glossary

This document is meant to serve as a reference for ty developers, explaining the meaning of various
terms used in the codebase and documentation.

## Local type, public type, imported type, attribute type, nonlocal type

To understand the difference between these terms, consider the following example:

`module.py`:

```py
x_imported = 1

reveal_type(x_imported)  # revealed: Literal[1]

x_imported = 2

reveal_type(x_imported)  # revealed: Literal[2]
```

`main.py`:

```py
from module import x_imported

class C:
    x_attribute = 1
    x_attribute = 2

    reveal_type(x_attribute)  # revealed: Literal[2]

x_nonlocal = 1

def f():
    x_local = 1

    reveal_type(x_local)  # revealed: Literal[1]

    # All of the following are considered "public" types:
    reveal_type(x_imported)  # revealed: Unknown | Literal[2]
    reveal_type(C.x_attribute)  # revealed: Unknown | Literal[2]
    reveal_type(x_nonlocal)  # revealed: Unknown | Literal[1, 2]

    x_local = 2

f()

x_nonlocal = 2

f()
```

The first distinction we make is between "local types" and "public types".

A **local type** refers to the type of a symbol that is defined and accessed within the same scope
(`x_local`). Local types are subject to flow-sensitive analysis: the type of `x_local` at the usage
site in `f` is `Literal[1]`, despite the fact that `x_local` is later reassigned to `2`.

In contrast, the **public type** of a symbol refers to the type that is visible to external scopes.
Since we can not generally analyze the full control flow of the program, we assume that these
symbols could be modified by code that is not currently visible to us. We therefore use the union
with `Unknown` for public uses of these symbols.

Public types can be further subdivided into:

- **Imported types**: The type of a symbol that is imported from another module. Notice how the
    imported type of `x_imported` in `main.py` only includes `Literal[2]`, but not `Literal[1]`. The
    imported type always refers to the local type that the symbol would have at the *end of the
    scope* in which it was defined.
- **Attribute types**: The type of an attribute expression such as `C.x_attribute`. Similar to
    imported types, the attribute type refers to the local type of the symbol at the *end of the
    scope* in which it was defined.
- **Nonlocal types**: Refers to the type of a symbol that is defined in an *enclosing scope*. In
    contrast to imported and attribute types, we consider *all reachable bindings* of the symbol in
    the enclosing scope. This is needed in the example above because `f()` is called from different
    positions in the enclosing scope. The nonlocal type therefore needs to include both `Literal[1]`
    and `Literal[2]`.

To learn more, see:

- [Public type of undeclared symbols](doc/public_type_undeclared_symbols.md)
- [Nonlocal types](nonlocal_types.md)
- [Boundness and declaredness: public uses](boundness_declaredness/public.md)
