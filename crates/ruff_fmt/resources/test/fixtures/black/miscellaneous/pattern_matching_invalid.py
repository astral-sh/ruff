# First match, no errors
match something:
    case bla():
        pass

# Problem on line 10
match invalid_case:
    case valid_case:
        pass
    case a := b:
        pass
    case valid_case:
        pass

# No problems either
match something:
    case bla():
        pass
