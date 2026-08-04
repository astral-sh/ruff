# Narrowing using `hasattr()`

## Basic narrowing

The builtin function `hasattr()` can be used to narrow nominal and structural types. This is
accomplished using an intersection with a synthesized protocol:

```py
from typing import final
from typing_extensions import LiteralString

class NonFinalClass: ...

def _(obj: NonFinalClass):
    if hasattr(obj, "spam"):
        reveal_type(obj)  # revealed: NonFinalClass & <Protocol with members 'spam'>
        reveal_type(obj.spam)  # revealed: object
    else:
        reveal_type(obj)  # revealed: NonFinalClass & ~<Protocol with members 'spam'>

        # error: [unresolved-attribute]
        reveal_type(obj.spam)  # revealed: Unknown

    if hasattr(obj, "not-an-identifier"):
        reveal_type(obj)  # revealed: NonFinalClass
    else:
        reveal_type(obj)  # revealed: NonFinalClass
```

For a final class, we recognize that there is no way that an object of `FinalClass` could ever have
a `spam` attribute, so the type is narrowed to `Never`:

```py
@final
class FinalClass: ...

def _(obj: FinalClass):
    if hasattr(obj, "spam"):
        reveal_type(obj)  # revealed: Never
        reveal_type(obj.spam)  # revealed: Never
    else:
        reveal_type(obj)  # revealed: FinalClass

        # error: [unresolved-attribute]
        reveal_type(obj.spam)  # revealed: Unknown
```

When the corresponding attribute is already defined on the class, `hasattr` narrowing does not
change the type. `<Protocol with members 'spam'>` is a supertype of `WithSpam`, and so
`WithSpam & <Protocol …>` simplifies to `WithSpam`:

```py
class WithSpam:
    spam: int = 42

def _(obj: WithSpam):
    if hasattr(obj, "spam"):
        reveal_type(obj)  # revealed: WithSpam
        reveal_type(obj.spam)  # revealed: int
    else:
        reveal_type(obj)  # revealed: Never
```

When a class may or may not have a `spam` attribute, `hasattr` narrowing can provide evidence that
the attribute exists. Here, no `possibly-missing-attribute` error is emitted in the `if` branch:

```py
def returns_bool() -> bool:
    return False

class MaybeWithSpam:
    if returns_bool():
        spam: int = 42

def _(obj: MaybeWithSpam):
    # error: [possibly-missing-attribute]
    reveal_type(obj.spam)  # revealed: int

    if hasattr(obj, "spam"):
        reveal_type(obj)  #  revealed: MaybeWithSpam & <Protocol with members 'spam'>
        reveal_type(obj.spam)  # revealed: int
    else:
        reveal_type(obj)  # revealed: MaybeWithSpam & ~<Protocol with members 'spam'>

        # TODO: Ideally, we would emit `[unresolved-attribute]` and reveal `Unknown` here:
        # error: [possibly-missing-attribute]
        reveal_type(obj.spam)  # revealed: int
```

All attribute available on `object` are still available on these synthesized protocols, but
attributes that are not present on `object` are not available:

```py
def f(x: object):
    if hasattr(x, "__qualname__"):
        reveal_type(x.__repr__)  # revealed: bound method object.__repr__() -> str
        reveal_type(x.__str__)  # revealed: bound method object.__str__() -> str
        reveal_type(x.__dict__)  # revealed: dict[str, Any]

        # error: [unresolved-attribute] "Object of type `<Protocol with members '__qualname__'>` has no attribute `foo`"
        reveal_type(x.foo)  # revealed: Unknown
```

## Attribute types introduced by subclasses

An attribute check cannot eliminate a non-final union member because a subclass could define the
checked attribute with an unrelated type. The diagnostic explains why the checked attribute's type
is less precise than the type declared on the other union member:

```py
class Note: ...

class Editor:
    note: Note | None

class WebView: ...

class EditorWebView:
    editor: Editor | None

def f(webview: WebView | EditorWebView) -> None:
    if hasattr(webview, "editor") and webview.editor:
        webview.editor.note  # snapshot: unresolved-attribute
```

```snapshot
error[unresolved-attribute]: Object of type `~AlwaysFalsy` has no attribute `note`
  --> src/mdtest_snippet.py:13:9
   |
12 |     if hasattr(webview, "editor") and webview.editor:
   |        -------------------------- This check also matches subclasses of `WebView` that define `editor` with an unrelated type
13 |         webview.editor.note  # snapshot: unresolved-attribute
   |         ^^^^^^^^^^^^^^^^^^^
help: If `WebView` should not be subclassed, decorate it with `@final`
```

## The diagnostic is independent of names and condition shape

The same explanation is emitted for unrelated classes and attributes when the attribute check and
the truthiness check occur in nested conditions:

```py
class Payload:
    identifier: str

class Extensible: ...

class WithPayload:
    payload: Payload | None

def f(value: Extensible | WithPayload) -> None:
    if hasattr(value, "payload"):
        if value.payload:
            value.payload.identifier  # snapshot: unresolved-attribute
```

```snapshot
error[unresolved-attribute]: Object of type `~AlwaysFalsy` has no attribute `identifier`
  --> src/mdtest_snippet.py:12:13
   |
10 |     if hasattr(value, "payload"):
   |        ------------------------- This check also matches subclasses of `Extensible` that define `payload` with an unrelated type
11 |         if value.payload:
12 |             value.payload.identifier  # snapshot: unresolved-attribute
   |             ^^^^^^^^^^^^^^^^^^^^^^^^
help: If `Extensible` should not be subclassed, decorate it with `@final`
```

When more than one union member could acquire the checked attribute in a subclass, the diagnostic
lists every class that contributes to the imprecise attribute type:

```py
class AlsoExtensible: ...

def g(value: Extensible | AlsoExtensible | WithPayload) -> None:
    if hasattr(value, "payload"):
        if value.payload:
            value.payload.identifier  # snapshot: unresolved-attribute
```

```snapshot
error[unresolved-attribute]: Object of type `~AlwaysFalsy` has no attribute `identifier`
  --> src/mdtest_snippet.py:18:13
   |
16 |     if hasattr(value, "payload"):
   |        ------------------------- This check also matches subclasses of `Extensible` and `AlsoExtensible` that define `payload` with an unrelated type
17 |         if value.payload:
18 |             value.payload.identifier  # snapshot: unresolved-attribute
   |             ^^^^^^^^^^^^^^^^^^^^^^^^
help: If `Extensible` and `AlsoExtensible` should not be subclassed, decorate all of them with `@final`
```

## A nested `hasattr` call that does not control the branch

A `hasattr` call that is merely an argument to another predicate is not the source of narrowing and
does not receive an explanatory annotation:

```py
from typing import Protocol
from typing_extensions import TypeIs

class Payload:
    identifier: str

class Extensible: ...

class WithPayload:
    payload: Payload | None

class HasPayload(Protocol):
    payload: object

def is_payload(value: object, ignored: bool) -> TypeIs[HasPayload]:
    return True

def f(value: Extensible | WithPayload) -> None:
    if is_payload(value, hasattr(value, "payload")) and value.payload:
        value.payload.identifier  # snapshot: unresolved-attribute
```

```snapshot
error[unresolved-attribute]: Object of type `~AlwaysFalsy` has no attribute `identifier`
  --> src/mdtest_snippet.py:20:9
   |
20 |         value.payload.identifier  # snapshot: unresolved-attribute
   |         ^^^^^^^^^^^^^^^^^^^^^^^^
```
