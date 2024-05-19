#: W605:1:10
regex = '\.png$'

#: W605:2:1
regex = '''
\.png$
'''

#: W605:2:6
f(
    '\_'
)

#: W605:4:6
"""
multi-line
literal
with \_ somewhere
in the middle
"""

#: W605:1:38
value = 'new line\nand invalid escape \_ here'


def f():
    #: W605:1:11
    return'\.png$'

#: Okay
regex = r'\.png$'
regex = '\\.png$'
regex = r'''
\.png$
'''
regex = r'''
\\.png$
'''
s = '\\'
regex = '\w'  # noqa
regex = '''
\w
'''  # noqa

regex = '\\\_'

#: W605:1:7
u'foo\ bar'

#: W605:1:13
(
    "foo \
    bar \. baz"
)

#: W605:1:6
"foo \. bar \t"

#: W605:1:13
"foo \t bar \."
