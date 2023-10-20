#: Okay
# 情
#: W291:1:6
print 
#: W293:2:1
class Foo(object):
    
    bang = 12
#: W291:2:35
'''multiline
string with trailing whitespace'''   
#: W291 W292 noeol
x = 1   
#: W191 W292 noeol
if False:
	pass  # indented with tabs
#: W292:1:36 noeol
# This line doesn't have a linefeed
#: W292:1:5 E225:1:2 noeol
1+ 1
#: W292:1:27 E261:1:12 noeol
import this # no line feed
#: W292:3:22 noeol
class Test(object):
    def __repr__(self):
        return 'test'
