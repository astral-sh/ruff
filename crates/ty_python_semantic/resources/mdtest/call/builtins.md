# Calling builtins

## `bool` with incorrect arguments

```py
class NotBool:
    __bool__ = None

# error: [too-many-positional-arguments] "Too many positional arguments to class `bool`: expected 1, got 2"
bool(1, 2)

# TODO: We should emit an `unsupported-bool-conversion` error here because the argument doesn't implement `__bool__` correctly.
bool(NotBool())
```

## Calls to `type()`

A single-argument call to `type()` returns an object that has the argument's meta-type. (This is
tested more extensively in `crates/ty_python_semantic/resources/mdtest/attributes.md`, alongside the
tests for the `__class__` attribute.)

```py
reveal_type(type(1))  # revealed: <class 'int'>
```

But a three-argument call to type creates a dynamic instance of the `type` class:

```py
class Base: ...

reveal_type(type("Foo", (), {}))  # revealed: type

reveal_type(type("Foo", (Base,), {"attr": 1}))  # revealed: type
```

Other numbers of arguments are invalid

```py
# error: [no-matching-overload] "No overload of class `type` matches arguments"
type("Foo", ())

# error: [no-matching-overload] "No overload of class `type` matches arguments"
type("Foo", (), {}, weird_other_arg=42)
```

The following calls are also invalid, due to incorrect argument types:

```py
class Base: ...

# error: [invalid-argument-type] "Argument to class `type` is incorrect: Expected `str`, found `Literal[b"Foo"]`"
type(b"Foo", (), {})

# error: [invalid-argument-type] "Argument to class `type` is incorrect: Expected `tuple[type, ...]`, found `<class 'Base'>`"
type("Foo", Base, {})

# error: [invalid-argument-type] "Argument to class `type` is incorrect: Expected `tuple[type, ...]`, found `tuple[Literal[1], Literal[2]]`"
type("Foo", (1, 2), {})

# TODO: this should be an error
type("Foo", (Base,), {b"attr": 1})
```

## Calls to `str()`

### Valid calls

```py
str()
str("")
str(b"")
str(1)
str(object=1)

str(b"M\xc3\xbcsli", "utf-8")
str(b"M\xc3\xbcsli", "utf-8", "replace")

str(b"M\x00\xfc\x00s\x00l\x00i\x00", encoding="utf-16")
str(b"M\x00\xfc\x00s\x00l\x00i\x00", encoding="utf-16", errors="ignore")

str(bytearray.fromhex("4d c3 bc 73 6c 69"), "utf-8")
str(bytearray(), "utf-8")

str(encoding="utf-8", object=b"M\xc3\xbcsli")
str(b"", errors="replace")
str(encoding="utf-8")
str(errors="replace")
```

### Invalid calls

```py
# error: [invalid-argument-type] "Argument to class `str` is incorrect: Expected `bytes | bytearray`, found `Literal[1]`"
# error: [invalid-argument-type] "Argument to class `str` is incorrect: Expected `str`, found `Literal[2]`"
str(1, 2)

# error: [no-matching-overload]
str(o=1)

# First argument is not a bytes-like object:
# error: [invalid-argument-type] "Argument to class `str` is incorrect: Expected `bytes | bytearray`, found `Literal["Müsli"]`"
str("Müsli", "utf-8")

# Second argument is not a valid encoding:
# error: [invalid-argument-type] "Argument to class `str` is incorrect: Expected `str`, found `Literal[b"utf-8"]`"
str(b"M\xc3\xbcsli", b"utf-8")
```
