#import os
# from foo import junk
#a = 3
a = 4
#foo(1, 2, 3)

def foo(x, y, z):
    content = 1 # print('hello')
    print(x, y, z)

    # This is a real comment.
    # # This is a (nested) comment.
    #return True
    return False

#import os  # noqa: ERA001


class A():
    pass
    # b = c


dictionary = {
    # "key1": 123,  # noqa: ERA001
    # "key2": 456,
    # "key3": 789,  # test
}

#import os  # noqa
