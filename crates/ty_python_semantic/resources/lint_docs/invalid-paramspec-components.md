## What it does

Checks that `P.args` and `P.kwargs` annotate a function's variadic positional and keyword
parameters, respectively. Both components must come from the same `ParamSpec` and appear together
with no keyword-only parameters between them.

## Why is this bad?

A `ParamSpec` captures a callable's full parameter list. A caller can often pass the same argument
by position or by keyword. A wrapper needs both components to forward either call correctly.
Inserting a keyword-only parameter between the components can also intercept a keyword intended for
the wrapped callable.

## Examples

```toml
[environment]
python-version = "3.12"
```

```python
from collections.abc import Callable


def call[**P](
    f: Callable[P, int],
    /,
    *args: P.args,
    debug: bool,  # error
    **kwargs: P.kwargs,
) -> int:
    if debug:  # "debug" could also be a keyword argument to f
        print(f)
    return f(*args, **kwargs)
```

Make `debug` positional-only so the caller can pass `debug=` through to `f`:

```python
from collections.abc import Callable


def call[**P](
    f: Callable[P, int], debug: bool, /, *args: P.args, **kwargs: P.kwargs
) -> int:
    if debug:
        print(f)
    return f(*args, **kwargs)
```

## References

- [Typing specification: The components of a ParamSpec](https://typing.python.org/en/latest/spec/generics.html#the-components-of-a-paramspec)

## Rule history

- v0.0.102: Split from `invalid-paramspec` and `invalid-type-form`.
