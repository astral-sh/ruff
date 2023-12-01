# Errors.

if 1 in (1,):
    print("Single-element tuple")

if 1 in [1]:
    print("Single-element list")

if 1 in {1}:
    print("Single-element set")

if "a" in "a":
    print("Single-element string")

if 1 not in (1,):
    print("Check `not in` membership test")

if not 1 in (1,):
    print("Check the negated membership test")

# Non-errors.

if 1 in (1, 2):
    pass

if 1 in [1, 2]:
    pass

if 1 in {1, 2}:
    pass

if "a" in "ab":
    pass

if 1 == (1,):
    pass

if 1 > [1]:
    pass

if 1 is {1}:
    pass

if "a" == "a":
    pass

if 1 in {*[1]}:
    pass
