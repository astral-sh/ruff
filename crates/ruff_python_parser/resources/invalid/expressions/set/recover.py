# Test cases for set expressions where the parser recovers from a syntax error.
# There are valid expressions in between invalid ones to verify that.
# These are same as for the list expressions.

{,}

{1,,2}

{1,,}

# Missing comma
{1 2}

# Dictionary element in a list
{1: 2}

# Missing expression
{1, x + }

{1; 2}

[*]
