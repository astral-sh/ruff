x += 1
x.y += (1, 2, 3)
x[y] += (1, 2, 3)

# All possible augmented assignment tokens
x += 1
x -= 1
x *= 1
x /= 1
x //= 1
x %= 1
x **= 1
x &= 1
x |= 1
x ^= 1
x <<= 1
x >>= 1
x @= 1

# Mixed
a //= (a + b) - c ** 2