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

If it is not, a `type-assertion-failure` diagnostic is emitted.

```py
from typing_extensions import assert_never, Never, Any
from ty_extensions import Unknown

def _():
    assert_never(0)  # snapshot: type-assertion-failure
```

```snapshot
error[type-assertion-failure]: Argument does not have asserted type `Never`
 --> src/mdtest_snippet.py:5:5
  |
5 |     assert_never(0)  # snapshot: type-assertion-failure
  |     ^^^^^^^^^^^^^-^
  |                  |
  |                  Inferred type of argument is `Literal[0]`
  |
info: `Never` and `Literal[0]` are not equivalent types
```

```py
def _():
    assert_never("")  # snapshot: type-assertion-failure
```

```snapshot
error[type-assertion-failure]: Argument does not have asserted type `Never`
 --> src/mdtest_snippet.py:7:5
  |
7 |     assert_never("")  # snapshot: type-assertion-failure
  |     ^^^^^^^^^^^^^--^
  |                  |
  |                  Inferred type of argument is `Literal[""]`
  |
info: `Never` and `Literal[""]` are not equivalent types
```

```py
def _():
    assert_never(None)  # snapshot: type-assertion-failure
```

```snapshot
error[type-assertion-failure]: Argument does not have asserted type `Never`
 --> src/mdtest_snippet.py:9:5
  |
9 |     assert_never(None)  # snapshot: type-assertion-failure
  |     ^^^^^^^^^^^^^----^
  |                  |
  |                  Inferred type of argument is `None`
  |
info: `Never` and `None` are not equivalent types
```

```py
def _():
    assert_never(())  # snapshot: type-assertion-failure
```

```snapshot
error[type-assertion-failure]: Argument does not have asserted type `Never`
  --> src/mdtest_snippet.py:11:5
   |
11 |     assert_never(())  # snapshot: type-assertion-failure
   |     ^^^^^^^^^^^^^--^
   |                  |
   |                  Inferred type of argument is `tuple[()]`
   |
info: `Never` and `tuple[()]` are not equivalent types
```

```py
def _(flag: bool, never: Never):
    assert_never(1 if flag else never)  # snapshot: type-assertion-failure
```

```snapshot
error[type-assertion-failure]: Argument does not have asserted type `Never`
  --> src/mdtest_snippet.py:13:5
   |
13 |     assert_never(1 if flag else never)  # snapshot: type-assertion-failure
   |     ^^^^^^^^^^^^^--------------------^
   |                  |
   |                  Inferred type of argument is `Literal[1]`
   |
info: `Never` and `Literal[1]` are not equivalent types
```

```py
def _(any_: Any):
    assert_never(any_)  # snapshot: type-assertion-failure
```

```snapshot
error[type-assertion-failure]: Argument does not have asserted type `Never`
  --> src/mdtest_snippet.py:15:5
   |
15 |     assert_never(any_)  # snapshot: type-assertion-failure
   |     ^^^^^^^^^^^^^----^
   |                  |
   |                  Inferred type of argument is `Any`
   |
info: `Never` and `Any` are not equivalent types
```

```py
def _(unknown: Unknown):
    assert_never(unknown)  # snapshot: type-assertion-failure
```

```snapshot
error[type-assertion-failure]: Argument does not have asserted type `Never`
  --> src/mdtest_snippet.py:17:5
   |
17 |     assert_never(unknown)  # snapshot: type-assertion-failure
   |     ^^^^^^^^^^^^^-------^
   |                  |
   |                  Inferred type of argument is `Unknown`
   |
info: `Never` and `Unknown` are not equivalent types
```

### Return type of `assert_never`

The return type of `assert_never` is always `Never`, despite the type of the argument:

```py
from typing_extensions import Never, assert_never

def _(never: Never):
    # revealed: Never
    reveal_type(assert_never(never))

def _():
    # revealed: Never
    reveal_type(assert_never(0))  # error: [type-assertion-failure]
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
