min(1, 2, 3)
min(1, min(2, 3))
min(1, min(2, min(3, 4)))
min(1, foo("a", "b"), min(3, 4))
min(1, max(2, 3))
max(1, 2, 3)
max(1, max(2, 3))
max(1, max(2, max(3, 4)))
max(1, foo("a", "b"), max(3, 4))

# These should not trigger; we do not flag cases with keyword args.
min(1, min(2, 3), key=test)
min(1, min(2, 3, key=test))
# This will still trigger, to merge the calls without keyword args.
min(1, min(2, 3, key=test), min(4, 5))

# Don't provide a fix if there are comments within the call.
min(
    1,  # This is a comment.
    min(2, 3),
)

# Handle iterable expressions.
min(1, min(a))
min(1, min(i for i in range(10)))
max(1, max(a))
max(1, max(i for i in range(10)))

tuples_list = [
    (1, 2),
    (2, 3),
    (3, 4),
    (4, 5),
    (5, 6),
]

min(min(tuples_list))
max(max(tuples_list))

# Starred argument should be copied as it is.
max(1, max(*a))

import builtins
builtins.min(1, min(2, 3))
