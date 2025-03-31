###
# Non-fixable Errors.
###
foo + [  # This will be preserved.
]
[*foo] + [  # This will be preserved.
]
first = [
    # The order
    1,  # here
    2,  # is
    # extremely
    3,  # critical
    # to preserve
]
second = first + [
    # please
    4,
    # don't
    5,
    # touch
    6,
]


###
# Fixable errors.
###
class Fun:
    words = ("how", "fun!")

    def yay(self):
        return self.words


yay = Fun().yay

foo = [4, 5, 6]
bar = [1, 2, 3] + foo
zoob = tuple(bar)
quux = (7, 8, 9) + zoob
spam = quux + (10, 11, 12)
spom = list(spam)
eggs = spom + [13, 14, 15]
elatement = ("we all say",) + yay()
excitement = ("we all think",) + Fun().yay()
astonishment = ("we all feel",) + Fun.words

chain = ["a", "b", "c"] + eggs + list(("yes", "no", "pants") + zoob)

baz = () + zoob

[] + foo + [
]

pylint_call = [sys.executable, "-m", "pylint"] + args + [path]
pylint_call_tuple = (sys.executable, "-m", "pylint") + args + (path, path2)
b = a + [2, 3] + [4]

# Uses the non-preferred quote style, which should be retained.
f"{a() + ['b']}"

###
# Non-errors.
###
a = (1,) + [2]
a = [1, 2] + (3, 4)
a = ([1, 2, 3] + b) + (4, 5, 6)
