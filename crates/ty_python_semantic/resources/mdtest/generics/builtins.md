# Generic builtins

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
class dict[K, V, Extra]: ...
```

`/typeshed/stdlib/typing_extensions.pyi`:

```pyi
def reveal_type(obj, /): ...
```

If we don't, then we won't be able to infer the types of variadic keyword arguments correctly.

```py
def f(**kwargs):
    reveal_type(kwargs)  # revealed: Unknown

def g(**kwargs: int):
    reveal_type(kwargs)  # revealed: Unknown
```
