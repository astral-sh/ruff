some_dict = {
    "a": 12,
    "b": 32,
    "c": 44
}

for _, value in some_dict.items():  # W8120
    print(value)


for key, _ in some_dict.items():  # W8120
    print(key)


for key, value in some_dict.items():  # OK
    print(key, value)


for key in some_dict.keys():  # OK
    print(key)


for value in some_dict.values():  # OK
    print(value)


class Foo:
    def items(self):
        return [("some_key", 12), ("some_other_key", 34)]


foo = Foo()

for _, val in foo.items(): # OK
    print(val)
