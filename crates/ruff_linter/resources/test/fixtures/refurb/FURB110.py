z = x if x else y  # FURB110

z = x \
    if x else y  # FURB110

z = x if x \
    else  \
        y  # FURB110

z = x() if x() else y()  # FURB110

# FURB110
z = x if (
    # Test for x.
    x
) else (
    # Test for y.
    y
)

# FURB110
z = (
    x if (
        # Test for x.
        x
    ) else (
        # Test for y.
        y
    )
)

# FURB110
z = (
    x if
    # If true, use x.
    x
    # Otherwise, use y.
    else
    y
)

# FURB110
z = (
    x
    if x
    else y
    if y > 0
    else None
)
