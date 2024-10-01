#: E121
print("E121", (
  "dent"))
#: E122
print("E122", (
"dent"))
#: E123
my_list = [
    1, 2, 3,
    4, 5, 6,
    ]
#: E124
print("E124", ("visual",
               "indent_two"
              ))
#: E124
print("E124", ("visual",
               "indent_five"
))
#: E124
a = (123,
)
#: E129 W503
if (row < 0 or self.moduleCount <= row
    or col < 0 or self.moduleCount <= col):
    raise Exception("%s,%s - %s" % (row, col, self.moduleCount))
#: E126
print("E126", (
            "dent"))
#: E126
print("E126", (
        "dent"))
#: E127
print("E127", ("over-",
                  "over-indent"))
#: E128
print("E128", ("visual",
    "hanging"))
#: E128
print("E128", ("under-",
              "under-indent"))
#:


#: E126
my_list = [
    1, 2, 3,
    4, 5, 6,
     ]
#: E121
result = {
   'key1': 'value',
   'key2': 'value',
}
#: E126 E126
rv.update(dict.fromkeys((
    'qualif_nr', 'reasonComment_en', 'reasonComment_fr',
    'reasonComment_de', 'reasonComment_it'),
          '?'),
          "foo")
#: E126
abricot = 3 + \
          4 + \
          5 + 6
#: E131
print("hello", (

    "there",
     # "john",
    "dude"))
#: E126
part = set_mimetype((
    a.get('mime_type', 'text')),
                       'default')
#:


#: E122
if True:
    result = some_function_that_takes_arguments(
        'a', 'b', 'c',
        'd', 'e', 'f',
)
#: E122
if some_very_very_very_long_variable_name or var \
or another_very_long_variable_name:
    raise Exception()
#: E122
if some_very_very_very_long_variable_name or var[0] \
or another_very_long_variable_name:
    raise Exception()
#: E122
if True:
    if some_very_very_very_long_variable_name or var \
    or another_very_long_variable_name:
        raise Exception()
#: E122
if True:
    if some_very_very_very_long_variable_name or var[0] \
    or another_very_long_variable_name:
        raise Exception()
#: E122
dictionary = {
    "is": {
    "nested": yes(),
    },
}
#: E122
setup('',
      scripts=[''],
      classifiers=[
      'Development Status :: 4 - Beta',
          'Environment :: Console',
          'Intended Audience :: Developers',
      ])
#:


#: E123 W291
print("E123", (
    "bad", "hanging", "close"
    ))
#
#: E123 E123 E123
result = {
    'foo': [
        'bar', {
            'baz': 'frop',
            }
        ]
    }
#: E123
result = some_function_that_takes_arguments(
    'a', 'b', 'c',
    'd', 'e', 'f',
    )
#: E124
my_list = [1, 2, 3,
           4, 5, 6,
]
#: E124
my_list = [1, 2, 3,
           4, 5, 6,
                   ]
#: E124
result = some_function_that_takes_arguments('a', 'b', 'c',
                                            'd', 'e', 'f',
)
#: E124
fooff(aaaa,
      cca(
          vvv,
          dadd
      ), fff,
)
#: E124
fooff(aaaa,
      ccaaa(
          vvv,
          dadd
      ),
      fff,
)
#: E124
d = dict('foo',
         help="exclude files or directories which match these "
              "comma separated patterns (default: %s)" % DEFAULT_EXCLUDE
              )
#: E124 E128 E128
if line_removed:
    self.event(cr, uid,
        name="Removing the option for contract",
        description="contract line has been removed",
        )
#:


#: E125
if foo is None and bar is "frop" and \
    blah == 'yeah':
    blah = 'yeahnah'
#: E125
# Further indentation required as indentation is not distinguishable


def long_function_name(
    var_one, var_two, var_three,
    var_four):
    print(var_one)
#
#: E125


def qualify_by_address(
    self, cr, uid, ids, context=None,
    params_to_check=frozenset(QUALIF_BY_ADDRESS_PARAM)):
    """ This gets called by the web server """
#: E129 W503
if (a == 2
    or b == "abc def ghi"
    "jkl mno"):
    return True
#:


#: E126
my_list = [
    1, 2, 3,
    4, 5, 6,
        ]
#: E126
abris = 3 + \
        4 + \
        5 + 6
#: E126
fixed = re.sub(r'\t+', ' ', target[c::-1], 1)[::-1] + \
        target[c + 1:]
#: E126 E126
rv.update(dict.fromkeys((
            'qualif_nr', 'reasonComment_en', 'reasonComment_fr',
            'reasonComment_de', 'reasonComment_it'),
        '?'),
    "foo")
#: E126
eat_a_dict_a_day({
        "foo": "bar",
})
#: E126 W503
if (
    x == (
            3
    )
        or y == 4):
    pass
#: E126 W503 W503
if (
    x == (
        3
    )
    or x == (
            3)
        or y == 4):
    pass
#: E131
troublesome_hash = {
    "hash": "value",
    "long": "the quick brown fox jumps over the lazy dog before doing a "
        "somersault",
}
#:


#: E128
# Arguments on first line forbidden when not using vertical alignment
foo = long_function_name(var_one, var_two,
    var_three, var_four)
#
#: E128
print('l.%s\t%s\t%s\t%r' %
    (token[2][0], pos, tokenize.tok_name[token[0]], token[1]))
#: E128


def qualify_by_address(self, cr, uid, ids, context=None,
        params_to_check=frozenset(QUALIF_BY_ADDRESS_PARAM)):
    """ This gets called by the web server """
#:


#: E128
foo(1, 2, 3,
4, 5, 6)
#: E128
foo(1, 2, 3,
 4, 5, 6)
#: E128
foo(1, 2, 3,
  4, 5, 6)
#: E128
foo(1, 2, 3,
   4, 5, 6)
#: E127
foo(1, 2, 3,
     4, 5, 6)
#: E127
foo(1, 2, 3,
      4, 5, 6)
#: E127
foo(1, 2, 3,
       4, 5, 6)
#: E127
foo(1, 2, 3,
        4, 5, 6)
#: E127
foo(1, 2, 3,
         4, 5, 6)
#: E127
foo(1, 2, 3,
          4, 5, 6)
#: E127
foo(1, 2, 3,
           4, 5, 6)
#: E127
foo(1, 2, 3,
            4, 5, 6)
#: E127
foo(1, 2, 3,
             4, 5, 6)
#: E128 E128
if line_removed:
    self.event(cr, uid,
              name="Removing the option for contract",
              description="contract line has been removed",
               )
#: E124 E127 E127
if line_removed:
    self.event(cr, uid,
                name="Removing the option for contract",
                description="contract line has been removed",
                )
#: E127
rv.update(d=('a', 'b', 'c'),
             e=42)
#
#: E127 W503
rv.update(d=('a' + 'b', 'c'),
          e=42, f=42
                 + 42)
#: E127 W503
input1 = {'a': {'calc': 1 + 2}, 'b': 1
                          + 42}
#: E128 W503
rv.update(d=('a' + 'b', 'c'),
          e=42, f=(42
                 + 42))
#: E123
if True:
    def example_issue254():
        return [node.copy(
            (
                replacement
                # First, look at all the node's current children.
                for child in node.children
                # Replace them.
                for replacement in replace(child)
                ),
            dict(name=token.undefined)
        )]
#: E125:2:5 E125:8:5
if ("""
    """):
    pass

for foo in """
    abc
    123
    """.strip().split():
    print(foo)
#: E122:6:5 E122:7:5 E122:8:1
print(dedent(
    '''
        mkdir -p ./{build}/
        mv ./build/ ./{build}/%(revision)s/
    '''.format(
    build='build',
    # more stuff
)
))
#: E701:1:8 E231:1:8 E122:2:1 E203:4:8 E128:5:1
if True:\
print(True)

print(a
, end=' ')
#: E127:4:12
def foo():
    pass
    raise 123 + \
           123
#: E127:4:13
class Eggs:
    pass
    assert 123456 == \
            123456
#: E127:4:11
def f1():
    print('foo')
    with open('/path/to/some/file/you/want/to/read') as file_1, \
          open('/path/to/some/file/being/written', 'w') as file_2:
        file_2.write(file_1.read())
#: E127:5:11
def f1():
    print('foo')
    with open('/path/to/some/file/you/want/to/read') as file_1, \
         open('/path/to/some/file/being/written', 'w') as file_2, \
          open('later-misindent'):
        file_2.write(file_1.read())
