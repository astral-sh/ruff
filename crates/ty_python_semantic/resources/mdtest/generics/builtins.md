# Generic builtins

## Unbound inherited methods

Methods inherited by `list` and `dict` can be called through the unsubscripted class, with an
instance passed explicitly as the receiver. Their implicit `Self` bounds use the class's default
type arguments, just as methods declared directly on the class do.

```py
def clear_containers(items: list[int], mapping: dict[str, int]) -> None:
    list.clear(items)
    list.reverse(items)
    dict.clear(mapping)

    list.append(items, 1)
    dict.__setitem__(mapping, "a", 1)

    list[int].clear(items)
    dict[str, int].clear(mapping)

    list[int].clear(["a"])  # error: [invalid-argument-type]
    dict[str, int].clear({"a": "b"})  # error: [invalid-argument-type]
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
