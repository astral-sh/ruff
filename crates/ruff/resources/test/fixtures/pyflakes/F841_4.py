def my_func(var):
    match var:
        case "foo":
            print("blah1")
        case "bar":
            print("blah2")
        case other:
            print("invalid blah")


my_func("boo")
