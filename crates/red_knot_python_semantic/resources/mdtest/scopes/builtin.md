# Builtin scope

## Conditionally global or builtin

If a builtin name is conditionally defined as a global, a name lookup should union the builtin type
with the conditionally-defined type:

```py
def returns_bool() -> bool:
    return True

if returns_bool():
    chr = 1

def f():
    reveal_type(chr)  # revealed: Literal[chr] | Literal[1]
```

## Conditionally global or builtin, with annotation

Same is true if the name is annotated:

```py
def returns_bool() -> bool:
    return True

if returns_bool():
    chr: int = 1

def f():
    reveal_type(chr)  # revealed: Literal[chr] | int
```
