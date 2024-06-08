match (1, 2)  # Identifier
match (1, 2):  # Keyword
    case _: ...
match [1:]  # Identifier
match [1, 2]:  # Keyword
    case _: ...
match * foo  # Identifier
match - foo  # Identifier
match -foo:  # Keyword
    case _: ...
