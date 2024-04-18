# Simple
1 + 2
1 - 2
1 * 2
1 / 2
1 // 2
1 % 2
1 ** 2
1 | 2
1 ^ 2
1 & 2
1 >> 2
1 << 2
1 @ 2

# Same precedence
1 + 2 - 3 + 4
1 * 2 / 3 // 4 @ 5 % 6
1 << 2 >> 3 >> 4 << 5

# Different precedence
1 + 2 * 3
1 * 2 + 3
1 ** 2 * 3 - 4 @ 5 + 6 - 7 // 8
# With bitwise operators
1 | 2 & 3 ^ 4 + 5 @ 6 << 7 // 8 >> 9

# Associativity
1 + (2 + 3) + 4
1 + 2 + (3 + 4 + 5)

# Addition with a unary plus
x ++ y
