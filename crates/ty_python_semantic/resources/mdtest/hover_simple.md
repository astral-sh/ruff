# Simple hover test

Testing basic hover functionality with simple cases.

```py
# Test 1: Simple variable with longer name
my_value = 10
#↓ hover: Literal[10]
my_value

# Test 2: Try hovering directly on the number literal
#          ↓ hover: Literal[10]
some_var = 10

# Test 3: Variable reference with longer name
another_var = 42
#↓ hover: Literal[42]
another_var
```
