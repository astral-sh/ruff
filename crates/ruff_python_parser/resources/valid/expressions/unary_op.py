# Simple
-1
+1
~1
not x

# Multiple
---1
-+~1
not-+~1
not not x

# Precedence check
- await 1
+ await 1 ** -2
~(1, 2)
-1 + 2

# Precedence check for `not` operator because it is higher than other unary operators
not a and b or not c | d and not e
not (x := 1)
not a | (not b)
