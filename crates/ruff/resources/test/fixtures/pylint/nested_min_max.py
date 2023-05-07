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
