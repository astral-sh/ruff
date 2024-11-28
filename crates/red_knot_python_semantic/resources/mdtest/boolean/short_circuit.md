# Short-Circuit Evaluation

## Not all boolean expressions must be evaluated

In `or` expressions, if the left-hand side is truthy, the right-hand side is not evaluated.
Similarly, in `and` expressions, if the left-hand side is falsy, the right-hand side is not
evaluated.

```py
def bool_instance() -> bool:
    return True

if bool_instance() or (x := 1):
    # error: [possibly-unresolved-reference]
    reveal_type(x)  # revealed: Literal[1]

if bool_instance() and (x := 1):
    # error: [possibly-unresolved-reference]
    reveal_type(x)  # revealed: Literal[1]
```

## First expression is always evaluated

```py
def bool_instance() -> bool:
    return True

if (x := 1) or bool_instance():
    reveal_type(x)  # revealed: Literal[1]

if (x := 1) and bool_instance():
    # TODO
    reveal_type(x)  # revealed: Never
```

## Statically known truthiness

```py
if True or (x := 1):
    # TODO: infer that the second arm is never executed, and raise `unresolved-reference`.
    # error: [possibly-unresolved-reference]
    reveal_type(x)  # revealed: Never

if True and (x := 1):
    # TODO: infer that the second arm is always executed, do not raise a diagnostic
    # TODO
    # error: [possibly-unresolved-reference]
    reveal_type(x)  # revealed: Never
```

## Later expressions can always use variables from earlier expressions

```py
def bool_instance() -> bool:
    return True

bool_instance() or (x := 1) or reveal_type(x)  # revealed: Literal[1]

# error: [unresolved-reference]
bool_instance() or reveal_type(y) or (y := 1)  # revealed: Unknown
```

## Nested expressions

```py
def bool_instance() -> bool:
    return True

if bool_instance() or ((x := 1) and bool_instance()):
    # error: [possibly-unresolved-reference]
    reveal_type(x)  # revealed: Literal[1]

if ((y := 1) and bool_instance()) or bool_instance():
    reveal_type(y)  # revealed: Literal[1]

# error: [possibly-unresolved-reference]
if (bool_instance() and (z := 1)) or reveal_type(z):  # revealed: Literal[1]
    # error: [possibly-unresolved-reference]
    reveal_type(z)  # revealed: Literal[1]
```
