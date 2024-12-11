# Nonlocal references

## One level up

```py
def f():
    x = 1
    def g():
        reveal_type(x)  # revealed: Literal[1]
```

## Two levels up

```py
def f():
    x = 1
    def g():
        def h():
            reveal_type(x)  # revealed: Literal[1]
```

## Skips class scope

```py
def f():
    x = 1

    class C:
        x = 2
        def g():
            reveal_type(x)  # revealed: Literal[1]
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
            reveal_type(x)  # revealed: Literal[1]
```
