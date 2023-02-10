# combine-if-conditions (SIM114)

Derived from the **flake8-simplify** linter.

### What it does
Checks if consecutive `if` branches have the same body.

### Why is this bad?
These branches can be combine using the python `or` statement

### Example
```if x = 1:
    print("Hello")
elif x = 2:
    print("Hello")
```

Use instead:
```if x = 1 or x = 2
    print("Hello")
```