a if True else b
f() if x else None
a if b else c if d else e
1 + x if 1 < 0 else -1
a and b if x else False
x <= y if y else x
True if a and b else False
1, 1 if a else c

# Lambda is allowed in orelse expression
x if True else lambda y: y

# These test expression are only allowed when parenthesized
x if (yield x) else y
x if (yield from x) else y
x if (lambda x: x) else y

# Split across multiple lines
(x
if y
else z)