# `extra-paths` must not let user-supplied `typing.py` shadow typeshed

Regression test for ty issue #3169. When `--extra-search-path` points at a directory containing a
CPython runtime `typing.py` (which happens whenever the runtime stdlib is reachable via `PYTHONPATH`
or similar), ty resolved `from typing import Any` to that file rather than the bundled typeshed
`typing.pyi`. The runtime declares `class Any(metaclass=_AnyMeta): ...`, a regular class, so `Any`
annotations stopped acting as the dynamic type and `Literal[X]` arguments were rejected as not
assignable.

Root cause: `extra_paths` enter `SearchPaths::static_paths`, which `SearchPathIterator::next` (in
`crates/ty_module_resolver/src/resolve.rs`) iterates before `stdlib_path`. Fix: treat `typing` as
non-shadowable in `ModuleResolveMode::is_non_shadowable`, alongside the existing `types` and
`typing_extensions` exceptions.

```toml
[environment]
extra-paths = ["/runtime_lib"]
```

`/runtime_lib/typing.py`:

```py
class _AnyMeta(type): ...

class Any(metaclass=_AnyMeta):
    """Special type indicating an unconstrained type."""
```

`main.py`:

```py
from typing import Any

# Must resolve to the typeshed special form, not the user class above.
reveal_type(Any)  # revealed: <special-form 'typing.Any'>

def takes_any(**kwargs: Any) -> None: ...

# `Literal["openai/gpt-4o"]` is assignable to `Any`. No error.
takes_any(model="openai/gpt-4o")

def boom() -> None:
    # `BaseException.__new__` takes `*args: Any` per typeshed. No error.
    raise ValueError("oops")

def newline() -> str:
    # `chr` takes `SupportsIndex`; `int` (and `Literal[10]`) implements it. No error.
    return chr(10)
```
