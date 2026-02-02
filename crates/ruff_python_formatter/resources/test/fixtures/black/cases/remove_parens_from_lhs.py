# Remove unnecessary parentheses from LHS of assignments


def a():
    return [1, 2, 3]


# Single variable with unnecessary parentheses
(b) = a()[0]

# Tuple unpacking with unnecessary parentheses
(c, *_) = a()

# These should not be changed - parentheses are necessary
(d,) = a()  # single-element tuple
e = (1 + 2) * 3  # RHS has precedence needs
