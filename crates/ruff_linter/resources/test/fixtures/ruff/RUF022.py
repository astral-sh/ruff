##################################################
# Single-line __all__ definitions (nice 'n' easy!)
##################################################

__all__ = ["d", "c", "b", "a"]  # a comment that is untouched
__all__ += ["foo", "bar", "antipasti"]
__all__ = ("d", "c", "b", "a")

# Quoting style is retained,
# but unnecessary parens are not
__all__: list = ['b', "c", ((('a')))]
# Trailing commas are also not retained in single-line `__all__` definitions
# (but they are in multiline `__all__` definitions)
__all__: tuple = ("b", "c", "a",)

if bool():
    __all__ += ("x", "m", "a", "s")
else:
    __all__ += "foo3", "foo2", "foo1"  # NB: an implicit tuple (without parens)

__all__: list[str] = ["the", "three", "little", "pigs"]

__all__ = ("parenthesized_item"), "in", ("an_unparenthesized_tuple")
__all__.extend(["foo", "bar"])
__all__.extend(("foo", "bar"))
__all__.extend((((["foo", "bar"]))))

####################################
# Neat multiline __all__ definitions
####################################

__all__ = (
    "d0",
    "c0",  # a comment regarding 'c0'
    "b0",
    # a comment regarding 'a0':
    "a0"
)

__all__ = [
    "d",
    "c",  # a comment regarding 'c'
    "b",
    # a comment regarding 'a':
    "a"
]

# we implement an "isort-style sort":
# SCEAMING_CASE constants first,
# then CamelCase classes,
# then anything that's lowercase_snake_case.
# This (which is currently alphabetically sorted)
# should get reordered accordingly:
__all__ = [
    "APRIL",
    "AUGUST",
    "Calendar",
    "DECEMBER",
    "Day",
    "FEBRUARY",
    "FRIDAY",
    "HTMLCalendar",
    "IllegalMonthError",
    "JANUARY",
    "JULY",
    "JUNE",
    "LocaleHTMLCalendar",
    "MARCH",
    "MAY",
    "MONDAY",
    "Month",
    "NOVEMBER",
    "OCTOBER",
    "SATURDAY",
    "SEPTEMBER",
    "SUNDAY",
    "THURSDAY",
    "TUESDAY",
    "TextCalendar",
    "WEDNESDAY",
    "calendar",
    "timegm",
    "weekday",
    "weekheader"]

##########################################
# Messier multiline __all__ definitions...
##########################################

# comment0
__all__ = ("d", "a",  # comment1
           # comment2
           "f", "b",
                                        "strangely",  # comment3
            # comment4
    "formatted",
    # comment5
)  # comment6
# comment7

__all__ = [  # comment0
    # comment1
    # comment2
    "dx", "cx", "bx", "ax"  # comment3
    # comment4
    # comment5
    # comment6
]  # comment7

__all__ = ["register", "lookup", "open", "EncodedFile", "BOM", "BOM_BE",
           "BOM_LE", "BOM32_BE", "BOM32_LE", "BOM64_BE", "BOM64_LE",
           "BOM_UTF8", "BOM_UTF16", "BOM_UTF16_LE", "BOM_UTF16_BE",
           "BOM_UTF32", "BOM_UTF32_LE", "BOM_UTF32_BE",
           "CodecInfo", "Codec", "IncrementalEncoder", "IncrementalDecoder",
           "StreamReader", "StreamWriter",
           "StreamReaderWriter", "StreamRecoder",
           "getencoder", "getdecoder", "getincrementalencoder",
           "getincrementaldecoder", "getreader", "getwriter",
           "encode", "decode", "iterencode", "iterdecode",
           "strict_errors", "ignore_errors", "replace_errors",
           "xmlcharrefreplace_errors",
           "backslashreplace_errors", "namereplace_errors",
           "register_error", "lookup_error"]

__all__: tuple[str, ...] = (  # a comment about the opening paren
    # multiline comment about "bbb" part 1
    # multiline comment about "bbb" part 2
    "bbb",
    # multiline comment about "aaa" part 1
    # multiline comment about "aaa" part 2
    "aaa",
)

# we use natural sort for `__all__`,
# not alphabetical sort.
# Also, this doesn't end with a trailing comma,
# so the autofix shouldn't introduce one:
__all__ = (
    "aadvark237",
    "aadvark10092",
    "aadvark174",         # the very long whitespace span before this comment is retained
    "aadvark532"                       # the even longer whitespace span before this comment is retained
)

__all__.extend((  # comment0
    # comment about foo
    "foo",  # comment about foo
    # comment about bar
    "bar"  # comment about bar
    # comment1
))  # comment2

__all__.extend(  # comment0
    # comment1
    (  # comment2
        # comment about foo
        "foo",  # comment about foo
        # comment about bar
        "bar"  # comment about bar
        # comment3
    )  # comment4
)  # comment2

__all__.extend([  # comment0
    # comment about foo
    "foo",  # comment about foo
    # comment about bar
    "bar"  # comment about bar
    # comment1
])  # comment2

__all__.extend(  # comment0
    # comment1
    [  # comment2
        # comment about foo
        "foo",  # comment about foo
        # comment about bar
        "bar"  # comment about bar
        # comment3
    ]  # comment4
)  # comment2

__all__ = ["Style", "Treeview",
           # Extensions
           "LabeledScale", "OptionMenu",
]

__all__ = ["Awaitable", "Coroutine",
           "AsyncIterable", "AsyncIterator", "AsyncGenerator",
           ]

__all__ = [
    "foo",
    "bar",
    "baz",
    ]

#########################################################################
# These should be flagged, but not fixed:
# - Parenthesized items in multiline definitions are out of scope
# - The same goes for any `__all__` definitions with concatenated strings
#########################################################################

__all__ = (
    "look",
    (
        "a_veeeeeeeeeeeeeeeeeeery_long_parenthesized_item"
    ),
)

__all__ = (
    "b",
    ((
        "c"
    )),
    "a"
)

__all__ = ("don't" "care" "about", "__all__" "with", "concatenated" "strings")

############################################################
# Trailing-comma edge cases that should be flagged and fixed
############################################################

__all__ = (
    "loads",
    "dumps",)

__all__ = [
    "loads",
    "dumps"       ,     ]

__all__ = ['xp', 'yp',
                'canvas'

                # very strangely placed comment

                ,

                # another strangely placed comment
                ]

__all__ = (
    "foo"
    # strange comment 1
    ,
    # comment about bar
    "bar"
    # strange comment 2
    ,
)

__all__ = (  # comment about the opening paren
    # multiline strange comment 0a
    # multiline strange comment 0b
    "foo"  # inline comment about foo
    # multiline strange comment 1a
    # multiline strange comment 1b
    ,  # comment about the comma??
    # comment about bar part a
    # comment about bar part b
    "bar"  # inline comment about bar
    # strange multiline comment comment 2a
    # strange multiline comment 2b
    ,
    # strange multiline comment 3a
    # strange multiline comment 3b
)  # comment about the closing paren

###################################
# These should all not get flagged:
###################################

__all__ = ()
__all__ = []
__all__ = ("single_item",)
__all__ = (
    "single_item_multiline",
)
__all__ = ["single_item",]
__all__ = ["single_item_no_trailing_comma"]
__all__ = [
    "single_item_multiline_no_trailing_comma"
]
__all__ = ("not_a_tuple_just_a_string")
__all__ = ["a", "b", "c", "d"]
__all__ += ["e", "f", "g"]
__all__ = ("a", "b", "c", "d")

if bool():
    __all__ += ("e", "f", "g")
else:
    __all__ += ["alpha", "omega"]

class IntroducesNonModuleScope:
    __all__ = ("b", "a", "e", "d")
    __all__ = ["b", "a", "e", "d"]
    __all__ += ["foo", "bar", "antipasti"]
    __all__.extend(["zebra", "giraffe", "antelope"])

__all__ = {"look", "a", "set"}
__all__ = {"very": "strange", "not": "sorted", "we don't": "care"}
["not", "an", "assignment", "just", "an", "expression"]
__all__ = (9, 8, 7)
__all__ = (  # This is just an empty tuple,
    # but,
    # it's very well
)  # documented

__all__.append("foo")
__all__.extend(["bar", "foo"])
__all__.extend([
    "bar",  # comment0
    "foo"  # comment1
])
__all__.extend(("bar", "foo"))
__all__.extend(
    (
        "bar",
        "foo"
    )
)

# We don't deduplicate elements (yet);
# this just ensures that duplicate elements aren't unnecessarily
# reordered by an autofix:
__all__ = (
    "duplicate_element",  # comment1
    "duplicate_element",  # comment3
    "duplicate_element",  # comment2
    "duplicate_element",  # comment0
)

__all__ =[
    []
]
__all__ [
    ()
]
__all__ = (
    ()
)
__all__ = (
    []
)
__all__ = (
    (),
)
__all__ = (
    [],
)
__all__ = (
    "foo", [], "bar"
)
__all__ = [
    "foo", (), "bar"
]

__all__ = "foo", "an" "implicitly_concatenated_second_item", not_a_string_literal
