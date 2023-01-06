# Bad
if a:
    b = c
else:
    b = d

# Good
b = c if a else d

# https://github.com/MartinThoma/flake8-simplify/issues/115
if a:
    b = c
elif c:
    b = a
else:
    b = d
