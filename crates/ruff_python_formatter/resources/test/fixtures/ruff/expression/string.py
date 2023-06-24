"' test"
'" test'

"\" test"
'\' test'

# Prefer single quotes for string with more double quotes
"' \" \" '' \" \" '"

# Prefer double quotes for string with more single quotes
'\' " " \'\' " " \''

# Prefer double quotes for string with equal amount of single and double quotes
'" \' " " \'\''
"' \" '' \" \""

"\\' \"\""
'\\\' ""'


u"Test"
U"Test"

r"Test"
R"Test"

'This string will not include \
backslashes or newline characters.'

if True:
    'This string will not include \
        backslashes or newline characters.'

"""Multiline
String \"
"""

'''Multiline
String \'
'''

'''Multiline
String ""
'''

'''Multiline
String """
'''

'''Multiline
String "'''

"""Multiline
String '''
"""

"""Multiline
String '"""

'''Multiline
String \"\"\"
'''
