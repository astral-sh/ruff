import re
import regex

# Errors
re.compile('single free-spacing', flags=re.X)
re.findall('si\ngle')
re.finditer("dou\ble")
re.fullmatch('''t\riple single''')
re.match("""\triple double""")
re.search('two', 'args')
re.split("raw", r'second')
re.sub(u'''nicode''', u"f(?i)rst")
re.subn(b"""ytes are""", f"\u006e")

regex.compile('single free-spacing', flags=regex.X)
regex.findall('si\ngle')
regex.finditer("dou\ble")
regex.fullmatch('''t\riple single''')
regex.match("""\triple double""")
regex.search('two', 'args')
regex.split("raw", r'second')
regex.sub(u'''nicode''', u"f(?i)rst")
regex.subn(b"""ytes are""", f"\u006e")

regex.template("""(?m)
    (?:ulti)?
    (?=(?<!(?<=(?!l)))
    l(?i:ne)
""", flags = regex.X)


# No errors
re.compile(R'uppercase')
re.findall(not_literal)
re.finditer(0, literal_but_not_string)
re.fullmatch()  # no first argument
re.match('string' f'''concatenation''')
re.search(R"raw" r'concatenation')
re.split(rf"multiple", f"""lags""")
re.sub(FR'ee', '''as in free speech''')
re.subn(br"""eak your machine with rm -""", rf"""/""")

regex.compile(R'uppercase')
regex.findall(not_literal)
regex.finditer(0, literal_but_not_string)
regex.fullmatch()  # no first argument
regex.match('string' f'''concatenation''')
regex.search(R"raw" r'concatenation')
regex.split(rf"multiple", f"""lags""")
regex.sub(FR'ee', '''as in free speech''')
regex.subn(br"""eak your machine with rm -""", rf"""/""")

regex.splititer(both, non_literal)
regex.subf(f, lambda _: r'means', '"format"')
regex.subfn(fn, f'''a$1n't''', lambda: "'function'")


# https://github.com/astral-sh/ruff/issues/16713
re.compile("\a\f\n\r\t\u27F2\U0001F0A1\v\x41")  # with unsafe fix
re.compile("\b")  # without fix
re.compile("\"")  # without fix
re.compile("\'")  # without fix
re.compile('\"')  # without fix
re.compile('\'')  # without fix
re.compile("\\")  # without fix
re.compile("\101")  # without fix
re.compile("a\
b")  # without fix
