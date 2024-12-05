# Nonlocal references

## One level up

```py
def f():
    x = 1
    def g():
        y = x
        reveal_type(y)  # revealed: Literal[1]
```

## Two levels up

```py
def f():
    x = 1
    def g():
        def h():
            y = x
            reveal_type(y)  # revealed: Literal[1]
```

## Skips class scope

```py
def f():
    x = 1
    class C:
        x = 2
        def g():
            y = x
            reveal_type(y)  # revealed: Literal[1]
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
            y = x
            reveal_type(y)  # revealed: Literal[1]
```
