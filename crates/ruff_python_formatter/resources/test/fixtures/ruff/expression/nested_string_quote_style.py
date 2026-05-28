# Nested string literals inside interpolated string expressions follow either
# alternating or preferred quote normalization depending on
# nested-string-quote-style.

# Nested string literals.
f'{ "nested" }'
t'{ "nested" }'

# Multiple levels of nested interpolated strings.
f'level 1 {f"level 2"}'
t'level 1 {f"level 2"}'
f'''level 1 {f"level 2 {f'level 3'}"}'''
t'''level 1 {f"level 2 {f'level 3'}"}'''
f'level 1 {f"level 2 {f'level 3'}"}'  # syntax error pre-3.12
f'level 1 {f"level 2 {f'level 3 {f"level 4"}'}"}'  # syntax error pre-3.12

# Nested string literals with equal specifiers (debug expressions).
f'{ "nested" = }'
t'{ "nested" = }'
f"{10 + len('bar')=}"
t"{10 + len('bar')=}"

# Escape minimization.
f'"double" quotes and {"nested string"}'
t'"double" quotes and {"nested string"}'
f"'single' quotes and {'nested string'}"
t"'single' quotes and {'nested string'}"
f'"double" quotes and {"nested string with \"double\" quotes"}'  # syntax error pre-3.12
t'"double" quotes and {"nested string with \"double\" quotes"}'
f"'single' quotes and {'nested string with \'single\' quotes'}'"  # syntax error pre-3.12
t"'single' quotes and {'nested string with \'single\' quotes'}'"
f'"double" quotes and {"nested string with 'single' quotes"}'  # syntax error pre-3.12
t'"double" quotes and {"nested string with 'single' quotes"}'
f"'single' quotes and {'nested string with "double" quotes'}'"  # syntax error pre-3.12
t"'single' quotes and {'nested string with "double" quotes'}'"

# Nested strings in lists and dictionaries.
f'{ ["1", "2"] }'
t'{ ["1", "2"] }'
f'{ {"key": [{"inner": "value"}]} }'
t'{ {"key": [{"inner": "value"}]} }'

# Triple quotes and escaped quotes.
f'''{ "'single'" }'''
t'''{ "'single'" }'''
f'''{ '"double"' }'''
t'''{ '"double"' }'''
f''''single' { "'single'" }'''
t''''single' { "'single'" }'''
f'''"double" { '"double"' }'''
t'''"double" { '"double"' }'''
f''''single' { '"double"' }'''
t''''single' { '"double"' }'''
f'''"double" { "'single'" }'''
t'''"double" { "'single'" }'''

# Triple quotes and nested f-strings.
f"{f'''{'nested'} inner'''} outer"
t"{t'''{'nested'} inner'''} outer"

# Outer implicit concatenation.
f'{ "implicit " }' f'{ "concatenation" }'
t'{ "implicit " }' t'{ "concatenation" }'

# Outer implicit concatenation with escaped quotes.
f'"double" quotes and { "implicit " }' f'{ "concatenation" } with "double" quotes'
t'"double" quotes and { "implicit " }' t'{ "concatenation" } with "double" quotes'
f'\'single\' quotes and { "implicit " }' f'{ "concatenation" } with "double" quotes'
t'\'single\' quotes and { "implicit " }' t'{ "concatenation" } with "double" quotes'
f'"double" quotes and { "implicit " }' f'{ "concatenation" } with \'single\' quotes'
t'"double" quotes and { "implicit " }' t'{ "concatenation" } with \'single\' quotes'
f'\'single\' quotes and { "implicit " }' f'{ "concatenation" } with \'single\' quotes'
t'\'single\' quotes and { "implicit " }' t'{ "concatenation" } with \'single\' quotes'

# Inner implicit concatenation.
f'{ ("implicit " "concatenation", ["more", "strings"]) }'
t'{ ("implicit " "concatenation", ["more", "strings"]) }'

# Inner implicit concatenation with escaped quotes.
f'{ ("implicit " "concatenation", ["'single'", "\"double\""]) }'
t'{ ("implicit " "concatenation", ["'single'", "\"double\""]) }'
