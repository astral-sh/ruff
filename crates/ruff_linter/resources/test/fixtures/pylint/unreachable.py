def say_hello():
    return True
    print("Hello World!, Outside function.")  # PLW0101


def branched_hello():
    if True:
        return
        print("Hello World!, Inside if.")  # PLW0101
    else:
        print("Hello World!, Inside else.")
    return
    print("Hello World!")
    print("blah!")  # PLW0101


def branched_hello2():
    if True:
        print("Hello World!, Inside if.")  # PLW0101
    else:
        raise
        print("Hello World!, Inside else.")  # PLW0101
    print("Hello World!")  # PLW0101


def branched_hello3():
    return
    if True:
        print("Hello World!, Inside if.")
    else:
        raise
        print("Hello World!, Inside else.")
    print("Hello World!")  # PLW0101


def branched_hello4():
    def nested():
        if True:
            return
            print("Hello World!, Inside nested.")
        else:
            print("Hello World!, Inside else.")
        return
        print("Hello World!, Inside nested.")

    return
    print("Hello World!, Outside nested.")  # PLW0101


def many_returns():
    return
    return
    return
    return
    return
    return  # PLW0101


def ok():  # OK
    print("Hello World!")
    return


def ok_raise():  # OK
    print("Raising!")
    raise


class _NoValueType:  # OK
    __instance = None

    def __new__(cls):
        if not cls.__instance:
            cls.__instance = super().__new__(cls)
        return cls.__instance
