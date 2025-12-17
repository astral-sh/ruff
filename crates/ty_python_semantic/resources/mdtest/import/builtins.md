# Builtins

## Importing builtin module

Builtin symbols can be explicitly imported:

```py
import builtins

reveal_type(builtins.chr)  # revealed: def chr(i: SupportsIndex, /) -> str
```

## Implicit use of builtin

Or used implicitly:

```py
reveal_type(chr)  # revealed: def chr(i: SupportsIndex, /) -> str
reveal_type(str)  # revealed: <class 'str'>
```

## Builtin symbol from custom typeshed

If we specify a custom typeshed, we can use the builtin symbol from it, and no longer access the
builtins from the "actual" vendored typeshed:

```toml
[environment]
typeshed = "/typeshed"
```

`/typeshed/stdlib/builtins.pyi`:

```pyi
class Custom: ...

custom_builtin: Custom
```

`/typeshed/stdlib/typing_extensions.pyi`:

```pyi
def reveal_type(obj, /): ...
```

```py
reveal_type(custom_builtin)  # revealed: Custom

# error: [unresolved-reference]
reveal_type(str)  # revealed: Unknown
```

## Forward reference in builtins stub

In stub files, forward references are allowed in all expressions, so `foo` correctly resolves to the
type of `bar` even though `bar` is defined later:

```toml
[environment]
typeshed = "/typeshed"
```

`/typeshed/stdlib/builtins.pyi`:

```pyi
foo = bar
bar = 1
```

`/typeshed/stdlib/typing_extensions.pyi`:

```pyi
def reveal_type(obj, /): ...
```

```py
reveal_type(foo)  # revealed: Literal[1]
```
