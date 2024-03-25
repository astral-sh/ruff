match subject:
    #            Parser shouldn't confuse this as being a
    #            class pattern
    #            v
    case (x as y)(a, b):
    #     ^^^^^^
    #    as-pattern
        pass
