# Positive cases

counter = 0


def count():
    global counter
    nonlocal counter
    counter += 1


def count():
    counter = 0

    def count(counter_type):
        if counter_type == "nonlocal":
            nonlocal counter
            counter += 1
        else:
            global counter
            counter += 1


def count():
    counter = 0

    def count_twice():
        for i in range(2):
            nonlocal counter
            counter += 1
        global counter


def count():
    nonlocal counter
    global counter
    counter += 1


# Negative cases

counter = 0


def count():
    global counter
    counter += 1


def count():
    counter = 0

    def count_local():
        nonlocal counter
        counter += 1


def count():
    counter = 0

    def count_local():
        nonlocal counter
        counter += 1

    def count_global():
        global counter
        counter += 1
