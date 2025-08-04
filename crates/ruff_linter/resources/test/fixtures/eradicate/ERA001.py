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

# Script tag with multiple closing tags (OK)
# /// script
# [tool.uv]
# extra-index-url = ["https://pypi.org/simple", """\
# https://example.com/
# ///
# """
# ]
# ///
print(1)

# Script tag without a closing tag (Error)

# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "requests<3",
#   "rich",
# ]

# Script tag block followed by normal block (Ok)

# /// script
# # https://github.com/astral-sh/ruff/issues/15321
# requires-python = ">=3.12"
# dependencies = [
#   "requests<3",
#   "rich",
# ]
# ///
#
# Foobar


# Regression tests for https://github.com/astral-sh/ruff/issues/19713

# mypy: ignore-errors
# pyright: ignore-errors
# pyrefly: ignore-errors
# ty: ignore[unresolved-import]
# pyrefly: ignore[unused-import]

print(1)
