##################################################
# Single-line __all__ definitions (nice 'n' easy!)
##################################################

__all__ = ["d", "c", "b", "a"]  # a comment that is untouched
__all__ += ["foo", "bar", "antipasti"]
__all__ = ("d", "c", "b", "a")

__all__: list = ["b", "c", "a",]  # note the trailing comma, which is retained
__all__: tuple = ("b", "c", "a",)  # note the trailing comma, which is retained

if bool():
    __all__ += ("x", "m", "a", "s")
else:
    __all__ += "foo3", "foo2", "foo1"  # NB: an implicit tuple (without parens)

__all__: list[str] = ["the", "three", "little", "pigs"]

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
    "aadvark174",
    "aadvark532"
)

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

__all__ = {"look", "a", "set"}
__all__ = {"very": "strange", "not": "sorted", "we don't": "care"}
["not", "an", "assignment", "just", "an", "expression"]
__all__ = (9, 8, 7)
__all__ = ("don't" "care" "about", "__all__" "with", "concatenated" "strings")

__all__ = (
    "look",
    (
        "a_veeeeeeeeeeeeeeeeeeery_long_parenthesized_item_we_dont_care_about"
    ),
)

__all__ = (  # This is just an empty tuple,
    # but,
    # it's very well
)  # documented

