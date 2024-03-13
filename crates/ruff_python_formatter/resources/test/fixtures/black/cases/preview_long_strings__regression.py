class A:
    def foo():
        result = type(message)("")


# Don't merge multiline (e.g. triple-quoted) strings.
def foo():
    query = (
        """SELECT xxxxxxxxxxxxxxxxxxxx(xxx)"""
        """ FROM xxxxxxxxxxxxxxxx WHERE xxxxxxxxxx AND xxx <> xxxxxxxxxxxxxx()""")

# There was a bug where tuples were being identified as long strings.
long_tuple = ('Apple', 'Berry', 'Cherry', 'Dill', 'Evergreen', 'Fig',
           'Grape', 'Harry', 'Iglu', 'Jaguar')

stupid_format_method_bug = "Some really long string that just so happens to be the {} {} to force the 'format' method to hang over the line length boundary. This is pretty annoying.".format("perfect", "length")

class A:
    def foo():
        os.system("This is a regression test. xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxx xxxx.".format("xxxxxxxxxx", "xxxxxx", "xxxxxxxxxx"))


class A:
    def foo():
        XXXXXXXXXXXX.append(
            (
                "xxx_xxxxxxxxxx(xxxxx={}, xxxx={}, xxxxx, xxxx_xxxx_xxxxxxxxxx={})".format(
                    xxxxx, xxxx, xxxx_xxxx_xxxxxxxxxx
                ),
                my_var,
                my_other_var,
            )
        )

class A:
    class B:
        def foo():
            bar(
                (
                    "[{}]: xxx_xxxxxxxxxx(xxxxx={}, xxxx={}, xxxxx={}"
                    " xxxx_xxxx_xxxxxxxxxx={}, xxxx={})"
                    .format(xxxx._xxxxxxxxxxxxxx, xxxxx, xxxx, xxxx_xxxx_xxxxxxxxxx, xxxxxxx)
                ),
                varX,
                varY,
                varZ,
            )

def foo(xxxx):
    for (xxx_xxxx, _xxx_xxx, _xxx_xxxxx, xxx_xxxx) in xxxx:
        for xxx in xxx_xxxx:
            assert ("x" in xxx) or (
                xxx in xxx_xxx_xxxxx
            ), "{0} xxxxxxx xx {1}, xxx {1} xx xxx xx xxxx xx xxx xxxx: xxx xxxx {2}".format(
                xxx_xxxx, xxx, xxxxxx.xxxxxxx(xxx_xxx_xxxxx)
            )

class A:
    def disappearing_comment():
        return (
            (  # xx -x xxxxxxx xx xxx xxxxxxx.
                '{{xxx_xxxxxxxxxx_xxxxxxxx}} xxx xxxx'
                ' {} {{xxxx}} >&2'
                .format(
                    "{xxxx} {xxxxxx}"
                    if xxxxx.xx_xxxxxxxxxx
                    else ( # Disappearing Comment
                        "--xxxxxxx --xxxxxx=x --xxxxxx-xxxxx=xxxxxx"
                        " --xxxxxx-xxxx=xxxxxxxxxxx.xxx"
                    )
                )
            ),
            (x, y, z),
        )

class A:
    class B:
        def foo():
            xxxxx_xxxx(
                xx, "\t"
                "@xxxxxx '{xxxx_xxx}\t' > {xxxxxx_xxxx}.xxxxxxx;"
                "{xxxx_xxx} >> {xxxxxx_xxxx}.xxxxxxx 2>&1; xx=$$?;"
                "xxxx $$xx"
                .format(xxxx_xxx=xxxx_xxxxxxx, xxxxxx_xxxx=xxxxxxx + "/" + xxxx_xxx_xxxx, x=xxx_xxxxx_xxxxx_xxx),
                x,
                y,
                z,
            )

func_call_where_string_arg_has_method_call_and_bad_parens(
    (
        "A long string with {}. This string is so long that it is ridiculous. It can't fit on one line at alllll.".format("formatting")
    ),
)

func_call_where_string_arg_has_old_fmt_and_bad_parens(
    (
        "A long string with {}. This string is so long that it is ridiculous. It can't fit on one line at alllll." % "formatting"
    ),
)

func_call_where_string_arg_has_old_fmt_and_bad_parens(
    (
        "A long string with {}. This {} is so long that it is ridiculous. It can't fit on one line at alllll." % ("formatting", "string")
    ),
)

class A:
    def append(self):
        if True:
            xxxx.xxxxxxx.xxxxx( ('xxxxxxxxxx xxxx xx xxxxxx(%x) xx %x xxxx xx xxx %x.xx'
                                 % (len(self) + 1,
                                    xxxx.xxxxxxxxxx,
                                    xxxx.xxxxxxxxxx))
                                + (' %.3f (%s) to %.3f (%s).\n'
                                   % (xxxx.xxxxxxxxx,
                                      xxxx.xxxxxxxxxxxxxx(xxxx.xxxxxxxxx),
                                      x,
                                      xxxx.xxxxxxxxxxxxxx( xx)
                                      )))

class A:
    def foo():
        some_func_call(
            'xxxxxxxxxx',
            (
                "xx {xxxxxxxxxxx}/xxxxxxxxxxx.xxx xxxx.xxx && xxxxxx -x "
                "\"xxxx xxxxxxx xxxxxx xxxx; xxxx xxxxxx_xxxxx xxxxxx xxxx; "
                "xxxx.xxxx_xxxxxx(['xxxx.xxx'], xxxx.xxxxxxx().xxxxxxxxxx)\" "
            ),
            None,
            ('xxxxxxxxxxx',),
        ),

class A:
    def foo():
        some_func_call(
            (
                "xx {xxxxxxxxxxx}/xxxxxxxxxxx.xxx xxxx.xxx && xxxxxx -x "
                "xxxx, ('xxxxxxx xxxxxx xxxx, xxxx') xxxxxx_xxxxx xxxxxx xxxx; "
                "xxxx.xxxx_xxxxxx(['xxxx.xxx'], xxxx.xxxxxxx().xxxxxxxxxx)\" "
            ),
            None,
            ('xxxxxxxxxxx',),
        ),

xxxxxxx = { 'xx' : 'xxxx xxxxxxx xxxxxxxxx -x xxx -x /xxx/{0} -x xxx,xxx -xx {1} \
-xx {1} -xx xxx=xxx_xxxx,xxx_xx,xxx_xxx,xxx_xxxx,xxx_xx,xxx_xxx |\
 xxxxxx -x xxxxxxxx -x xxxxxxxx -x',
         'xx' : 'xxxx xxxxxxx xxxxxxxxx -x xxx -x /xxx/{0} -x xxx,xxx -xx {1} \
-xx {1} -xx xxx=xxx_xxxx_xxx_xxxx,xxx_xx_xxx_xxxx,xxx_xxxx_xxx_xxxx,\
xxx_xx_xxxx_xxxx,xxx_xxx_xxxx,xxx_xxx_xxxx xxxx=xxx | xxxxxx -x xxxxxxxx -x xxxxxxxx -x'
}

class A:
    def foo(self):
        if True:
            xxxxx_xxxxxxxxxxxx('xxx xxxxxx xxx xxxxxxxxx.xx xx xxxxxxxx.  xxx xxxxxxxxxxxxx.xx xxxxxxx '
                               + 'xx xxxxxx xxxxxx xxxxxx xx xxxxxxx xxx xxx ${0} xx x xxxxxxxx xxxxx'.xxxxxx(xxxxxx_xxxxxx_xxx))

class A:
    class B:
        def foo():
            row = {
                'xxxxxxxxxxxxxxx' : xxxxxx_xxxxx_xxxx,
                # 'xxxxxxxxxxxxxxxxxxxxxxx'
                # 'xxxxxxxxxxxxxxxxxxxxxx'
                # 'xxxxxxxxxxxxxxxxxx'
                # 'xxxxxxxxxxxxxxxxx'
                'xxxxxxxxxx' : xxxxx_xxxxx,
                }

class A:
    def xxxx_xxx_xx_xxxxxxxxxx_xxxx_xxxxxxxxx(xxxx):
        xxxxxxxx = [
            xxxxxxxxxxxxxxxx(
                'xxxx',
                xxxxxxxxxxx={
                    'xxxx' : 1.0,
                },
                xxxxxx={'xxxxxx 1' : xxxxxx(xxxx='xxxxxx 1', xxxxxx=600.0)},
                xxxxxxxx_xxxxxxx=0.0,
            ),
            xxxxxxxxxxxxxxxx(
                'xxxxxxx',
                xxxxxxxxxxx={
                    'xxxx' : 1.0,
                },
                xxxxxx={'xxxxxx 1' : xxxxxx(xxxx='xxxxxx 1', xxxxxx=200.0)},
                xxxxxxxx_xxxxxxx=0.0,
            ),
            xxxxxxxxxxxxxxxx(
                'xxxx',
            ),
        ]

some_dictionary = {
    'xxxxx006': ['xxx-xxx xxxxx3xxxx1xx2xxxxxxxxxxxxxx0xx6xxxxxxxxxx2xxxxxx9xxxxxxxxxx0xxxxx1xxx2x/xx9xx6+x+xxxxxxxxxxxxxx4xxxxxxxxxxxxxxxxxxxxx43xxx2xx2x4x++xxx6xxxxxxxxx+xxxxx/xx9x+xxxxxxxxxxxxxx8x15xxxxxxxxxxxxxxxxx82xx/xxxxxxxxxxxxxx/x5xxxxxxxxxxxxxx6xxxxxx74x4/xxx4x+xxxxxxxxx2xxxxxxxx87xxxxx4xxxxxxxx3xx0xxxxx4xxx1xx9xx5xxxxxxx/xxxxx5xx6xx4xxxx1x/x2xxxxxxxxxxxx64xxxxxxx1x0xx5xxxxxxxxxxxxxx== xxxxx000 xxxxxxxxxx\n',
                 'xxx-xxx xxxxx3xxxx1xx2xxxxxxxxxxxxxx6xxxxxxxxxxxxxx9xxxxxxxxxxxxx3xxx9xxxxxxxxxxxxxxxx0xxxxxxxxxxxxxxxxx2xxxx2xxx6xxxxx/xx54xxxxxxxxx4xxx3xxxxxx9xx3xxxxx39xxxxxxxxx5xx91xxxx7xxxxxx8xxxxxxxxxxxxxxxx9xxx93xxxxxxxxxxxxxxxxx7xxx8xx8xx4/x1xxxxx1x3xxxxxxxxxxxxx3xxxxxx9xx4xx4x7xxxxxxxxxxxxx1xxxxxxxxx7xxxxxxxxxxxxxx4xx6xxxxxxxxx9xxx7xxxx2xxxxxxxxxxxxxxxxxxxxxx8xxxxxxxxxxxxxxxxxxxx6xx== xxxxx010 xxxxxxxxxx\n'],
    'xxxxx016': ['xxx-xxx xxxxx3xxxx1xx2xxxxxxxxxxxxxx0xx6xxxxxxxxxx2xxxxxx9xxxxxxxxxx0xxxxx1xxx2x/xx9xx6+x+xxxxxxxxxxxxxx4xxxxxxxxxxxxxxxxxxxxx43xxx2xx2x4x++xxx6xxxxxxxxx+xxxxx/xx9x+xxxxxxxxxxxxxx8x15xxxxxxxxxxxxxxxxx82xx/xxxxxxxxxxxxxx/x5xxxxxxxxxxxxxx6xxxxxx74x4/xxx4x+xxxxxxxxx2xxxxxxxx87xxxxx4xxxxxxxx3xx0xxxxx4xxx1xx9xx5xxxxxxx/xxxxx5xx6xx4xxxx1x/x2xxxxxxxxxxxx64xxxxxxx1x0xx5xxxxxxxxxxxxxx== xxxxx000 xxxxxxxxxx\n',
                 'xxx-xxx xxxxx3xxxx1xx2xxxxxxxxxxxxxx6xxxxxxxxxxxxxx9xxxxxxxxxxxxx3xxx9xxxxxxxxxxxxxxxx0xxxxxxxxxxxxxxxxx2xxxx2xxx6xxxxx/xx54xxxxxxxxx4xxx3xxxxxx9xx3xxxxx39xxxxxxxxx5xx91xxxx7xxxxxx8xxxxxxxxxxxxxxxx9xxx93xxxxxxxxxxxxxxxxx7xxx8xx8xx4/x1xxxxx1x3xxxxxxxxxxxxx3xxxxxx9xx4xx4x7xxxxxxxxxxxxx1xxxxxxxxx7xxxxxxxxxxxxxx4xx6xxxxxxxxx9xxx7xxxx2xxxxxxxxxxxxxxxxxxxxxx8xxxxxxxxxxxxxxxxxxxx6xx== xxxxx010 xxxxxxxxxx\n']
}

def foo():
    xxx_xxx = (
        'xxxx xxx xxxxxxxx_xxxx xx "xxxxxxxxxx".'
        '\n xxx: xxxxxx xxxxxxxx_xxxx=xxxxxxxxxx'
    ) # xxxx xxxxxxxxxx xxxx xx xxxx xx xxx xxxxxxxx xxxxxx xxxxx.

some_tuple = ("some string", "some string" " which should be joined")

some_commented_string = (  # This comment stays at the top.
    "This string is long but not so long that it needs hahahah toooooo be so greatttt"
    " {} that I just can't think of any more good words to say about it at"
    " allllllllllll".format("ha")  # comments here are fine
)

some_commented_string = (
    "This string is long but not so long that it needs hahahah toooooo be so greatttt"  # But these
    " {} that I just can't think of any more good words to say about it at"  # comments will stay
    " allllllllllll".format("ha")  # comments here are fine
)

lpar_and_rpar_have_comments = func_call(  # LPAR Comment
    "Long really ridiculous type of string that shouldn't really even exist at all. I mean commmme onnn!!!",  # Comma Comment
)  # RPAR Comment

cmd_fstring = (
    f"sudo -E deluge-console info --detailed --sort-reverse=time_added "
    f"{'' if ID is None else ID} | perl -nE 'print if /^{field}:/'"
)

cmd_fstring = f"sudo -E deluge-console info --detailed --sort-reverse=time_added {'' if ID is None else ID} | perl -nE 'print if /^{field}:/'"

cmd_fstring = f"sudo -E deluge-console info --detailed --sort-reverse=time_added {'{{}}' if ID is None else ID} | perl -nE 'print if /^{field}:/'"

cmd_fstring = f"sudo -E deluge-console info --detailed --sort-reverse=time_added {{'' if ID is None else ID}} | perl -nE 'print if /^{field}:/'"

fstring = f"This string really doesn't need to be an {{{{fstring}}}}, but this one most certainly, absolutely {does}."

fstring = (
    f"We have to remember to escape {braces}."
    " Like {these}."
    f" But not {this}."
)

class A:
    class B:
        def foo():
            st_error = STError(
                f"This string ({string_leaf.value}) appears to be pointless (i.e. has"
                " no parent)."
            )

def foo():
    user_regex = _lazy_re_compile(
        r"(^[-!#$%&'*+/=?^_`{}|~0-9A-Z]+(\.[-!#$%&'*+/=?^_`{}|~0-9A-Z]+)*\Z"  # dot-atom
        r'|^"([\001-\010\013\014\016-\037!#-\[\]-\177]|\\[\001-\011\013\014\016-\177])*"\Z)',  # quoted-string
        re.IGNORECASE)

def foo():
    user_regex = _lazy_re_compile(
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"  # dot-atom
        'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx',  # quoted-string
        xyz
    )

def foo():
    user_regex = _lazy_re_compile(
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"  # dot-atom
        'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx',  # quoted-string
        xyz
    )

class A:
    class B:
        def foo():
            if not hasattr(module, name):
                raise ValueError(
                    "Could not find object %s in %s.\n"
                    "Please note that you cannot serialize things like inner "
                    "classes. Please move the object into the main module "
                    "body to use migrations.\n"
                    "For more information, see "
                    "https://docs.djangoproject.com/en/%s/topics/migrations/#serializing-values"
                    % (name, module_name, get_docs_version()))

class A:
    class B:
        def foo():
            if not hasattr(module, name):
                raise ValueError(
                    "Could not find object %s in %s.\nPlease note that you cannot serialize things like inner classes. Please move the object into the main module body to use migrations.\nFor more information, see https://docs.djangoproject.com/en/%s/topics/migrations/#serializing-values"
                    % (name, module_name, get_docs_version()))

x = (
    "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
    "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
    "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
    "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
)

class Step(StepBase):
    def who(self):
        self.cmd = 'SR AAAA-CORRECT NAME IS {last_name} {first_name}{middle_name} {title}/P{passenger_association}'.format(
            last_name=last_name,
            first_name=first_name,
            middle_name=middle_name,
            title=title,
            passenger_association=passenger_association,
        )

xxxxxxx_xxxxxx_xxxxxxx = xxx(
    [
        xxxxxxxxxxxx(
            xxxxxx_xxxxxxx=(
                '((x.aaaaaaaaa = "xxxxxx.xxxxxxxxxxxxxxxxxxxxx") || (x.xxxxxxxxx = "xxxxxxxxxxxx")) && '
                # xxxxx xxxxxxxxxxxx xxxx xxx (xxxxxxxxxxxxxxxx) xx x xxxxxxxxx xx xxxxxx.
                "(x.bbbbbbbbbbbb.xxx != "
                '"xxx:xxx:xxx::cccccccccccc:xxxxxxx-xxxx/xxxxxxxxxxx/xxxxxxxxxxxxxxxxx") && '
            )
        )
    ]
)

if __name__ == "__main__":
    for i in range(4, 8):
        cmd = (
            r"for pid in $(ps aux | grep paster | grep -v grep | grep '\-%d' | awk '{print $2}'); do kill $pid; done"
            % (i)
        )

def A():
    def B():
        def C():
            def D():
                def E():
                    def F():
                        def G():
                            assert (
                                c_float(val[0][0] / val[0][1]).value
                                == c_float(value[0][0] / value[0][1]).value
                            ), "%s didn't roundtrip" % tag

class xxxxxxxxxxxxxxxxxxxxx(xxxx.xxxxxxxxxxxxx):
    def xxxxxxx_xxxxxx(xxxx):
        assert xxxxxxx_xxxx in [
            x.xxxxx.xxxxxx.xxxxx.xxxxxx,
            x.xxxxx.xxxxxx.xxxxx.xxxx,
        ], ("xxxxxxxxxxx xxxxxxx xxxx (xxxxxx xxxx) %x xxx xxxxx" % xxxxxxx_xxxx)

value.__dict__[
    key
] = "test"  # set some Thrift field to non-None in the struct aa bb cc dd ee

RE_ONE_BACKSLASH = {
    "asdf_hjkl_jkl": re.compile(
        r"(?<!([0-9]\ ))(?<=(^|\ ))([A-Z]+(\ )?|[0-9](\ )|[a-z](\ )){4,7}([A-Z]|[0-9]|[a-z])($|\b)(?!(\ ?([0-9]\ )|(\.)))"
    ),
}

RE_TWO_BACKSLASHES = {
    "asdf_hjkl_jkl": re.compile(
        r"(?<!([0-9]\ ))(?<=(^|\ ))([A-Z]+(\ )?|[0-9](\ )|[a-z](\\ )){4,7}([A-Z]|[0-9]|[a-z])($|\b)(?!(\ ?([0-9]\ )|(\.)))"
    ),
}

RE_THREE_BACKSLASHES = {
    "asdf_hjkl_jkl": re.compile(
        r"(?<!([0-9]\ ))(?<=(^|\ ))([A-Z]+(\ )?|[0-9](\ )|[a-z](\\\ )){4,7}([A-Z]|[0-9]|[a-z])($|\b)(?!(\ ?([0-9]\ )|(\.)))"
    ),
}

# We do NOT split on f-string expressions.
print(f"Lorem ipsum dolor sit amet, consetetur sadipscing elitr, sed diam. {[f'{i}' for i in range(10)]}")
x = f"This is a long string which contains an f-expr that should not split {{{[i for i in range(5)]}}}."

# The parens should NOT be removed in this case.
(
    "my very long string that should get formatted if I'm careful to make sure it goes"
    " over 88 characters which it has now"
)

# The parens should NOT be removed in this case.
(
    "my very long string that should get formatted if I'm careful to make sure it goes over 88 characters which"
    " it has now"
)

# The parens should NOT be removed in this case.
(
    "my very long string"
    " that should get formatted"
    " if I'm careful to make sure"
    " it goes over 88 characters which"
    " it has now"
)


def _legacy_listen_examples():
    text += (
        "    \"listen for the '%(event_name)s' event\"\n"
        "\n    # ... (event logic logic logic) ...\n"
        % {
            "since": since,
        }
    )


class X:
    async def foo(self):
        msg = ""
        for candidate in CANDIDATES:
            msg += (
                "**{candidate.object_type} {candidate.rev}**"
                " - {candidate.description}\n"
            )


temp_msg = (
    f"{f'{humanize_number(pos)}.': <{pound_len+2}} "
    f"{balance: <{bal_len + 5}} "
    f"<<{author.display_name}>>\n"
)

assert str(suffix_arr) == (
    "['$', 'angaroo$', 'angrykangaroo$', 'aroo$', 'garoo$', "
    "'grykangaroo$', 'kangaroo$', 'ngaroo$', 'ngrykangaroo$', "
    "'o$', 'oo$', 'roo$', 'rykangaroo$', 'ykangaroo$']"
)
assert str(suffix_arr) != (
    "['$', 'angaroo$', 'angrykangaroo$', 'aroo$', 'garoo$', "
    "'grykangaroo$', 'kangaroo$', 'ngaroo$', 'ngrykangaroo$', "
    "'o$', 'oo$', 'roo$', 'rykangaroo$', 'ykangaroo$']"
)
assert str(suffix_arr) <= (
    "['$', 'angaroo$', 'angrykangaroo$', 'aroo$', 'garoo$', "
    "'grykangaroo$', 'kangaroo$', 'ngaroo$', 'ngrykangaroo$', "
    "'o$', 'oo$', 'roo$', 'rykangaroo$', 'ykangaroo$']"
)
assert str(suffix_arr) >= (
    "['$', 'angaroo$', 'angrykangaroo$', 'aroo$', 'garoo$', "
    "'grykangaroo$', 'kangaroo$', 'ngaroo$', 'ngrykangaroo$', "
    "'o$', 'oo$', 'roo$', 'rykangaroo$', 'ykangaroo$']"
)
assert str(suffix_arr) < (
    "['$', 'angaroo$', 'angrykangaroo$', 'aroo$', 'garoo$', "
    "'grykangaroo$', 'kangaroo$', 'ngaroo$', 'ngrykangaroo$', "
    "'o$', 'oo$', 'roo$', 'rykangaroo$', 'ykangaroo$']"
)
assert str(suffix_arr) > (
    "['$', 'angaroo$', 'angrykangaroo$', 'aroo$', 'garoo$', "
    "'grykangaroo$', 'kangaroo$', 'ngaroo$', 'ngrykangaroo$', "
    "'o$', 'oo$', 'roo$', 'rykangaroo$', 'ykangaroo$']"
)
assert str(suffix_arr) in "['$', 'angaroo$', 'angrykangaroo$', 'aroo$', 'garoo$', 'grykangaroo$', 'kangaroo$', 'ngaroo$', 'ngrykangaroo$', 'o$', 'oo$', 'roo$', 'rykangaroo$', 'ykangaroo$']"
assert str(suffix_arr) not in "['$', 'angaroo$', 'angrykangaroo$', 'aroo$', 'garoo$', 'grykangaroo$', 'kangaroo$', 'ngaroo$', 'ngrykangaroo$', 'o$', 'oo$', 'roo$', 'rykangaroo$', 'ykangaroo$']"
message = (
    f"1. Go to Google Developers Console and log in with your Google account."
    "(https://console.developers.google.com/)"
    "2. You should be prompted to create a new project (name does not matter)."
    "3. Click on Enable APIs and Services at the top."
    "4. In the list of APIs choose or search for YouTube Data API v3 and "
    "click on it. Choose Enable."
    "5. Click on Credentials on the left navigation bar."
    "6. Click on Create Credential at the top."
    '7. At the top click the link for "API key".'
    "8. No application restrictions are needed. Click Create at the bottom."
    "9. You now have a key to add to `{prefix}set api youtube api_key`"
)
message = (
    f"1. Go to Google Developers Console and log in with your Google account."
    "(https://console.developers.google.com/)"
    "2. You should be prompted to create a new project (name does not matter)."
    f"3. Click on Enable APIs and Services at the top."
    "4. In the list of APIs choose or search for YouTube Data API v3 and "
    "click on it. Choose Enable."
    f"5. Click on Credentials on the left navigation bar."
    "6. Click on Create Credential at the top."
    '7. At the top click the link for "API key".'
    "8. No application restrictions are needed. Click Create at the bottom."
    "9. You now have a key to add to `{prefix}set api youtube api_key`"
)
message = (
    f"1. Go to Google Developers Console and log in with your Google account."
    "(https://console.developers.google.com/)"
    "2. You should be prompted to create a new project (name does not matter)."
    f"3. Click on Enable APIs and Services at the top."
    "4. In the list of APIs choose or search for YouTube Data API v3 and "
    "click on it. Choose Enable."
    f"5. Click on Credentials on the left navigation bar."
    "6. Click on Create Credential at the top."
    '7. At the top click the link for "API key".'
    "8. No application restrictions are needed. Click Create at the bottom."
    f"9. You now have a key to add to `{prefix}set api youtube api_key`"
)

# It shouldn't matter if the string prefixes are capitalized.
temp_msg = (
    F"{F'{humanize_number(pos)}.': <{pound_len+2}} "
    F"{balance: <{bal_len + 5}} "
    F"<<{author.display_name}>>\n"
)

fstring = (
    F"We have to remember to escape {braces}."
    " Like {these}."
    F" But not {this}."
)

welcome_to_programming = R"hello," R" world!"

fstring = F"f-strings definitely make things more {difficult} than they need to be for {{black}}. But boy they sure are handy. The problem is that some lines will need to have the 'f' whereas others do not. This {line}, for example, needs one."

x = F"This is a long string which contains an f-expr that should not split {{{[i for i in range(5)]}}}."

x = (
    "\N{BLACK RIGHT-POINTING TRIANGLE WITH DOUBLE VERTICAL BAR}\N{VARIATION SELECTOR-16}"
)

xxxxxx_xxx_xxxx_xx_xxxxx_xxxxxxxx_xxxxxxxx_xxxxxxxxxx_xxxx_xxxx_xxxxx = xxxx.xxxxxx.xxxxxxxxx.xxxxxxxxxxxxxxxxxxxx(
    xx_xxxxxx={
        "x3_xxxxxxxx": "xxx3_xxxxx_xxxxxxxx_xxxxxxxx_xxxxxxxxxx_xxxxxxxx_xxxxxx_xxxxxxx",
    },
)

# Regression test for https://github.com/psf/black/issues/3117.
some_dict = {
    "something_something":
        r"Lorem ipsum dolor sit amet, an sed convenire eloquentiam \t"
        r"signiferumque, duo ea vocibus consetetur scriptorem. Facer \t",
}

# Regression test for https://github.com/psf/black/issues/3459.
xxxx(
    empty_str_as_first_split=''
    f'xxxxxxx {xxxxxxxxxx} xxx xxxxxxxxxx xxxxx xxx xxx xx '
    'xxxxx xxxxxxxxx xxxxxxx, xxx xxxxxxxxxxx xxx xxxxx. '
    f'xxxxxxxxxxxxx xxxx xx xxxxxxxxxx. xxxxx: {x.xxx}',
    empty_u_str_as_first_split=u''
    f'xxxxxxx {xxxxxxxxxx} xxx xxxxxxxxxx xxxxx xxx xxx xx '
    'xxxxx xxxxxxxxx xxxxxxx, xxx xxxxxxxxxxx xxx xxxxx. '
    f'xxxxxxxxxxxxx xxxx xx xxxxxxxxxx. xxxxx: {x.xxx}',
)

# Regression test for https://github.com/psf/black/issues/3455.
a_dict = {
    "/this/is/a/very/very/very/very/very/very/very/very/very/very/long/key/without/spaces":
        # And there is a comment before the value
        ("item1", "item2", "item3"),
}

# Regression test for https://github.com/psf/black/issues/3506.
s = (
    "With single quote: ' "
    f" {my_dict['foo']}"
    ' With double quote: " '
    f' {my_dict["bar"]}'
)

s = f'Lorem Ipsum is simply dummy text of the printing and typesetting industry:\'{my_dict["foo"]}\''
