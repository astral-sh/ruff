# Class defenitions in stubs

## Cyclical class definition

In type stubs, classes can reference themselves in their base class definitions. For example, in `typeshed`, we have `class str(Sequence[str]): ...`.

```py path=a.pyi
class C(C): ...

reveal_type(C)  # revealed: Literal[C]
```
