# Legacy `ParamSpec`

## Definition

### Valid

```py
from typing import ParamSpec

P = ParamSpec("P")
reveal_type(type(P))  # revealed: <class 'ParamSpec'>
reveal_type(P)  # revealed: ParamSpec
reveal_type(P.__name__)  # revealed: Literal["P"]
```

The paramspec name can also be provided as a keyword argument:

```py
from typing import ParamSpec

P = ParamSpec(name="P")
reveal_type(P.__name__)  # revealed: Literal["P"]
```

### Requires a name

```py
from typing import ParamSpec

# error: [invalid-paramspec] "The `name` parameter of `ParamSpec` is required."
P = ParamSpec()
```

### Must be directly assigned to a variable

```py
from typing import ParamSpec

P = ParamSpec("P")
# error: [invalid-paramspec]
P1: ParamSpec = ParamSpec("P1")

# error: [invalid-paramspec]
tuple_with_typevar = ("foo", ParamSpec("W"))
reveal_type(tuple_with_typevar[1])  # revealed: ParamSpec
```

```py
from typing_extensions import ParamSpec

T = ParamSpec("T")
# error: [invalid-paramspec]
P1: ParamSpec = ParamSpec("P1")

# error: [invalid-paramspec]
tuple_with_typevar = ("foo", ParamSpec("P2"))
reveal_type(tuple_with_typevar[1])  # revealed: ParamSpec
```

### `ParamSpec` parameter must match variable name

```py
from typing import Callable, Generic, ParamSpec

P1 = ParamSpec("P1")

# error: [mismatched-type-name]
P2 = ParamSpec("P3")

class Wrapper(Generic[P2]): ...

def decorator(f: Callable[P2, int]) -> Callable[P2, int]:
    return f
```

### Bounds and constraints

`ParamSpec` does not allow defining bounds or constraints.

```py
from typing import ParamSpec

# error: [invalid-paramspec]
P1 = ParamSpec("P1", bound=int)
# error: [invalid-paramspec]
P2 = ParamSpec("P2", int, str)
```

### Variance

Legacy `ParamSpec` accepts `covariant` and `contravariant` arguments. A `ParamSpec` with no variance
specified is invariant, and a `ParamSpec` with `infer_variance=True` uses variance inference.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Callable, Generic, ParamSpec

P = ParamSpec("P")

class InvariantParamSpec(Generic[P]):
    callback: Callable[P, None]

in_out_obj: InvariantParamSpec[object] = InvariantParamSpec[int]()  # error: [invalid-assignment]
in_out_int: InvariantParamSpec[int] = InvariantParamSpec[object]()  # error: [invalid-assignment]

InP = ParamSpec("InP", contravariant=True)

class ContravariantParamSpec(Generic[InP]):
    def parameters(self) -> Callable[InP, None]:
        raise NotImplementedError

in_obj: ContravariantParamSpec[object] = ContravariantParamSpec[int]()  # error: [invalid-assignment]
in_int: ContravariantParamSpec[int] = ContravariantParamSpec[object]()

OutP = ParamSpec("OutP", covariant=True)

class CovariantParamSpec(Generic[OutP]):
    def accepts_callback(self, callback: Callable[OutP, None]) -> None:
        raise NotImplementedError

out_int: CovariantParamSpec[int] = CovariantParamSpec[object]()  # error: [invalid-assignment]
out_obj: CovariantParamSpec[object] = CovariantParamSpec[int]()

InferredInP = ParamSpec("InferredInP", infer_variance=True)

class InferredContravariantParamSpec(Generic[InferredInP]):
    def parameters(self) -> Callable[InferredInP, None]:
        raise NotImplementedError

inferred_in_obj: InferredContravariantParamSpec[object] = InferredContravariantParamSpec[int]()  # error: [invalid-assignment]
inferred_in_int: InferredContravariantParamSpec[int] = InferredContravariantParamSpec[object]()

InferredOutP = ParamSpec("InferredOutP", infer_variance=True)

class InferredCovariantParamSpec(Generic[InferredOutP]):
    def accepts_callback(self, callback: Callable[InferredOutP, None]) -> None:
        raise NotImplementedError

inferred_out_int: InferredCovariantParamSpec[int] = InferredCovariantParamSpec[object]()  # error: [invalid-assignment]
inferred_out_obj: InferredCovariantParamSpec[object] = InferredCovariantParamSpec[int]()
```

```py
from typing import ParamSpec

def cond() -> bool:
    return True

# error: [invalid-paramspec]
Both = ParamSpec("Both", covariant=True, contravariant=True)
# error: [invalid-paramspec]
AmbiguousCovariant = ParamSpec("AmbiguousCovariant", covariant=cond())
# error: [invalid-paramspec]
AmbiguousContravariant = ParamSpec("AmbiguousContravariant", contravariant=cond())
# error: [invalid-paramspec]
AmbiguousInferVariance = ParamSpec("AmbiguousInferVariance", infer_variance=cond())
# error: [invalid-paramspec]
CovariantAndInferred = ParamSpec("CovariantAndInferred", covariant=True, infer_variance=True)
```

### Variance in method signatures

Returning `Callable[P, None]` uses `P` contravariantly. Accepting that callable as a parameter
reverses the direction, using `P` covariantly. An explicit variance declaration must permit these
uses.

```py
from typing import Callable, Generic, ParamSpec

P_co = ParamSpec("P_co", covariant=True)
P_contra = ParamSpec("P_contra", contravariant=True)

class Covariant(Generic[P_co]):
    def accepts(self, callback: Callable[P_co, None]) -> None: ...

    # snapshot: invalid-generic-class
    def returns(self) -> Callable[P_co, None]:
        raise NotImplementedError

class Contravariant(Generic[P_contra]):
    def returns(self) -> Callable[P_contra, None]:
        raise NotImplementedError

    # error: [invalid-generic-class] "Variance of type variable `P_contra` is incompatible with method `accepts`"
    def accepts(self, callback: Callable[P_contra, None]) -> None: ...
```

```snapshot
error[invalid-generic-class]: Variance of type variable `P_co` is incompatible with method `returns`
  --> src/mdtest_snippet.py:10:26
   |
10 |     def returns(self) -> Callable[P_co, None]:
   |                          ^^^^^^^^^^^^^^^^^^^^
info: Type variable `P_co` is declared as covariant, but this method requires it to be contravariant
```

Forwarding `P.args` and `P.kwargs` also consumes `P`.

```py
class CovariantForwarder(Generic[P_co]):
    # snapshot: invalid-generic-class
    def call(self, *args: P_co.args, **kwargs: P_co.kwargs) -> None: ...

class ContravariantForwarder(Generic[P_contra]):
    def call(self, *args: P_contra.args, **kwargs: P_contra.kwargs) -> None: ...
    def returns(self) -> Callable[P_contra, None]:
        raise NotImplementedError
```

```snapshot
error[invalid-generic-class]: Variance of type variable `P_co` is incompatible with method `call`
  --> src/mdtest_snippet.py:21:27
   |
21 |     def call(self, *args: P_co.args, **kwargs: P_co.kwargs) -> None: ...
   |                           ^^^^^^^^^
info: Type variable `P_co` is declared as covariant, but this method requires it to be contravariant
```

Constructors can establish a specialization without respecting its declared variance. A `ParamSpec`
bound to a function or method instead of the class ignores its declared variance.

```py
class Constructed(Generic[P_co]):
    def __init__(self, *args: P_co.args, **kwargs: P_co.kwargs) -> None: ...
    def __new__(cls, *args: P_co.args, **kwargs: P_co.kwargs) -> "Constructed[P_co]":
        raise NotImplementedError

def returns() -> Callable[P_co, None]:
    raise NotImplementedError

class NotGeneric:
    def returns(self) -> Callable[P_co, None]:
        raise NotImplementedError
```

Class methods are checked too. A static method has no receiver, so its first parameter contributes
to its variance.

```py
class MethodKinds(Generic[P_contra]):
    @classmethod
    # error: [invalid-generic-class]
    def class_method(cls, callback: Callable[P_contra, None]) -> None: ...
    @staticmethod
    # error: [invalid-generic-class]
    def static_method(callback: Callable[P_contra, None]) -> None: ...
```

### Variance in overloaded methods

TODO: Variance validation is deferred for overloaded methods until it accounts for the complete
overload set. We miss the invalid use of a covariant `ParamSpec` in the first overload's return
type.

```py
from typing import Callable, Generic, ParamSpec, overload

P_co = ParamSpec("P_co", covariant=True)

class Overloaded(Generic[P_co]):
    @overload
    # TODO: Emit `invalid-generic-class`; this use of `P_co` requires contravariance.
    def method(self, value: int) -> Callable[P_co, None]: ...
    @overload
    def method(self, value: str) -> None: ...
    def method(self, value: object) -> Callable[P_co, None] | None:
        return None
```

### Variance in generic methods

A method's independent type variable can accept any argument. The `Callable[P_contra, None]` arm in
the parameter annotation is redundant because `T` already accepts that argument, and the result
includes both types. This signature respects the class's contravariance.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Callable, Generic, ParamSpec, TypeVar

P_contra = ParamSpec("P_contra", contravariant=True)
T = TypeVar("T")

class Contravariant(Generic[P_contra]):
    def identity(self, value: Callable[P_contra, None] | T) -> Callable[P_contra, None] | T:
        return value
```

TODO: Until those relationships are checked, we defer validation for generic methods, including type
parameters scoped to a returned callable. The incompatible return annotations below are not yet
reported.

```py
P_co = ParamSpec("P_co", covariant=True)

class GenericMethods(Generic[P_co]):
    # TODO: Emit `invalid-generic-class`; this use of `P_co` requires contravariance.
    def legacy(self, value: T) -> Callable[P_co, T]:
        raise NotImplementedError

    # TODO: Emit `invalid-generic-class`; this use of `P_co` requires contravariance.
    def pep695[U](self, value: U) -> Callable[P_co, U]:
        raise NotImplementedError

    # TODO: Emit `invalid-generic-class`; this use of `P_co` requires contravariance.
    def returned_callable(self) -> Callable[P_co, T]:
        raise NotImplementedError
```

### Variance with explicit receivers

`Self` and the class's own `ParamSpec` do not restrict the receiver. A covariant `ParamSpec` in a
returned callable's parameters still violates covariance.

```toml
[environment]
python-version = "3.11"
```

```py
from typing import Callable, Generic, ParamSpec, Self

P_co = ParamSpec("P_co", covariant=True)

class Unrestricted(Generic[P_co]):
    # error: [invalid-generic-class]
    def method(self: Self) -> Callable[P_co, None]:
        raise NotImplementedError

    @classmethod
    # error: [invalid-generic-class]
    def class_method(cls: type["Unrestricted[P_co]"]) -> Callable[P_co, None]:
        raise NotImplementedError

    def accepts(self: "Unrestricted[P_co]", callback: Callable[P_co, None]) -> None: ...
```

A specialized receiver does not make this contravariant use of `P_co` valid.

```py
class Restricted(Generic[P_co]):
    # error: [invalid-generic-class]
    def method(self: "Restricted[[int]]") -> Callable[P_co, None]:
        raise NotImplementedError
```

### Defaults

```toml
[environment]
python-version = "3.13"
```

The default value for a `ParamSpec` can be either a list of types, `...`, or another `ParamSpec`.

```py
from typing import ParamSpec

P1 = ParamSpec("P1", default=[int, str])
P2 = ParamSpec("P2", default=...)
P3 = ParamSpec("P3", default=P2)
Q = ParamSpec("Q")

# error: [invalid-type-form] "Bare ParamSpec `Q` is not valid in this context"
P5 = ParamSpec("P5", default=[Q])
```

Other values are invalid.

```py
# error: [invalid-paramspec]
P4 = ParamSpec("P4", default=int)
```

### `default` parameter in `typing_extensions.ParamSpec`

```toml
[environment]
python-version = "3.12"
```

The `default` parameter to `ParamSpec` is available from `typing_extensions` in Python 3.12 and
earlier.

```py
from typing import ParamSpec
from typing_extensions import ParamSpec as ExtParamSpec

# This shouldn't emit a diagnostic
P1 = ExtParamSpec("P1", default=[int, str])

# But, this should
# error: [invalid-paramspec] "The `default` parameter of `typing.ParamSpec` was added in Python 3.13"
P2 = ParamSpec("P2", default=[int, str])
```

And, it allows the same set of values as `typing.ParamSpec`.

```py
P3 = ExtParamSpec("P3", default=...)
P4 = ExtParamSpec("P4", default=P3)

# error: [invalid-paramspec]
P5 = ExtParamSpec("P5", default=int)
```

### `typing_extensions.ParamSpec` defaults specialize generic classes

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Callable, Generic
from typing_extensions import ParamSpec

PList = ParamSpec("PList", default=[str])
PEllipsis = ParamSpec("PEllipsis", default=...)
PAnother = ParamSpec("PAnother", default=PList)

class C1(Generic[PList]):
    x: Callable[PList, None]

class C2(Generic[PEllipsis]):
    x: Callable[PEllipsis, None]

class C3(Generic[PList, PAnother]):
    x: Callable[PAnother, None]

reveal_type(C1().x)  # revealed: (str, /) -> None
reveal_type(C2().x)  # revealed: (...) -> None
reveal_type(C3().x)  # revealed: (str, /) -> None
```

### Forward references in stub files

Stubs natively support forward references, so patterns that would raise `NameError` at runtime are
allowed in stub files:

```pyi
from typing_extensions import ParamSpec

P = ParamSpec("P", default=[A, B])

class A: ...
class B: ...
```

## Validating `ParamSpec` usage

In type annotations, `ParamSpec` is only valid as the first element to `Callable`, the final element
to `Concatenate`, or as a type parameter to `Protocol` or `Generic`.

<!-- snapshot-diagnostics -->

`library.py`:

```py
from typing import ParamSpec

LibraryP = ParamSpec("LibraryP")
```

`main.py`:

```py
import library
from typing import Any, Final, ParamSpec, Callable, Concatenate, Protocol, Generic, Union, Optional, Annotated

P = ParamSpec("P")
Q = ParamSpec("Q")

class ValidProtocol(Protocol[P]):
    def method(self, c: Callable[P, int]) -> None: ...

class ValidGeneric(Generic[P]):
    def method(self, c: Callable[P, int]) -> None: ...

def valid(
    a1: Callable[P, int],
    a2: Callable[Concatenate[int, P], int],
    a3: Callable["P", int],
    a4: Callable[Concatenate[int, "P"], int],
    a5: Callable[library.LibraryP, int],
    a6: Callable["Concatenate[int, P]", int],
    a7: Callable["library.LibraryP", int],
) -> None: ...
def invalid(
    # error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
    a1: P,
    # error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
    a3: Callable[[P], int],
    # error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
    a4: Callable[..., P],
    # error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
    a5: Callable[Concatenate[P, ...], int],
    # error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
    a6: P | int,
    # error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
    a7: Union[P, int],
    # error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
    a8: Optional[P],
    # error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
    a9: Annotated[P, "metadata"],
    # error: [invalid-type-form] "The first argument to `Callable` must be either a list of types, ParamSpec, Concatenate, or `...`"
    a10: Callable["[int, str]", str],
    # error: [invalid-type-form] "The first argument to `Callable` must be either a list of types, ParamSpec, Concatenate, or `...`"
    a11: Callable["...", int],
) -> None: ...

# error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
def invalid_return() -> P:
    raise NotImplementedError

def invalid_variable_annotation(y: Any) -> None:
    # error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
    x: P = y

def invalid_with_qualifier(y: Any) -> None:
    # error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
    x: Final[P] = y

# error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
def invalid_stringified_return() -> "P":
    raise NotImplementedError

def invalid_stringified_annotation(
    # error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
    a: "P",
) -> None: ...
def invalid_stringified_variable_annotation(y: Any) -> None:
    # error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
    x: "P" = y

class InvalidSpecializationTarget(Generic[P]):
    attr: Callable[P, None]

def invalid_specialization(
    # error: [invalid-type-form] "Bare ParamSpec `Q` is not valid in this context"
    a: InvalidSpecializationTarget[[Q]],
    # error: [invalid-type-form] "Bare ParamSpec `Q` is not valid in this context"
    b: InvalidSpecializationTarget[Q,],
) -> None: ...
```

## Validating `P.args` and `P.kwargs` usage

The components of `ParamSpec` i.e., `P.args` and `P.kwargs` are only valid when used as the
annotated types of `*args` and `**kwargs` respectively.

```py
from typing import Generic, Callable, ParamSpec
from ty_extensions._internal import generic_context

P = ParamSpec("P")

def foo1(c: Callable[P, int]) -> None:
    def nested1(*args: P.args, **kwargs: P.kwargs) -> None: ...
    def nested2(
        # error: [invalid-type-form] "`P.kwargs` is valid only in `**kwargs` annotation: Did you mean `P.args`?"
        *args: P.kwargs,
        # error: [invalid-type-form] "`P.args` is valid only in `*args` annotation: Did you mean `P.kwargs`?"
        **kwargs: P.args,
    ) -> None: ...

    # error: [invalid-paramspec] "`*args: P.args` must be accompanied by `**kwargs: P.kwargs`"
    def nested3(*args: P.args) -> None: ...

    # error: [invalid-paramspec] "`**kwargs: P.kwargs` must be accompanied by `*args: P.args`"
    def nested4(**kwargs: P.kwargs) -> None: ...

    # error: [invalid-paramspec] "No parameters may appear between `*args: P.args` and `**kwargs: P.kwargs`"
    def nested5(*args: P.args, x: int, **kwargs: P.kwargs) -> None: ...

    # error: [invalid-paramspec] "`P.args` is only valid for annotating `*args`"
    def nested6(x: P.args) -> None: ...
    def nested7(
        *args: P.args,
        # error: [invalid-paramspec] "`*args: P.args` must be accompanied by `**kwargs: P.kwargs`"
        **kwargs: int,
    ) -> None: ...
```

`P.args` and `P.kwargs` do not bind `P` themselves. They must refer to a `ParamSpec` bound by
another parameter annotation or a visible enclosing generic context. A return annotation on the same
function is not sufficient. A generic outer class does not make its `ParamSpec` visible across a
nested class boundary.

```py
# snapshot: unbound-type-variable
def bar1(*args: P.args, **kwargs: P.kwargs) -> None:
    pass

# error: [unbound-type-variable] "ParamSpec `P` is not in scope"
def return_only(*args: P.args, **kwargs: P.kwargs) -> Callable[P, int]:
    raise NotImplementedError

class Foo1:
    # error: [unbound-type-variable] "ParamSpec `P` is not in scope"
    def method(self, *args: P.args, **kwargs: P.kwargs) -> None: ...

class Outer(Generic[P]):
    def method(self, *args: P.args, **kwargs: P.kwargs) -> None: ...

    class Inner:
        # error: [unbound-type-variable] "ParamSpec `P` is not in scope"
        def method(self, *args: P.args, **kwargs: P.kwargs) -> None: ...

    def method_with_nested_class(self, callback: Callable[P, int]) -> None:
        class Inner:
            # error: [unbound-type-variable] "ParamSpec `P` is not in scope"
            def method(self, *args: P.args, **kwargs: P.kwargs) -> None: ...

    def method_with_components(self, *outer_args: P.args, **outer_kwargs: P.kwargs) -> None:
        class Inner:
            # error: [unbound-type-variable] "ParamSpec `P` is not in scope"
            def method(self, *args: P.args, **kwargs: P.kwargs) -> None: ...
```

```snapshot
error[unbound-type-variable]: ParamSpec `P` is not in scope
  --> src/mdtest_snippet.py:32:17
   |
32 | def bar1(*args: P.args, **kwargs: P.kwargs) -> None:
   |                 ^^^^^^            -------- This component uses the same out-of-scope ParamSpec
```

A `ParamSpec` moved to an enclosing factory's returned callable remains lexically visible within the
factory's body. A nested function referring to it through its components owns its own binding,
consistently with an ordinary return-only `TypeVar`:

```py
def callable_factory() -> Callable[P, int]:
    def nested(*args: P.args, **kwargs: P.kwargs) -> int:
        return 1

    def nested_with_parameter(callback: Callable[P, int], *args: P.args, **kwargs: P.kwargs) -> int:
        return callback(*args, **kwargs)

    # revealed: ty_extensions._internal.GenericContext[P@nested_with_parameter]
    reveal_type(generic_context(nested_with_parameter))
    return nested

def repeated_paramspec_factory() -> Callable[P, Callable[P, int]]:
    def nested(*args: P.args, **kwargs: P.kwargs) -> Callable[P, int]:
        callback: Callable[P, int]
        raise NotImplementedError

    return nested

def nested_components_factory() -> Callable[P, int]:
    def outer(*args: P.args, **kwargs: P.kwargs) -> int:
        def inner(*inner_args: P.args, **inner_kwargs: P.kwargs) -> int:
            return 1

        return inner(*args, **kwargs)

    return outer
```

And, they need to be used together.

```py
def foo2(c: Callable[P, int]) -> None:
    # error: [invalid-paramspec] "`*args: P.args` must be accompanied by `**kwargs: P.kwargs`"
    def nested1(*args: P.args) -> None: ...

    # error: [invalid-paramspec] "`**kwargs: P.kwargs` must be accompanied by `*args: P.args`"
    def nested2(**kwargs: P.kwargs) -> None: ...

class Foo2:
    # error: [invalid-paramspec] "`P.args` is only valid for annotating `*args` function parameters"
    args: P.args

    # error: [invalid-paramspec] "`P.kwargs` is only valid for annotating `**kwargs` function parameters"
    kwargs: P.kwargs
```

The name of these parameters does not need to be `args` or `kwargs`, it's the annotated type to the
respective variadic parameter that matters.

```py
class Foo3(Generic[P]):
    def method1(self, *paramspec_args: P.args, **paramspec_kwargs: P.kwargs) -> None: ...
    def method2(
        self,
        # error: [invalid-type-form] "`P.kwargs` is valid only in `**kwargs` annotation: Did you mean `P.args`?"
        *paramspec_args: P.kwargs,
        # error: [invalid-type-form] "`P.args` is valid only in `*args` annotation: Did you mean `P.kwargs`?"
        **paramspec_kwargs: P.args,
    ) -> None: ...
```

Error messages for `invalid-paramspec` also use the actual parameter names:

```py
def bar(c: Callable[P, int]) -> None:
    # error: [invalid-paramspec] "`*my_args: P.args` must be accompanied by `**my_kwargs: P.kwargs`"
    def f1(*my_args: P.args, **my_kwargs: int) -> None: ...

    # error: [invalid-paramspec] "`*positional: P.args` must be accompanied by `**kwargs: P.kwargs`"
    def f2(*positional: P.args) -> None: ...

    # error: [invalid-paramspec] "`**keyword: P.kwargs` must be accompanied by `*args: P.args`"
    def f3(**keyword: P.kwargs) -> None: ...

    # error: [invalid-paramspec] "No parameters may appear between `*a: P.args` and `**kw: P.kwargs`"
    def f4(*a: P.args, x: int, **kw: P.kwargs) -> None: ...
```

## Return-only ParamSpecs own nested callables

A legacy `ParamSpec` appearing only in a factory's returned callable belongs to the callable, just
as a return-only `TypeVar` does. The nested callable should therefore own its `ParamSpec`, making it
possible to infer concrete arguments when the callable is used inside the factory.

```py
from typing import Callable, ParamSpec, TypeVar
from ty_extensions._internal import generic_context

P = ParamSpec("P")
T = TypeVar("T")

def takes_int(value: int) -> int:
    return value

def paramspec_factory() -> Callable[P, int]:
    def nested(*args: P.args, **kwargs: P.kwargs) -> int:
        reveal_type(args)  # revealed: P@nested.args
        return 1

    def with_callback(callback: Callable[P, int], *args: P.args, **kwargs: P.kwargs) -> int:
        reveal_type(callback)  # revealed: (**P@with_callback) -> int
        return callback(*args, **kwargs)

    reveal_type(generic_context(nested))  # revealed: ty_extensions._internal.GenericContext[P@nested]
    reveal_type(generic_context(with_callback))  # revealed: ty_extensions._internal.GenericContext[P@with_callback]
    reveal_type(with_callback(takes_int, 1))  # revealed: int
    return nested

def typevar_factory() -> Callable[[T], int]:
    def nested(value: T) -> int:
        reveal_type(value)  # revealed: T@nested
        return 1

    reveal_type(generic_context(nested))  # revealed: ty_extensions._internal.GenericContext[T@nested]
    return nested
```

A genuinely generic enclosing function still owns the `ParamSpec` captured by its nested callable.

```py
def public_paramspec(callback: Callable[P, int]) -> None:
    def nested(*args: P.args, **kwargs: P.kwargs) -> int:
        reveal_type(args)  # revealed: P@public_paramspec.args
        return callback(*args, **kwargs)

    reveal_type(generic_context(nested))  # revealed: None
```

## Specializing generic classes explicitly

```py
from typing import Any, Generic, ParamSpec, Callable, TypeVar

P1 = ParamSpec("P1")
P2 = ParamSpec("P2")
T1 = TypeVar("T1")

class OnlyParamSpec(Generic[P1]):
    attr: Callable[P1, None]

class TwoParamSpec(Generic[P1, P2]):
    attr1: Callable[P1, None]
    attr2: Callable[P2, None]

class TypeVarAndParamSpec(Generic[T1, P1]):
    attr: Callable[P1, T1]

class ParamSpecAndTypeVar(Generic[P1, T1]):
    attr: Callable[P1, T1]
```

Explicit specialization of a generic class involving `ParamSpec` is done by providing either a list
of types, `...`, or another in-scope `ParamSpec`.

```py
reveal_type(OnlyParamSpec[[]]().attr)  # revealed: () -> None
reveal_type(OnlyParamSpec[[int, str]]().attr)  # revealed: (int, str, /) -> None
reveal_type(OnlyParamSpec[...]().attr)  # revealed: (...) -> None

def func(c: Callable[P2, None]):
    reveal_type(OnlyParamSpec[P2]().attr)  # revealed: (**P2@func) -> None

# error: [unbound-type-variable] "Type variable `P2` is not bound to any outer generic context"
reveal_type(OnlyParamSpec[P2]().attr)  # revealed: (...) -> None

# error: [invalid-type-arguments] "No type argument provided for required type variable `P1` of class `OnlyParamSpec`"
reveal_type(OnlyParamSpec[()]().attr)  # revealed: (...) -> None
```

An explicit tuple expression (unlike an implicit one that omits the parentheses) is also accepted
when the `ParamSpec` is the only type variable. But, this isn't recommended is mainly a fallout of
it having the same AST as the one without the parentheses. Both mypy and Pyright also allow this.

```py
reveal_type(OnlyParamSpec[(int, str)]().attr)  # revealed: (int, str, /) -> None
```

<!-- fmt:off -->

```py
# error: [invalid-syntax]
reveal_type(OnlyParamSpec[]().attr)  # revealed: (...) -> None
```

<!-- fmt:on -->

The square brackets can be omitted when `ParamSpec` is the only type variable

```py
reveal_type(OnlyParamSpec[int, str]().attr)  # revealed: (int, str, /) -> None
reveal_type(OnlyParamSpec[int,]().attr)  # revealed: (int, /) -> None

# Even when there is only one element
reveal_type(OnlyParamSpec[Any]().attr)  # revealed: (Any, /) -> None
reveal_type(OnlyParamSpec[object]().attr)  # revealed: (object, /) -> None
reveal_type(OnlyParamSpec[int]().attr)  # revealed: (int, /) -> None
```

But, they cannot be omitted when there are multiple type variables.

```py
reveal_type(TypeVarAndParamSpec[int, []]().attr)  # revealed: () -> int
reveal_type(TypeVarAndParamSpec[int, [int, str]]().attr)  # revealed: (int, str, /) -> int
reveal_type(TypeVarAndParamSpec[int, [str]]().attr)  # revealed: (str, /) -> int
reveal_type(TypeVarAndParamSpec[int, ...]().attr)  # revealed: (...) -> int
reveal_type(ParamSpecAndTypeVar[[int, str], str]().attr)  # revealed: (int, str, /) -> str

# error: [unbound-type-variable] "Type variable `P2` is not bound to any outer generic context"
reveal_type(TypeVarAndParamSpec[int, P2]().attr)  # revealed: (...) -> int
# error: [invalid-type-arguments] "Type argument for `ParamSpec` must be either a list of types, `ParamSpec`, `Concatenate`, or `...`"
reveal_type(TypeVarAndParamSpec[int, int]().attr)  # revealed: (...) -> int
# error: [invalid-type-arguments] "Type argument for `ParamSpec` must be"
reveal_type(TypeVarAndParamSpec[int, ()]().attr)  # revealed: (...) -> int
# error: [invalid-type-arguments] "Type argument for `ParamSpec` must be"
reveal_type(TypeVarAndParamSpec[int, (int, str)]().attr)  # revealed: (...) -> int
```

Nor can they be omitted when there are more than one `ParamSpec`s.

```py
p = TwoParamSpec[[int, str], [int]]()
reveal_type(p.attr1)  # revealed: (int, str, /) -> None
reveal_type(p.attr2)  # revealed: (int, /) -> None

# error: [invalid-type-arguments]
# error: [invalid-type-arguments]
TwoParamSpec[int, str]
```

Specializing `ParamSpec` type variable using `typing.Any` isn't explicitly allowed by the spec but
both mypy and Pyright allow this and there are usages of this in the wild e.g.,
`staticmethod[Any, Any]`.

```py
reveal_type(TypeVarAndParamSpec[int, Any]().attr)  # revealed: (...) -> int
```

`...` has the same gradual behavior when used as a `ParamSpec` argument in a generic class,
regardless of the variance of the `ParamSpec`.

```py
from typing import Callable, Generic, ParamSpec

# legacy ParamSpec with no variance specified is invariant
P = ParamSpec("P")

class Command(Generic[P]):
    callback: Callable[P, None]

# confirm that Command is invariant in P
def _(of_int: Command[int], of_bool: Command[bool]) -> None:
    a: Command[int] = of_bool  # error: [invalid-assignment]
    b: Command[bool] = of_int  # error: [invalid-assignment]

# but gradual signature is still assignable in both directions
def _(concrete: Command[[str]], gradual: Command[...]) -> None:
    a: Command[...] = concrete
    b: Command[[str]] = gradual
```

`ParamSpec` specializations in generic classes are compared using the callable parameter relation.
This avoids rejecting wrappers around callbacks that are safe to use with a positional-only callback
protocol.

```py
from collections.abc import Callable
from typing import Final, Generic, ParamSpec

P = ParamSpec("P", contravariant=True)

class Job(Generic[P]):
    target: Final[Callable[P, None]]

    def __init__(self, target: Callable[P, None]) -> None:
        self.target = target

def named(x: int) -> None:
    pass

def defaulted(x: int | None = None) -> None:
    pass

def wrong(x: str) -> None:
    pass

named_job = Job(named)
defaulted_job = Job(defaulted)
wrong_job = Job(wrong)

def takes_int_job(job: Job[[int]]) -> None:
    pass

takes_int_job(named_job)
takes_int_job(defaulted_job)
takes_int_job(wrong_job)  # error: [invalid-argument-type]
```

A fixed `ParamSpec` can contain required parameters. A wrapper around such a callback cannot be used
as a wrapper around a callback that accepts no arguments.

```py
def erase_parameters(job: Job[P]) -> Job[[]]:
    return job  # error: [invalid-return-type]
```

The same restriction applies in the other direction when a class consumes callbacks. A consumer of
callbacks with no parameters cannot accept a callback with arbitrary required parameters.

```py
P_co = ParamSpec("P_co", covariant=True)

class CallbackConsumer(Generic[P_co]):
    def consume(self, callback: Callable[P_co, None]) -> None: ...

def broaden_parameters(consumer: CallbackConsumer[[]]) -> CallbackConsumer[P_co]:
    return consumer  # error: [invalid-return-type]
```

## Inferring an invariant `ParamSpec` through `Concatenate`

A `Concatenate` prefix is positional-only, so a callback whose first parameter also accepts a
keyword is not compatible with an invariant wrapper. Even though that argument is rejected, its
remaining parameters must still be inferred precisely.

```py
from typing import Callable, Concatenate, Generic, ParamSpec

P = ParamSpec("P")

class Callback(Generic[P]):
    def __init__(self, callback: Callable[P, None]) -> None: ...

def without_first(callback: Callback[Concatenate[object, P]]) -> Callable[P, None]:
    raise NotImplementedError

def original(first: object, value: str) -> None: ...

remaining = without_first(Callback(original))  # error: [invalid-argument-type]
reveal_type(remaining)  # revealed: (value: str) -> None
remaining(1)  # error: [invalid-argument-type]
```

## Inferring a contravariant `ParamSpec` through `Concatenate`

The same callback is compatible with a contravariant wrapper, and its remaining parameters must
still be inferred precisely enough to reject an incompatible later argument.

```py
from typing import Callable, Concatenate, Generic, ParamSpec, TypeVar

P = ParamSpec("P", contravariant=True)

class Callback(Generic[P]):
    def __init__(self, callback: Callable[P, None]) -> None: ...

def without_first(callback: Callback[Concatenate[object, P]]) -> Callable[P, None]:
    raise NotImplementedError

def original(first: object, value: str) -> None: ...

remaining = without_first(Callback(original))
reveal_type(remaining)  # revealed: (value: str) -> None
remaining(1)  # error: [invalid-argument-type]
```

A contravariant callback can accept a broader positional-only prefix than a bounded type variable
requires. The separate `Middle()` argument determines the narrower specialization without rejecting
the valid callback.

```py
class Base: ...
class Middle(Base): ...
class Other: ...

Bounded = TypeVar("Bounded", bound=Middle)
Q = ParamSpec("Q")

def accepts_base(first: Base, /, value: str) -> None: ...
def bounded(callback: Callback[Concatenate[Bounded, Q]], witness: Bounded) -> tuple[Bounded, Callable[Q, None]]:
    raise NotImplementedError

bounded_result = bounded(Callback(accepts_base), Middle())
reveal_type(bounded_result)  # revealed: tuple[Middle, (value: str) -> None]
bounded_result[1](1)  # error: [invalid-argument-type]
```

The same contravariant relationship remains valid when the prefix type variable has explicit
constraints instead of an upper bound.

```py
Constrained = TypeVar("Constrained", Middle, Other)

def constrained(callback: Callback[Concatenate[Constrained, Q]], witness: Constrained) -> tuple[Constrained, Callable[Q, None]]:
    raise NotImplementedError

constrained_result = constrained(Callback(accepts_base), Middle())
reveal_type(constrained_result)  # revealed: tuple[Middle, (value: str) -> None]
constrained_result[1](1)  # error: [invalid-argument-type]
```

## Inferring a covariant `ParamSpec` through `Concatenate`

A covariant wrapper containing a callback with a narrower positional-only prefix remains valid,
whether the wrapper is stored first or constructed inline.

```py
from typing import Callable, Concatenate, Generic, ParamSpec

P = ParamSpec("P", covariant=True)
Q = ParamSpec("Q")

class Base: ...
class Middle(Base): ...

class Callback(Generic[P]):
    def __init__(self, callback: Callable[P, None]) -> None: ...

def without_first(callback: Callback[Concatenate[Base, Q]]) -> Callable[Q, None]:
    raise NotImplementedError

def original(first: Middle, /, value: str) -> None: ...

wrapped = Callback(original)
# TODO: Should reveal `(value: str) -> None`. Needs ParamSpecs in the new constraint solver.
reveal_type(without_first(wrapped))  # revealed: (...) -> None
# TODO: Should reveal `(value: str) -> None`. Needs ParamSpecs in the new constraint solver.
reveal_type(without_first(Callback(original)))  # revealed: (...) -> None
```

## Inferring through unions of structural `ParamSpec` protocols

Different protocols with the same parameter list can satisfy a target protocol structurally, even
when they appear in a union nested inside an invariant or contravariant wrapper.

```py
from typing import Generic, ParamSpec, Protocol, TypeVar

P = ParamSpec("P")
T = TypeVar("T")
TContra = TypeVar("TContra", contravariant=True)

class Invariant(Generic[T]):
    value: T

class Contravariant(Generic[TContra]):
    def put(self, value: TContra) -> None: ...

class Target(Protocol[P]):
    def call(self, *args: P.args, **kwargs: P.kwargs) -> None: ...

class Actual(Protocol[P]):
    def call(self, *args: P.args, **kwargs: P.kwargs) -> None: ...

class Other(Protocol[P]):
    def call(self, *args: P.args, **kwargs: P.kwargs) -> None: ...

def invariant(value: Invariant[Target[P]]) -> Target[P]:
    raise NotImplementedError

def contravariant(value: Contravariant[Target[P]]) -> Target[P]:
    raise NotImplementedError

def compatible(
    first: Invariant[Actual[[str]] | Other[[str]]],
    second: Contravariant[Actual[[str]] | Other[[str]]],
) -> None:
    reveal_type(invariant(first))  # revealed: Target[(str, /)]
    reveal_type(contravariant(second))  # revealed: Target[(str, /)]
```

Union members with incompatible parameter lists cannot satisfy either wrapper. Their signatures must
remain visible in the inferred result instead of collapsing to a gradual parameter list.

```py
def incompatible(
    first: Invariant[Actual[[str]] | Other[[bytes]]],
    second: Contravariant[Actual[[str]] | Other[[bytes]]],
) -> None:
    # error: [invalid-argument-type]
    reveal_type(invariant(first))  # revealed: Target[((str, /)) | ((bytes, /))]
    # error: [invalid-argument-type]
    reveal_type(contravariant(second))  # revealed: Target[((str, /)) | ((bytes, /))]
```

## `ParamSpec` cannot specialize a `TypeVar`, and vice versa

<!-- snapshot-diagnostics -->

A `ParamSpec` is not a valid type argument for a regular `TypeVar`, and vice versa.

```py
from typing import Generic, Callable, TypeVar, ParamSpec

T = TypeVar("T")
P = ParamSpec("P")

class OnlyTypeVar(Generic[T]):
    attr: T

def func(c: Callable[P, None]):
    # error: [invalid-type-arguments] "ParamSpec `P` cannot be used to specialize type variable `T`"
    a: OnlyTypeVar[P]

class OnlyParamSpec(Generic[P]):
    attr: Callable[P, None]

# This is fine due to the special case whereby `OnlyParamSpec[T]` is interpreted the same as
# `OnlyParamSpec[[T]]`, due to the fact that `OnlyParamSpec` is only generic over a single
# `ParamSpec` and no other type variables.
def func2(c: OnlyParamSpec[T], other: T):
    reveal_type(c.attr)  # revealed: (T@func2, /) -> None

class ParamSpecAndTypeVar(Generic[P, T]):
    attr: Callable[P, T]

# error: [invalid-type-arguments] "Type argument for `ParamSpec` must be either a list of types, `ParamSpec`, `Concatenate`, or `...`"
def func3(c: ParamSpecAndTypeVar[T, int], other: T): ...
```

## Specialization when defaults are involved

```toml
[environment]
python-version = "3.13"
```

```py
from typing import Any, Generic, ParamSpec, Callable, TypeVar

P = ParamSpec("P")
PList = ParamSpec("PList", default=[int, str])
PEllipsis = ParamSpec("PEllipsis", default=...)
PAnother = ParamSpec("PAnother", default=P)
PAnotherWithDefault = ParamSpec("PAnotherWithDefault", default=PList)
```

```py
class ParamSpecWithDefault1(Generic[PList]):
    attr: Callable[PList, None]

reveal_type(ParamSpecWithDefault1().attr)  # revealed: (int, str, /) -> None
reveal_type(ParamSpecWithDefault1[[int]]().attr)  # revealed: (int, /) -> None
```

```py
class ParamSpecWithDefault2(Generic[PEllipsis]):
    attr: Callable[PEllipsis, None]

reveal_type(ParamSpecWithDefault2().attr)  # revealed: (...) -> None
reveal_type(ParamSpecWithDefault2[[int, str]]().attr)  # revealed: (int, str, /) -> None
```

```py
class ParamSpecWithDefault3(Generic[P, PAnother]):
    attr1: Callable[P, None]
    attr2: Callable[PAnother, None]

# `P` hasn't been specialized, so it defaults to `Unknown` gradual form
p1 = ParamSpecWithDefault3()
reveal_type(p1.attr1)  # revealed: (...) -> None
reveal_type(p1.attr2)  # revealed: (...) -> None

p2 = ParamSpecWithDefault3[[int, str]]()
reveal_type(p2.attr1)  # revealed: (int, str, /) -> None
reveal_type(p2.attr2)  # revealed: (int, str, /) -> None

p3 = ParamSpecWithDefault3[[int], [str]]()
reveal_type(p3.attr1)  # revealed: (int, /) -> None
reveal_type(p3.attr2)  # revealed: (str, /) -> None

class ParamSpecWithDefault4(Generic[PList, PAnotherWithDefault]):
    attr1: Callable[PList, None]
    attr2: Callable[PAnotherWithDefault, None]

p1 = ParamSpecWithDefault4()
reveal_type(p1.attr1)  # revealed: (int, str, /) -> None
reveal_type(p1.attr2)  # revealed: (int, str, /) -> None

p2 = ParamSpecWithDefault4[[int]]()
reveal_type(p2.attr1)  # revealed: (int, /) -> None
reveal_type(p2.attr2)  # revealed: (int, /) -> None

p3 = ParamSpecWithDefault4[[int], [str]]()
reveal_type(p3.attr1)  # revealed: (int, /) -> None
reveal_type(p3.attr2)  # revealed: (str, /) -> None

# Un-ordered type variables as the default of `PAnother` is `P`
# error: [invalid-generic-class]
# error: [invalid-generic-class]
class ParamSpecWithDefault5(Generic[PAnother, P]):
    attr: Callable[PAnother, None]

# PAnother has default as P (another ParamSpec) which is not in scope
# error: [invalid-generic-class]
class ParamSpecWithDefault6(Generic[PAnother]):
    attr: Callable[PAnother, None]
```

## Semantics

The semantics of `ParamSpec` are described in
[the PEP 695 `ParamSpec` document](./../pep695/paramspec.md) to avoid duplication unless there are
any behavior specific to the legacy `ParamSpec` implementation.

### Binding contexts

A nested definition can use a `ParamSpec` from an enclosing class or function without adding it to
its own generic context:

```py
from typing import Callable, Generic, ParamSpec
from ty_extensions._internal import generic_context

P = ParamSpec("P")

class C(Generic[P]):
    def method(self, *args: P.args, **kwargs: P.kwargs): ...

# revealed: ty_extensions._internal.GenericContext[Self@method]
reveal_type(generic_context(C.method))

def outer(_: Callable[P, None]):
    def inner(_: Callable[P, None]): ...

    reveal_type(generic_context(inner))  # revealed: None
```
