# Similar to `./fstring_format_spec_1.py` except that the continuation character itself
# is escaped. So, the lexer should be moved back.

f'hello {x:\\
    'world'}'