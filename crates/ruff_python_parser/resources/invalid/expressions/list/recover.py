# Test cases for list expressions where the parser recovers from a syntax error.

[,]

[1,,2]

[1,,]

# Missing comma
[1 2]

# Dictionary element in a list
[1: 2]

# Missing expression
[1, x + ]

[1; 2]

[*]
