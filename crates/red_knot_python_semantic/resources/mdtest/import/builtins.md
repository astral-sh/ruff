# Builtins

## Importing builtin module

Builtin symbols can be explicitly imported:

```py
import builtins

reveal_type(builtins.chr)  # revealed: Literal[chr]
```

## Implicit use of builtin

Or used implicitly:

```py
reveal_type(chr)  # revealed: Literal[chr]
reveal_type(str)  # revealed: Literal[str]
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

## Unknown builtin (later defined)

`foo` has a type of `Unknown` in this example, as it relies on `bar` which has not been defined at
that point:

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
reveal_type(foo)  # revealed: Unknown
```
