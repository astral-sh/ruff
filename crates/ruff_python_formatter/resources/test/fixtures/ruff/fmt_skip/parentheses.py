if (
    # a leading condition comment
    len([1,   23, 3, 4, 5]) > 2 # trailing condition comment
    # trailing own line comment
): # fmt: skip
    pass


if ( # trailing open parentheses comment
    # a leading condition comment
    len([1, 23, 3, 4, 5]) > 2
)   and ((((y)))): # fmt: skip
    pass


if ( # trailing open parentheses comment
    # a leading condition comment
    len([1, 23, 3, 4, 5]) > 2
) and   y: # fmt: skip
    pass
