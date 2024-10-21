# Valid
x = 1 if True else 2

# Invalid
x = 1 if True else 1

# Invalid
x = "a" if True else "a"

# Invalid
x = 0.1 if False else 0.1

# Invalid (may contain side effects from dunder methods, so no autofixes given)
x = 1 if x > 0.2 else 1
x = 2 if x == 1 else 2
x = 3 if f(x) else 3
