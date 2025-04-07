def simple_match():
    match bar:
        case This:
            print("this")
        case That:
            print("that")
    print("after")
