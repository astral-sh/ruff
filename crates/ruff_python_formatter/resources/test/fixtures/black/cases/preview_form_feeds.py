# Warning! This file contains form feeds (ASCII 0x0C, often represented by \f or ^L).
# These may be invisible in your editor: ensure you can see them before making changes here.

# There's one at the start that'll get stripped

# Comment and statement processing is different enough that we'll test variations of both
# contexts here

#


#


#



#



#



#


#



#

#
        
#

\
#
pass

pass


pass


pass



pass



pass



pass


pass



pass

pass
        
pass


# form feed after a dedent
def foo():
    pass

pass


# form feeds are prohibited inside blocks, or on a line with nonwhitespace
defbar(a=1,b:bool=False):

    
    pass


class Baz:

    def __init__(self):
        pass
    
    
    def something(self):
        pass
    


# 
pass
pass #
a = 1
#
pass
a = 1

a = [

]

# as internal whitespace of a comment is allowed but why
"form feed literal in a string is okay"

# form feeds at the very end get removed.
