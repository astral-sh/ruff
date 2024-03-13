x = 1
y = 2

x if x > y else y  # FURB136

x if x >= y else y  # FURB136

x if x < y else y  # FURB136

x if x <= y else y  # FURB136

y if x > y else x  # FURB136

y if x >= y else x  # FURB136

y if x < y else x  # FURB136

y if x <= y else x  # FURB136

x + y if x > y else y  # OK

x if (
    x
    > y
) else y  # FURB136
