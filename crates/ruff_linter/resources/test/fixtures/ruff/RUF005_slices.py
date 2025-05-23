foo = [4, 5, 6]
bar = [1, 2, 3] + foo
slicing1 = foo[:1] + [7, 8, 9]
slicing2 = [7, 8, 9] + bar[1:]
slicing3 = foo[:1] + [7, 8, 9] + bar[1:]
indexing = foo[0] + [7, 8, 9] + bar[1]  # Not changed; looks a little suspect for concatenation.
