#: E261:1:5
pass # an inline comment
#: E262:1:12
x = x + 1  #Increment x
#: E262:1:12
x = x + 1  #  Increment x
#: E262:1:12
x = y + 1  #:  Increment x
#: E265:1:1
#Block comment
a = 1
#: E265:2:1
m = 42
#! This is important
mx = 42 - 42
#: E266:3:5 E266:6:5
def how_it_feel(r):

    ### This is a variable ###
    a = 42

    ### Of course it is unused
    return
#: E265:1:1 E266:2:1
##if DEBUG:
##    logging.error()
#: W291:1:42
#########################################
#:

#: Okay
#!/usr/bin/env python

pass  # an inline comment
x = x + 1   # Increment x
y = y + 1   #: Increment x

# Block comment
a = 1

# Block comment1

# Block comment2
aaa = 1


# example of docstring (not parsed)
def oof():
    """
    #foo not parsed
    """

    ####################################################################
    #                            A SEPARATOR                           #
    ####################################################################

    # ################################################################ #
    # ####################### another separator ###################### #
    # ################################################################ #
#: E262:3:9
# -*- coding: utf8 -*-
#  (One space one NBSP) Ok for block comment
a = 42  #  (One space one NBSP)
#: E262:2:9
#  (Two spaces) Ok for block comment
a = 42  #  (Two spaces)
