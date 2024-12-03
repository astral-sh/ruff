import re


### Errors
re.escape('normal')
re.escape("normal' 'double")
re.escape('''tri
ple''')
re.escape("""triple \
double""")
re.escape(u'normal')
re.escape(u"normal double")
re.escape(r'aw')
re.escape(b'ytes')
re.escape(br'aw bytes')

re.escape('co\n' r"cate" 'natio\u006E')
re.escape(br"ytes" b'''\x63\x6F\x6E\x63\x61\x74\x65\x6E\x61\x74\x69\x6F\x6E''')


### No errors
re.escape(non_literal)
re.escape()  # no_arguments
re.escape()

re.escape(r"abc.")
re.escape(r"^abc")
re.escape(r"abc$")
re.escape(r"abc*")
re.escape(r"abc+")
re.escape(r"abc?")
re.escape(r"abc{2,3}")
re.escape(r"abc|def")
re.escape(r"(a)bc")
re.escape(r"a\bc")
