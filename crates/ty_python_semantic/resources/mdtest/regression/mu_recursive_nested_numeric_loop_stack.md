# Recursive nested numeric loop stack overflow

```toml
[environment]
python-version = "3.13"
```

This is minimized from a SymPy ecosystem stack overflow. A nested loop can update several
loop-carried numeric values, then return to the outer loop through either a narrowed `continue`
edge or a normal tuple assignment. The recovered recursive state must converge.

```py
def sympy_nested_numeric_loop(a, b):
    while a:
        x, y = a, b
        A, B, C, D = 1, 0, 0, 1

        while True:
            q = x + y
            x_qy, B_qD = x - q * y, B - q * D
            x, y = y, x_qy
            A, B, C, D = C, D, A - q * C, B_qD
            if y:
                break

        if B == 0:
            a, b = b, a % b
            continue

        a, b = A * a + B * b, C * a + D * b

    return a
```
