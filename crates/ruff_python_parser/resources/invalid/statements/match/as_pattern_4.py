match subject:
    # This `as` pattern is unparenthesized so the parser never takes the path
    # where it might be confused as a mapping key pattern.
    case {x as y: 1}:
        pass
