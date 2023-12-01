call(
    # Leading starred comment
    * # Trailing star comment
    [
        # Leading value comment
        [What, i, this, s, very, long, aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa]
    ] # trailing value comment
)

call(
    # Leading starred comment
    * ( # Leading value comment
        [What, i, this, s, very, long, aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa]
    ) # trailing value comment
)

call(
    x,
    # Leading starred comment
    * # Trailing star comment
    [
        # Leading value comment
        [What, i, this, s, very, long, aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa]
    ] # trailing value comment
)

call(
    x,
    * # Trailing star comment
    (  # Leading value comment
        y
    )
)
