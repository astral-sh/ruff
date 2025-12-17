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

## Builtins imported from custom project-level stubs

The project can add or replace builtins with the `__builtins__.pyi` stub. They will take precedence
over the typeshed ones.

```py
reveal_type(foo)  # revealed: int
reveal_type(bar)  # revealed: str
reveal_type(quux(1))  # revealed: int
b = baz  # error: [unresolved-reference]

reveal_type(ord(100))  # revealed: bool
a = ord("a")  # error: [invalid-argument-type]

bar = int(123)
reveal_type(bar)  # revealed: int
```

`__builtins__.pyi`:

```pyi
foo: int = ...
bar: str = ...

def quux(value: int) -> int: ...

unused: str = ...

def ord(x: int) -> bool: ...
```

Builtins stubs are searched relative to the project root, not the file using them.

`under/some/folder.py`:

```py
reveal_type(foo)  # revealed: int
reveal_type(bar)  # revealed: str
```

## Assigning custom builtins

```py
import builtins

builtins.foo = 123
builtins.bar = 456  # error: [unresolved-attribute]
builtins.baz = 789  # error: [invalid-assignment]
builtins.chr = lambda x: str(x)  # error: [invalid-assignment]
builtins.chr = 10
```

`__builtins__.pyi`:

```pyi
foo: int
baz: str
chr: int
```
