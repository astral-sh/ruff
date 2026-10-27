# Display of tuple types

## Shared elements

Each assignment reuses the preceding tuple type four times. An error message can abbreviate the
inner tuples once printing them in full would repeat too many elements.

```py
def f(n: int):
    a = (n, n, n, n)
    b = (a, a, a, a)
    c = (b, b, b, b)
    d = (c, c, c, c)

    d()  # error: [call-non-callable] "tuple[...]"
    with d:  # error: [invalid-context-manager] "tuple[...]"
        pass
```

## A tuple with one nested child at each level

Each level adds two elements, so the complete type still fits in the message.

```py
def chain(n: int):
    a = (n, n)
    b = (n, a)
    c = (n, b)
    d = (n, c)
    e = (n, d)
    f = (n, e)
    g = (n, f)
    h = (n, g)
    # revealed: tuple[int, tuple[int, tuple[int, tuple[int, tuple[int, tuple[int, tuple[int, tuple[int, int]]]]]]]]
    reveal_type(h)
```
