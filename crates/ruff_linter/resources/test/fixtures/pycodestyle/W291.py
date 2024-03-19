'''trailing whitespace 
inside a multiline string'''

f'''trailing whitespace 
inside a multiline f-string'''

# Trailing whitespace after `{`
f'abc { 
    1 + 2
}'

# Trailing whitespace after `2`
f'abc {
    1 + 2 
}'
