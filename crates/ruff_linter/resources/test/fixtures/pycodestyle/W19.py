	'''File starts with a tab
	multiline string with tab in it'''

#: W191
if False:
	print  # indented with 1 tab
#:


#: W191
y = x == 2 \
	or x == 3
#: E101 W191 W504
if (
        x == (
            3
        ) or
        y == 4):
	pass
#: E101 W191
if x == 2 \
    or y > 1 \
        or x == 3:
	pass
#: E101 W191
if x == 2 \
        or y > 1 \
        or x == 3:
	pass
#:

#: E101 W191 W504
if (foo == bar and
        baz == bop):
	pass
#: E101 W191 W504
if (
    foo == bar and
    baz == bop
):
	pass
#:

#: E101 E101 W191 W191
if start[1] > end_col and not (
        over_indent == 4 and indent_next):
	return (0, "E121 continuation line over-"
	        "indented for visual indent")
#:

#: E101 W191


def long_function_name(
        var_one, var_two, var_three,
        var_four):
	print(var_one)
#: E101 W191 W504
if ((row < 0 or self.moduleCount <= row or
     col < 0 or self.moduleCount <= col)):
	raise Exception("%s,%s - %s" % (row, col, self.moduleCount))
#: E101 E101 E101 E101 W191 W191 W191 W191 W191 W191
if bar:
	return (
	    start, 'E121 lines starting with a '
	    'closing bracket should be indented '
	    "to match that of the opening "
	    "bracket's line"
	)
#
#: E101 W191 W504
# you want vertical alignment, so use a parens
if ((foo.bar("baz") and
     foo.bar("bop")
     )):
	print("yes")
#: E101 W191 W504
# also ok, but starting to look like LISP
if ((foo.bar("baz") and
     foo.bar("bop"))):
	print("yes")
#: E101 W191 W504
if (a == 2 or
    b == "abc def ghi"
         "jkl mno"):
	return True
#: E101 W191 W504
if (a == 2 or
    b == """abc def ghi
jkl mno"""):
	return True
#: W191:2:1 W191:3:1 E101:3:2
if length > options.max_line_length:
	return options.max_line_length, \
	    "E501 line too long (%d characters)" % length


#
#: E101 W191 W191 W504
if os.path.exists(os.path.join(path, PEP8_BIN)):
	cmd = ([os.path.join(path, PEP8_BIN)] +
	       self._pep8_options(targetfile))
#: W191 - okay
'''
	multiline string with tab in it'''
#: E101 (W191 okay)
'''multiline string
	with tabs
   and spaces
'''
#: Okay
'''sometimes, you just need to go nuts in a multiline string
	and allow all sorts of crap
  like mixed tabs and spaces

or trailing whitespace
or long long long long long long long long long long long long long long long long long lines
'''  # nopep8
#: Okay
'''this one
	will get no warning
even though the noqa comment is not immediately after the string
''' + foo  # noqa
#
#: E101 W191
if foo is None and bar is "bop" and \
        blah == 'yeah':
	blah = 'yeahnah'


#
#: W191 W191 W191
if True:
	foo(
		1,
		2)
#: W191 W191 W191 W191 W191
def test_keys(self):
	"""areas.json - All regions are accounted for."""
	expected = set([
		u'Norrbotten',
		u'V\xe4sterbotten',
	])
#: W191
x = [
	'abc'
]
#: W191 - okay
'''	multiline string with tab in it, same lines'''
"""	here we're using '''different delimiters'''"""
'''
	multiline string with tab in it, different lines
'''
"	single line string with tab in it"

f"test{
	tab_indented_should_be_flagged
}	<- this tab is fine"

f"""test{
	tab_indented_should_be_flagged
}	<- this tab is fine"""
