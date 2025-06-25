# Errors.

if 1 in set([1]):
    print("Single-element set")

if 1 in set((1,)):
    print("Single-element set")

if 1 in set({1}):
    print("Single-element set")

if 1 in frozenset([1]):
    print("Single-element set")

if 1 in frozenset((1,)):
    print("Single-element set")

if 1 in frozenset({1}):
    print("Single-element set")

if 1 in set(set([1])):
    print('Recursive solution')



# Non-errors.

if 1 in set((1, 2)):
    pass

if 1 in set([1, 2]):
    pass

if 1 in set({1, 2}):
    pass

if 1 in frozenset((1, 2)):
    pass

if 1 in frozenset([1, 2]):
    pass

if 1 in frozenset({1, 2}):
    pass

if 1 in set(1,):
    pass

if 1 in set(1,2):
    pass

if 1 in set((x for x in range(2))):
    pass
