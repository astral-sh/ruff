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

# case 1:
# try:
# try:  # with comment
# try: print()
# except:
# except Foo:
# except Exception as e: print(e)


# Script tag without an opening tag (Error)

# requires-python = ">=3.11"
# dependencies = [
#   "requests<3",
#   "rich",
# ]
# ///

# Script tag (OK)

# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "requests<3",
#   "rich",
# ]
# ///

# Script tag without a closing tag (OK)

# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "requests<3",
#   "rich",
# ]
