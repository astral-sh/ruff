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
elatement = ("we all say", ) + yay()
excitement = ("we all think", ) + Fun().yay()
astonishment = ("we all feel", ) + Fun.words

chain = ['a', 'b', 'c'] + eggs + list(('yes', 'no', 'pants') + zoob)

baz = () + zoob

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

[] + foo + [
]

[] + foo + [  # This will be preserved, but doesn't prevent the fix
]
