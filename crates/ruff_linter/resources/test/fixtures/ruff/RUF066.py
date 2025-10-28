# Test cases for RUF066: Detect inefficient membership tests

# =====================================================================
# VIOLATIONS: Complex elements that prevent LOAD_CONST optimization
# =====================================================================

# Nested containers in lists/tuples
if item in [[1, 2], [3, 4]]:  # RUF066
    pass

if item in [{1}, {2}, {3}]:  # RUF066
    pass

if item in [{"a": 1}, {"b": 2}]:  # RUF066
    pass

if item in ({1, 2}, {3}):  # RUF066
    pass

# Function calls
if item in [func(), func2()]:  # RUF066
    pass

if item in {func(), other()}:  # RUF066
    pass

# Names/variables
x = 10
if item in [1, 2, x]:  # RUF066
    pass

# Operations with names
if item in [x + 2, x * 3]:  # RUF066
    pass

if item in {x + 2, x * 3}:  # RUF066
    pass

if item in [1 + 2, x + 2]:  # RUF066
    pass

# Lambdas
if item in [lambda x: x, lambda y: y]:  # RUF066
    pass

# Boolean operations (need parentheses)
a = 1
b = 2
c = 3
d = 4
if item in [a or b, c and d]:  # RUF066
    pass

# Comparison expressions (need parentheses)
y = 3
if item in [y < 10, y > 0]:  # RUF066
    pass

# F-strings with interpolation
if item in [f"hello {x}", f"world {y}"]:  # RUF066
    pass

# 'not in' operator (also triggers)
if item not in [[1], [2]]:  # RUF066
    pass

# Multiple violations in one expression
result = item in [[1], [2]] or item in [[3], [4]]  # RUF066 (both)

if item in [[1], [2]] and item in [[3], [4]]:  # RUF066 (both)
    pass

# Many complex elements
if item in [[1], [2], [3], [4], [5], [6], [7], [8], [9], [10], [11]]:  # RUF066
    pass


# =====================================================================
# OK: Simple elements that ARE optimized
# =====================================================================

# Literals: numbers, strings, None, booleans
if item in [1, 2, 3]:  # OK
    pass

if item in (1, 2, 3):  # OK
    pass

if item in {1, 2, 3}:  # OK
    pass

if item in ["foo", "bar", "baz"]:  # OK
    pass

# Tuples of simple values (optimized by Python)
if item in {(1, 2), (3, 4)}:  # OK
    pass

if item in ((1, 2), (3, 4)):  # OK
    pass

# Operations on literals (const-folded by Python)
if item in [1 + 2, 3 * 4, 5 - 1]:  # OK
    pass

if item in {1 + 2, 3 * 4}:  # OK
    pass

if item in [-5, +3, ~0]:  # OK
    pass

# F-strings without interpolation
if item in [f"hello", f"world"]:  # OK
    pass

if item in [f"a" f"b"]:  # OK
    pass

# Boolean operations on literals (const-folded by Python)
if item in [2.0 or True]:  # OK
    pass

if item in [1 and 2, True or False]:  # OK
    pass

# Complex number literals
if item in [1+2j, 3+4j]:  # OK
    pass


# =====================================================================
# OK: Not literal containers (out of scope)
# =====================================================================

# Variable reference
items = [[1, 2], [3, 4]]
if key in items:  # OK
    pass

# Function call
if item in get_items():  # OK
    pass

# Comprehension
if item in [x for x in range(10)]:  # OK
    pass


# =====================================================================
# OK: Wrong operator or container type
# =====================================================================

# Not membership test
if item == [1, 2]:  # OK
    pass

if item < [1, 2]:  # OK
    pass

# Empty containers
if item in []:  # OK
    pass

if item in ():  # OK
    pass

if item in set():  # OK
    pass

# String/bytes are single constants, not container literals
if char in "abc":  # OK
    pass

if byte in b"abc":  # OK
    pass
