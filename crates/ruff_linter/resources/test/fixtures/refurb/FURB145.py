l = [1, 2, 3, 4, 5]

# Errors.
a = l[:]
b, c = 1, l[:]
d, e = l[:], 1
m = l[::]
l[:]
print(l[:])

# False negatives.
aa = a[:]  # Type inference.

# OK.
t = (1, 2, 3, 4, 5)
f = t[:]  # t.copy() is not supported.
g = l[1:3]
h = l[1:]
i = l[:3]
j = l[1:3:2]
k = l[::2]
