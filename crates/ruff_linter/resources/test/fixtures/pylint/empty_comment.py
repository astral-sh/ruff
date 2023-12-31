# this line has a non-empty comment and is OK
    # this line is also OK, but the three following lines are not
#
    #
        #

# this non-empty comment has trailing whitespace and is OK

# Many codebases use multiple `#` characters on a single line to visually
# separate sections of code, so we don't consider these empty comments.

##########################################

# trailing `#` characters are not considered empty comments ###


def foo():  # this comment is OK, the one below is not
    pass  #


# the lines below have no comments and are OK
def bar():
    pass


# "Empty comments" are common in block comments
# to add legibility. For example:
#
# The above line's "empty comment" is likely
# intentional and is considered OK.


# lines in multi-line strings/comments whose last non-whitespace character is a `#`
# do not count as empty comments
"""
The following lines are all fine:
#
    #
        #
"""

# These should be removed, despite being an empty "block comment".

#
#

# These should also be removed.

x = 1

#
##
#

# This should be removed.

α = 1
α#
