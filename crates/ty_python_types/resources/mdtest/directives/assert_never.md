# `assert_never`

## Basic functionality

`assert_never` makes sure that the type of the argument is `Never`.

### Correct usage

```py
from typing_extensions import assert_never, Never, Any
from ty_extensions import Unknown

def _(never: Never):
    assert_never(never)  # fine
```

### Diagnostics

<!-- snapshot-diagnostics -->

If it is not, a `type-assertion-failure` diagnostic is emitted.

```py
from typing_extensions import assert_never, Never, Any
from ty_extensions import Unknown

def _():
    assert_never(0)  # error: [type-assertion-failure]

def _():
    assert_never("")  # error: [type-assertion-failure]

def _():
    assert_never(None)  # error: [type-assertion-failure]

def _():
    assert_never(())  # error: [type-assertion-failure]

def _(flag: bool, never: Never):
    assert_never(1 if flag else never)  # error: [type-assertion-failure]

def _(any_: Any):
    assert_never(any_)  # error: [type-assertion-failure]

def _(unknown: Unknown):
    assert_never(unknown)  # error: [type-assertion-failure]
```

## Use case: Type narrowing and exhaustiveness checking

```toml
[environment]
python-version = "3.10"
```

`assert_never` can be used in combination with type narrowing as a way to make sure that all cases
are handled in a series of `isinstance` checks or other narrowing patterns that are supported.

```py
from typing_extensions import assert_never, Literal

class A: ...
class B: ...
class C: ...

def if_else_isinstance_success(obj: A | B):
    if isinstance(obj, A):
        pass
    elif isinstance(obj, B):
        pass
    elif isinstance(obj, C):
        pass
    else:
        assert_never(obj)

def if_else_isinstance_error(obj: A | B):
    if isinstance(obj, A):
        pass
    # B is missing
    elif isinstance(obj, C):
        pass
    else:
        # error: [type-assertion-failure] "Type `B & ~A & ~C` is not equivalent to `Never`"
        assert_never(obj)

def if_else_singletons_success(obj: Literal[1, "a"] | None):
    if obj == 1:
        pass
    elif obj == "a":
        pass
    elif obj is None:
        pass
    else:
        assert_never(obj)

def if_else_singletons_error(obj: Literal[1, "a"] | None):
    if obj == 1:
        pass
    elif obj is "A":  # "A" instead of "a"
        pass
    elif obj is None:
        pass
    else:
        # error: [type-assertion-failure] "Type `Literal["a"]` is not equivalent to `Never`"
        assert_never(obj)

def match_singletons_success(obj: Literal[1, "a"] | None):
    match obj:
        case 1:
            pass
        case "a":
            pass
        case None:
            pass
        case _ as obj:
            assert_never(obj)

def match_singletons_error(obj: Literal[1, "a"] | None):
    match obj:
        case 1:
            pass
        case "A":  # "A" instead of "a"
            pass
        case None:
            pass
        case _ as obj:
            # TODO: We should emit an error here, but the message should
            # show the type `Literal["a"]` instead of `@Todo(…)`. We only
            # assert on the first part of the message because the `@Todo`
            # message is not available in release mode builds.
            # error: [type-assertion-failure] "Type `@Todo"
            assert_never(obj)
```
