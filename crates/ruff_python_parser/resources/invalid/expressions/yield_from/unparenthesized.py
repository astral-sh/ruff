# Unparenthesized named expression
yield from x := 1

# Unparenthesized tuple expression
yield from x, y

# This is a tuple expression parsing
#          vvvvvvvvvvvvv
yield from (x, *x and y)