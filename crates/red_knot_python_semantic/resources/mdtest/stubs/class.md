# Class definitions in stubs

## Cyclical class definition

In type stubs, classes can reference themselves in their base class definitions. For example, in
`typeshed`, we have `class str(Sequence[str]): ...`.

```pyi
class Foo[T]: ...

# TODO: actually is subscriptable
# error: [non-subscriptable]
class Bar(Foo[Bar]): ...

reveal_type(Bar)  # revealed: type[Bar]
reveal_type(Bar.__mro__)  # revealed: tuple[type[Bar], Unknown, type[object]]
```
