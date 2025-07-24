# Cases that should trigger the violation
x = 5
result1 = x | x
y = 10
result2 = y & y
z = 15
result3 = z ^ z
a = 20
result4 = a - a
b = 25
result5 = b / b
c = 30
result6 = c // c
d = 35
result7 = d % d

# Different variable names
value = 42
test1 = value | value
test2 = value & value
test3 = value ^ value
test4 = value - value
test5 = value / value
test6 = value // value
test7 = value % value

# Complex expressions
complex_expr = (x + 1) | (x + 1)
complex_and = (y * 2) & (y * 2)
complex_xor = (z - 3) ^ (z - 3)
complex_sub = (a + b) - (a + b)
complex_div = (c * d) / (c * d)
complex_floor = (value + 1) // (value + 1)
complex_mod = (x + y) % (x + y)

# Literals
literal_or = 5 | 5 
literal_and = 10 & 10
literal_xor = 15 ^ 15
literal_sub = 20 - 20
literal_div = 25 / 25
literal_floor = 30 // 30
literal_mod = 35 % 35

# Cases that should NOT trigger the violation
different_values = 5 | 10  # OK
different_vars = x | y  # OK
same_operator = x + x  # OK
multiplication = x * x  # OK
power = x ** x  # OK
shift_left = x << x  # OK
shift_right = x >> x  # OK
bool_or = True | True
bool_and = False & False
bool_xor = True ^ True
