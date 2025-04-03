# List subscripts

## Indexing into lists

A list can be indexed into with:

- numbers
- slices

```py
x = [1, 2, 3]
reveal_type(x)  # revealed: list

# TODO reveal int
reveal_type(x[0])  # revealed: @Todo(return type of overloaded function)

# TODO reveal list
reveal_type(x[0:1])  # revealed: @Todo(return type of overloaded function)

# TODO error
reveal_type(x["a"])  # revealed: @Todo(return type of overloaded function)
```

## Assignments within list assignment

In assignment, we might also have a named assignment. This should also get type checked.

```py
x = [1, 2, 3]
x[0 if (y := 2) else 1] = 5

# TODO error? (indeterminite index type)
x["a" if (y := 2) else 1] = 6

# TODO error (can't index via string)
x["a" if (y := 2) else "b"] = 6
```
