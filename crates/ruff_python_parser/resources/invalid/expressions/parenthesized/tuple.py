# Test cases for tuple expressions where the parser recovers from a syntax error.

(,)

(1,,2)

(1,,)

# Missing comma
(1 2)

# Dictionary element in a list
(1: 2)

# Missing expression
(1, x + )

(1; 2)

# Unparenthesized named expression is not allowed
x, y := 2, z