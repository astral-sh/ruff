# Simple
a == b
b < a
b > a
a >= b
a <= b
a != b
a is c
a in b
a not in c
a is not b

# Double operator mixed
a not in b is not c not in d not in e is not f

# Precedence check
a | b < c | d not in e & f
#     ^       ^^^^^^
#     Higher precedence than bitwise operators

# unary `not` is higher precedence, but is allowed at the start of the expression
# but not anywhere else
not x not in y

x or y not in z and a
x == await y
x is not await y

# All operators have the same precedence
a < b == c > d is e not in f is not g <= h >= i != j
