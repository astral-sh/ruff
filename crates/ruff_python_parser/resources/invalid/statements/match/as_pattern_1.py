match subject:
    #             Parser shouldn't confuse this as being a
    #             complex literal pattern
    #             v
    case (x as y) + 1j:
    #     ^^^^^^
    #    as-pattern
        pass
