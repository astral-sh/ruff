# if-to-dict (SIM116)

Derived from the **flake8-simplify** linter.

Autofix is always available.

### What it does
Checks for three or more consective if-statements with direct returns

### Why is this bad?
These can be simplified by using a dictionary

### Example
```if x = 1:
    return "Hello"
elif x = 2:
    return "Goodbye"
else:
   return "Goodnight"
```

Use instead:
`return {1: "Hello", 2: "Goodbye"}.get(x, "Goodnight")`