a, b, c = 3, 4,       5
if (
    a ==    3
    and b    != 9  # fmt: skip
    and c is not None
):
    print("I'm good!")
else:
    print("I'm bad")
