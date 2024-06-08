# Double star means that the parser will try to parse it as a dictionary expression but
# it's actually a comprehension.

{**x: y for x, y in data}

# TODO(dhruvmanila): This test case fails because there's no way to represent `**y`
# in the AST. The parser tries to parse it as a binary expression but the range isn't
# correct.
# {x: **y for x, y in data}
