# Class defenitions in stubs

## Cyclical class definition

In type stubs, classes can reference themselves in their base class definitions. For example, in
`typeshed`, we have `class str(Sequence[str]): ...`.

```py path=a.pyi
class Foo[T]: ...

# TODO we resolve `Foo[Bar]` to `Unknown` here without emitting a diagnostic.
# (Ideally we'd understand generics, but failing that, we shouldn't *silently* infer `Unknown`)
class Bar(Foo[Bar]): ...

reveal_type(Bar)  # revealed: Literal[Bar]
reveal_type(Bar.__mro__)  # revealed: tuple[Literal[Bar], Unknown, Literal[object]]
```
