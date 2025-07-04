# Nonlocal references

## One level up

```py
def f():
    x = 1
    def g():
        reveal_type(x)  # revealed: Unknown | Literal[1]
```

## Two levels up

```py
def f():
    x = 1
    def g():
        def h():
            reveal_type(x)  # revealed: Unknown | Literal[1]
```

## Skips class scope

```py
def f():
    x = 1

    class C:
        x = 2
        def g():
            reveal_type(x)  # revealed: Unknown | Literal[1]
```

## Skips annotation-only assignment

```py
def f():
    x = 1
    def g():
        # it's pretty weird to have an annotated assignment in a function where the
        # name is otherwise not defined; maybe should be an error?
        x: int
        def h():
            reveal_type(x)  # revealed: Unknown | Literal[1]
```

## The `nonlocal` keyword

```py
def f():
    x = 1
    def g():
        x += 1  # error: [unresolved-reference]
```

Without the `nonlocal` keyword, `x += 1` (or `x = x + 1` or `x = foo(x)`) is not allowed in an inner
scope like this. It might look like it would read the outer `x` and write to the inner `x`, but it
actually tries to read the not-yet-initialized inner `x` and raises `UnboundLocalError` at runtime.

```py
def f():
    x = 1
    def g():
        nonlocal x
        x += 1
```

## Late `nonlocal` declarations

Using a name prior to its `nonlocal` declaration in the same scope is a syntax error.

```py
def f():
    x = 1
    def g():
        print(x)
        nonlocal x  # error: [invalid-syntax] "name `x` is used prior to nonlocal declaration"
```

## TODO: get the write to land in the right scope

The first reveal passes, but the second one fails, I think because we apply `x = 2` to `g`'s scope
rather than to `f`'s.

```py
def f():
    x = 1
    def g():
        nonlocal x
        x += 1
        reveal_type(x)  # revealed: Literal[2]
    reveal_type(x)  # revealed: Literal[2]
```

## TODO: forbid bogus `nonlocal` declarations

`nonlocal x` shouldn't be allowed when there is no `x`.

```py
def f():
    def g():
        nonlocal x  # error: [something should fail here]
```

## TODO: unusual `nonlocal` ordering

`nonlocal x` should work even if `x` isn't bound until later.

```py
def f():
    def g():
        def h():
            nonlocal x
            print(x)
        reveal_type(h())  # error: [something should fail here too?]
        x = 1
        reveal_type(h())  # revealed: Literal[1]
```
