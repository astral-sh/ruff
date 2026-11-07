# Unnecessary parentheses in unpacking should be removed.

# For loop unpacking
points = [(1, 2), (3, 4)]
for (x), (y) in points:
    print(x, y)

for (x), y in points:
    pass

for x, (y) in points:
    pass

for ((a), (b)) in points:
    pass

for ((a), b) in points:
    pass

for (a, (b)) in points:
    pass

for (((a)), ((b))) in points:
    pass

for a, (b, c) in points:
    pass

for a, (b,) in points:
    pass

for a, [b, c] in points:
    pass

for a, ((b, c)) in points:
    pass

for a, ((b)) in points:
    pass

# Assignment unpacking
(a), (b) = 1, 2
a, (b) = 1, 2
[(a), (b)] = [1, 2]
((a), (b)) = 1, 2
((a), b) = 1, 2
(a, (b)) = 1, 2
(((a)), ((b))) = 1, 2
a, (b, (c)) = 1, (2, 3)
(a), (b) = (c), (d) = 1, 2
((a), (b)) = ((c), (d)) = 1, 2

# Comprehensions
[x for (a), (b) in points]
[x for ((a), (b)) in points]
{k: v for (a), (b) in points}
(x for (a), (b) in points)
{x for (a), (b) in points}

# With statement
with lock as ((a), (b)):
    pass

with c1 as ((a), (b)), c2 as ((c), (d)):
    pass

with lock as ((a), b):
    pass

with lock as (a, (b)):
    pass

# Delete statement
del (a), (b)
del ((a), (b))
del ((a), b)
del (a, (b))
del (((a)), ((b)))

# Star expression unpacking
*(a), (b) = 1, 2
*((a)), (b) = 1, 2
*(((a))), ((b)) = 1, 2
(*((a)), (b)) = 1, 2
(*((a)), ((b))) = 1, 2
