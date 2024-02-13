some_dict = {"a": 12, "b": 32, "c": 44}


def f():
    for _, value in some_dict.items():  # PERF102
        print(value)


def f():
    for key, _ in some_dict.items():  # PERF102
        print(key)


def f():
    for weird_arg_name, _ in some_dict.items():  # PERF102
        print(weird_arg_name)


def f():
    for name, (_, _) in some_dict.items():  # PERF102
        print(name)


def f():
    for name, (value1, _) in some_dict.items():  # OK
        print(name, value1)


def f():
    for (key1, _), (_, _) in some_dict.items():  # PERF102
        print(key1)


def f():
    for (_, (_, _)), (value, _) in some_dict.items():  # PERF102
        print(value)


def f():
    for (_, key2), (value1, _) in some_dict.items():  # OK
        print(key2, value1)


def f():
    for ((_, key2), (value1, _)) in some_dict.items():  # OK
        print(key2, value1)


def f():
    for ((_, key2), (_, _)) in some_dict.items():  # PERF102
        print(key2)


def f():
    for (_, _, _, variants), (r_language, _, _, _) in some_dict.items():  # OK
        print(variants, r_language)


def f():
    for (_, _, (_, variants)), (_, (_, (r_language, _))) in some_dict.items():  # OK
        print(variants, r_language)


def f():
    for key, value in some_dict.items():  # OK
        print(key, value)


def f():
    for _, value in some_dict.items(12):  # OK
        print(value)


def f():
    for key in some_dict.keys():  # OK
        print(key)


def f():
    for value in some_dict.values():  # OK
        print(value)


def f():
    for name, (_, _) in (some_function()).items():  # PERF102
        print(name)


def f():
    for name, (_, _) in (some_function().some_attribute).items():  # PERF102
        print(name)


def f():
    for name, unused_value in some_dict.items():  # PERF102
        print(name)


def f():
    for unused_name, value in some_dict.items():  # PERF102
        print(value)


# Regression test for: https://github.com/astral-sh/ruff/issues/7097
def _create_context(name_to_value):
    for(B,D)in A.items():
        if(C:=name_to_value.get(B.name)):A.run(B.set,C)

def f():
    key = 2
    print(key)

    for key, value in some_dict.items():  # PERF102
        print(value)


def f():
    value = 2
    print(value)

    for key, value in some_dict.items():  # PERF102
        print(key)


def f():
    key = 2
    print(key)

    for key, value in some_dict.items():  # OK
        print(key)
        print(value)


def f():
    key = 2
    print(key)

    for key, value in some_dict.items():  # OK
        print(value)

    print(key)


def f():
    value = 2
    print(value)

    for key, value in some_dict.items():  # OK
        print(key)

    print(value)


def f():
    another_dict = {"e": 1, "f": 2, "g": 3}

    for key, value in some_dict.items():  # PERF102
        for key_2, value_2 in another_dict.items():
            print(key_2)
            print(value_2)
            print(value)


def f():
    another_dict = {"e": 1, "f": 2, "g": 3}

    for key, value in some_dict.items():  # PERF102
        for key_2, value_2 in another_dict.items():
            print(key)
            print(value_2)
            print(value)


def f():
    another_dict = {"e": 1, "f": 2, "g": 3}
    key_2 = 2
    t = [key_2, 3]

    for key, value in some_dict.items():  # OK
        for key_2, value_2 in another_dict.items():
            print(key)
            print(key_2)
        print(value)

    print(value_2)
