# Generic builtins

## Unbound inherited methods

In typeshed, `list` inherits `clear` from `MutableSequence`, and `dict` inherits it from
`MutableMapping`. We can call these methods through `list` and `dict` without supplying type
arguments.

```py
def clear_containers(items: list[int], mapping: dict[str, int]) -> None:
    list.clear(items)
    dict.clear(mapping)
```

## Variadic keyword arguments with a custom `dict`

When we define `dict` in a custom typeshed, we must take care to define it as a generic class in the
same way as in the real typeshed.

```toml
[environment]
typeshed = "/typeshed"
```

`/typeshed/stdlib/builtins.pyi`:

```pyi
class object: ...
class int: ...
class tuple: ...
class dict[K, V, Extra]: ...
```

`/typeshed/stdlib/typing_extensions.pyi`:

```pyi
def reveal_type(obj, /): ...
```

If we don't, then we may get "surprising" results when inferring the types of variadic keyword
arguments.

```py
def f(**kwargs):
    reveal_type(kwargs)  # revealed: dict[Unknown, Unknown, Unknown]

def g(**kwargs: int):
    reveal_type(kwargs)  # revealed: dict[Unknown, Unknown, Unknown]
```
