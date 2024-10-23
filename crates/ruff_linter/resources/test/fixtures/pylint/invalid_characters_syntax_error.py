# These test cases contain syntax errors. The characters within the unterminated
# strings shouldn't be highlighted.

# Before any syntax error
b = ''
# Unterminated string
b = '
b = ''
# Unterminated f-string
b = f'
b = f''
# Implicitly concatenated
b = '' f'' '
