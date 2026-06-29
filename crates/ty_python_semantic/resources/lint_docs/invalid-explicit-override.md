## What it does

Checks for methods that are decorated with `@override` but do not override any method in a superclass.

## Why is this bad?

Decorating a method with `@override` declares to the type checker that the intention is that it should
override a method from a superclass.

## Example

```toml
[environment]
python-version = "3.12"
```

```python
from typing import override


class A:
    @override
    def foo(self): ...  # error


class B(A):
    @override
    def ffooo(self): ...  # error


class C:
    @override
    def __repr__(self): ...  # fine: overrides `object.__repr__`


class D(A):
    @override
    def foo(self): ...  # fine: overrides `A.foo`
```
