# Blender property definitions

Blender property declaration is class variable declaration with
only an annotation (no assignment) and the annotation is a call expression.
Ty infers the return type of the call as the declared type of the variable.

## All Blender property types

```py
def BoolProperty() -> int:
    return 1

def BoolVectorProperty() -> int:
    return 1

def CollectionProperty() -> int:
    return 1

def EnumProperty() -> int:
    return 1

def FloatProperty() -> int:
    return 1

def FloatVectorProperty() -> int:
    return 1

def IntProperty() -> int:
    return 1

def IntVectorProperty() -> int:
    return 1

def PointerProperty() -> int:
    return 1

def RemoveProperty() -> int:
    return 1

def StringProperty() -> int:
    return 1

class A:
    a: BoolProperty()
    b: BoolVectorProperty()
    c: CollectionProperty()
    d: EnumProperty()
    e: FloatProperty()
    f: FloatVectorProperty()
    g: IntProperty()
    h: IntVectorProperty()
    i: PointerProperty()
    j: RemoveProperty()
    k: StringProperty()

reveal_type(A.a)  # revealed: int
reveal_type(A.b)  # revealed: int
reveal_type(A.c)  # revealed: int
reveal_type(A.d)  # revealed: int
reveal_type(A.e)  # revealed: int
reveal_type(A.f)  # revealed: int
reveal_type(A.g)  # revealed: int
reveal_type(A.h)  # revealed: int
reveal_type(A.i)  # revealed: int
reveal_type(A.j)  # revealed: int
reveal_type(A.k)  # revealed: int
```

## Blender properties with arguments

```py
def IntProperty(
    *,
    name: str | None = "",
    description: str | None = "",
    default: int = 0,
    min: int | None = None,
    max: int | None = None
) -> int:
    return 1

class A:
    a: IntProperty()
    b: IntProperty(name="foo")
    c: IntProperty(name="foo", description="desc")
    d: IntProperty(name="foo", description="desc", default=5)
    e: IntProperty(name="foo", description="desc", default=5, min=1)
    f: IntProperty(name="foo", description="desc", default=5, min=1, max=10)

reveal_type(A.a)  # revealed: int
reveal_type(A.b)  # revealed: int
reveal_type(A.c)  # revealed: int
reveal_type(A.d)  # revealed: int
reveal_type(A.e)  # revealed: int
reveal_type(A.f)  # revealed: int
```

## Class body with mixed annotations

```py
def BoolProperty() -> bool:
    return True

def StringProperty() -> str:
    return ""

class A:
    a: BoolProperty()
    b: StringProperty()
    c: int = 42

reveal_type(A.a)  # revealed: bool
reveal_type(A.b)  # revealed: str
reveal_type(A.c)  # revealed: int
```

## Function parameter annotations are unchanged

Call expressions in function parameter annotations still produce an error.

```py
def foo() -> int:
    return 1

def func(
    x: foo(),  # error: [invalid-type-form] "Function calls are not allowed in type expressions (except for Blender properties)"
):
    reveal_type(x)  # revealed: Unknown
```

## With assignment is unchanged

Call expressions in annotations with an assignment still produce an error.

```py
def IntProperty() -> int:
    return 1

# error: [invalid-type-form] "Function calls are not allowed in type expressions (except for Blender properties)"
b: IntProperty() = 1
reveal_type(b)  # revealed: Literal[1]
```

## Functions with other names are unchanged

Call expressions in annotations other then Blender properties still produce an error.

```py
def Foo() -> str:
    return ""

def StringProp() -> str:
    return ""

def CustomProperty() -> str:
    return ""

class A:
    a: Foo() # error: [invalid-type-form] "Function calls are not allowed in type expressions (except for Blender properties)"
    b: StringProp() # error: [invalid-type-form] "Function calls are not allowed in type expressions (except for Blender properties)"
    c: CustomProperty() # error: [invalid-type-form] "Function calls are not allowed in type expressions (except for Blender properties)"

reveal_type(A.a)  # revealed: Unknown
reveal_type(A.b)  # revealed: Unknown
reveal_type(A.c)  # revealed: Unknown
```

## No return annotation

```py
def BoolProperty():
    pass

class A:
    a: BoolProperty()

reveal_type(A.a)  # revealed: Unknown
```
