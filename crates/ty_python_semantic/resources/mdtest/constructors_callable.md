# Constructors as `Callable`

These tests cover converting class constructors into `Callable` types, including the conformance
cases from `typing/conformance/tests/constructors_callable.py`.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, Callable, NoReturn, Self, overload
from typing_extensions import assert_type
from ty_extensions import generic_context, into_callable

def accepts_callable[**P, R](callable: Callable[P, R]) -> Callable[P, R]:
    return callable

class ClassWithoutConstructor: ...

# revealed: () -> ClassWithoutConstructor
reveal_type(into_callable(ClassWithoutConstructor))
# revealed: () -> ClassWithoutConstructor
reveal_type(accepts_callable(ClassWithoutConstructor))
# revealed: ClassWithoutConstructor
reveal_type(accepts_callable(ClassWithoutConstructor)())

class ClassWithNew:
    def __new__(cls, *args, **kwargs) -> Self:
        raise NotImplementedError

# revealed: (...) -> ClassWithNew
reveal_type(into_callable(ClassWithNew))
# revealed: (...) -> ClassWithNew
reveal_type(accepts_callable(ClassWithNew))
# revealed: ClassWithNew
reveal_type(accepts_callable(ClassWithNew)())

class ClassWithInit:
    def __init__(self, x: int) -> None: ...

# revealed: (x: int) -> ClassWithInit
reveal_type(into_callable(ClassWithInit))
# revealed: (x: int) -> ClassWithInit
reveal_type(accepts_callable(ClassWithInit))
assert_type(accepts_callable(ClassWithInit)(1), ClassWithInit)

class ClassWithNewAndInit:
    def __new__(cls, *args, **kwargs) -> Self:
        raise NotImplementedError

    def __init__(self, x: int) -> None: ...

# revealed: ((...) -> ClassWithNewAndInit) | ((x: int) -> ClassWithNewAndInit)
reveal_type(into_callable(ClassWithNewAndInit))
# revealed: (x: int) -> ClassWithNewAndInit
reveal_type(accepts_callable(ClassWithNewAndInit))
assert_type(accepts_callable(ClassWithNewAndInit)(1), ClassWithNewAndInit)

class ClassWithNonSelfNew:
    def __new__(cls, x: int) -> int:
        raise NotImplementedError

# revealed: (x: int) -> int
reveal_type(into_callable(ClassWithNonSelfNew))
# revealed: (x: int) -> int
reveal_type(accepts_callable(ClassWithNonSelfNew))
assert_type(accepts_callable(ClassWithNonSelfNew)(1), int)

class Meta(type):
    def __call__(cls, *args: Any, **kwargs: Any) -> NoReturn:
        raise NotImplementedError

class ClassWithNoReturnMetatype(metaclass=Meta):
    def __new__(cls, *args: Any, **kwargs: Any) -> Self:
        raise NotImplementedError

# revealed: (...) -> Never
reveal_type(into_callable(ClassWithNoReturnMetatype))
# revealed: (...) -> Never
reveal_type(accepts_callable(ClassWithNoReturnMetatype))
# revealed: Never
reveal_type(accepts_callable(ClassWithNoReturnMetatype)())

class Proxy: ...

class ClassWithIgnoredInit:
    def __new__(cls) -> Proxy:
        return Proxy()

    def __init__(self, x: int) -> None: ...

# revealed: () -> Proxy
reveal_type(into_callable(ClassWithIgnoredInit))
# revealed: () -> Proxy
reveal_type(accepts_callable(ClassWithIgnoredInit))
# revealed: Proxy
reveal_type(accepts_callable(ClassWithIgnoredInit)())

class ClassWithIgnoredInitViaAny:
    def __new__(cls) -> Any:
        return super().__new__(cls)

    def __init__(self, x: int) -> None: ...

# revealed: () -> Any
reveal_type(into_callable(ClassWithIgnoredInitViaAny))
# revealed: () -> Any
reveal_type(accepts_callable(ClassWithIgnoredInitViaAny))
# revealed: Any
reveal_type(accepts_callable(ClassWithIgnoredInitViaAny)())

class ClassWithOverloadedInit[T]:
    t: T  # invariant

    @overload
    def __init__(self: "ClassWithOverloadedInit[int]", x: int) -> None: ...
    @overload
    def __init__(self: "ClassWithOverloadedInit[str]", x: str) -> None: ...
    def __init__(self, x: int | str) -> None: ...

# revealed: Overload[[T](x: int) -> ClassWithOverloadedInit[int], [T](x: str) -> ClassWithOverloadedInit[str]]
reveal_type(into_callable(ClassWithOverloadedInit))
# revealed: Overload[[T](x: int) -> ClassWithOverloadedInit[int], [T](x: str) -> ClassWithOverloadedInit[str]]
reveal_type(accepts_callable(ClassWithOverloadedInit))
# revealed: ClassWithOverloadedInit[int]
reveal_type(accepts_callable(ClassWithOverloadedInit)(0))
# revealed: ClassWithOverloadedInit[str]
reveal_type(accepts_callable(ClassWithOverloadedInit)(""))

class GenericClass[T]:
    t: T  # invariant

    def __new__(cls, x: list[T], y: list[T]) -> Self:
        raise NotImplementedError

def _(x: list[str]):
    # revealed: [T](x: list[T], y: list[T]) -> GenericClass[T]
    reveal_type(into_callable(GenericClass))
    # revealed: ty_extensions.GenericContext[T@GenericClass]
    reveal_type(generic_context(into_callable(GenericClass)))

    # revealed: [T](x: list[T], y: list[T]) -> GenericClass[T]
    reveal_type(accepts_callable(GenericClass))
    # revealed: ty_extensions.GenericContext[T@GenericClass]
    reveal_type(generic_context(accepts_callable(GenericClass)))

    # revealed: GenericClass[str]
    reveal_type(accepts_callable(GenericClass)(x, x))

class ClassWithGenericInit:
    def __init__[T](self, x: list[T], y: list[T]) -> None: ...

def _(x: list[str]):
    # revealed: [T](x: list[T], y: list[T]) -> ClassWithGenericInit
    reveal_type(into_callable(ClassWithGenericInit))
    # revealed: [T](x: list[T], y: list[T]) -> ClassWithGenericInit
    reveal_type(accepts_callable(ClassWithGenericInit))
    # revealed: ClassWithGenericInit
    reveal_type(accepts_callable(ClassWithGenericInit)(x, x))
```
