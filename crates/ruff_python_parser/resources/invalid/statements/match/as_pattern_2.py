match subject:
    # This `as` pattern is unparenthesied so the parser never takes the path
    # where it might be confused as a complex literal pattern.
    case x as y + 1j:
        pass
