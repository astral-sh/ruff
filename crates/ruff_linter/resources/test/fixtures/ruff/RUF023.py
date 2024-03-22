#########################
# Single-line definitions
#########################

class Klass:
    __slots__ = ["d", "c", "b", "a"]  # a comment that is untouched
    __slots__ = ("d", "c", "b", "a")

    # Quoting style is retained,
    # but unnecessary parens are not
    __slots__: set = {'b', "c", ((('a')))}
    # Trailing commas are also not retained for single-line definitions
    # (but they are in multiline definitions)
    __slots__: tuple = ("b", "c", "a",)

class Klass2:
    if bool():
        __slots__ = {"x": "docs for x", "m": "docs for m", "a": "docs for a"}
    else:
        __slots__ = "foo3", "foo2", "foo1"  # NB: an implicit tuple (without parens)

    __slots__: list[str] = ["the", "three", "little", "pigs"]
    __slots__ = ("parenthesized_item"), "in", ("an_unparenthesized_tuple")
    # we use natural sort,
    # not alphabetical sort or "isort-style" sort
    __slots__ = {"aadvark237", "aadvark10092", "aadvark174", "aadvark532"}

############################
# Neat multiline definitions
############################

class Klass3:
    __slots__ = (
        "d0",
        "c0",  # a comment regarding 'c0'
        "b0",
        # a comment regarding 'a0':
        "a0"
    )
    __slots__ = [
        "d",
        "c",  # a comment regarding 'c'
        "b",
        # a comment regarding 'a':
        "a"
    ]

##################################
# Messier multiline definitions...
##################################

class Klass4:
    # comment0
    __slots__ = ("d", "a",  # comment1
            # comment2
            "f", "b",
                                            "strangely",  # comment3
                # comment4
        "formatted",
        # comment5
    )  # comment6
    # comment7

    __slots__ = [  # comment0
        # comment1
        # comment2
        "dx", "cx", "bx", "ax"  # comment3
        # comment4
        # comment5
        # comment6
    ]  # comment7

# from cpython/Lib/pathlib/__init__.py
class PurePath:
    __slots__ = (
        # The `_raw_paths` slot stores unnormalized string paths. This is set
        # in the `__init__()` method.
        '_raw_paths',

        # The `_drv`, `_root` and `_tail_cached` slots store parsed and
        # normalized parts of the path. They are set when any of the `drive`,
        # `root` or `_tail` properties are accessed for the first time. The
        # three-part division corresponds to the result of
        # `os.path.splitroot()`, except that the tail is further split on path
        # separators (i.e. it is a list of strings), and that the root and
        # tail are normalized.
        '_drv', '_root', '_tail_cached',

        # The `_str` slot stores the string representation of the path,
        # computed from the drive, root and tail when `__str__()` is called
        # for the first time. It's used to implement `_str_normcase`
        '_str',

        # The `_str_normcase_cached` slot stores the string path with
        # normalized case. It is set when the `_str_normcase` property is
        # accessed for the first time. It's used to implement `__eq__()`
        # `__hash__()`, and `_parts_normcase`
        '_str_normcase_cached',

        # The `_parts_normcase_cached` slot stores the case-normalized
        # string path after splitting on path separators. It's set when the
        # `_parts_normcase` property is accessed for the first time. It's used
        # to implement comparison methods like `__lt__()`.
        '_parts_normcase_cached',

        # The `_hash` slot stores the hash of the case-normalized string
        # path. It's set when `__hash__()` is called for the first time.
        '_hash',
    )

# From cpython/Lib/pickletools.py
class ArgumentDescriptor(object):
    __slots__ = (
        # name of descriptor record, also a module global name; a string
        'name',

        # length of argument, in bytes; an int; UP_TO_NEWLINE and
        # TAKEN_FROM_ARGUMENT{1,4,8} are negative values for variable-length
        # cases
        'n',

        # a function taking a file-like object, reading this kind of argument
        # from the object at the current position, advancing the current
        # position by n bytes, and returning the value of the argument
        'reader',

        # human-readable docs for this arg descriptor; a string
        'doc',
    )

####################################
# Should be flagged, but not fixed
####################################

# from cpython/Lib/test/test_inspect.py.
# Multiline dicts are out of scope.
class SlotUser:
    __slots__ = {'power': 'measured in kilowatts',
                 'distance': 'measured in kilometers'}

class Klass5:
    __slots__ = (
        "look",
        (
            "a_veeeeeeeeeeeeeeeeeeery_long_parenthesized_item"
        ),
    )
    __slots__ = (
        "b",
        ((
            "c"
        )),
        "a"
    )
    __slots__ = ("don't" "care" "about", "__slots__" "with", "concatenated" "strings")

############################################################
# Trailing-comma edge cases that should be flagged and fixed
############################################################

class BezierBuilder:
    __slots__ = ('xp', 'yp',
                 'canvas',)

class BezierBuilder2:
    __slots__ = {'xp', 'yp',
                 'canvas'      ,          }

class BezierBuilder3:
    __slots__ = ['xp', 'yp',
                 'canvas'

                 # very strangely placed comment

                 ,

                 # another strangely placed comment
                 ]

class BezierBuilder4:
    __slots__ = (
        "foo"
        # strange comment 1
        ,
        # comment about bar
        "bar"
        # strange comment 2
        ,
    )

    __slots__ = {"foo", "bar",
                 "baz", "bingo"
                 }

###################################
# These should all not get flagged:
###################################

class Klass6:
    __slots__ = ()
    __slots__ = []
    __slots__ = ("single_item",)
    __slots__ = (
        "single_item_multiline",
    )
    __slots__ = {"single_item",}
    __slots__ = {"single_item_no_trailing_comma": "docs for that"}
    __slots__ = [
        "single_item_multiline_no_trailing_comma"
    ]
    __slots__ = ("not_a_tuple_just_a_string")
    __slots__ = ["a", "b", "c", "d"]
    __slots__ += ["e", "f", "g"]
    __slots__ = ("a", "b", "c", "d")

    if bool():
        __slots__ += ("e", "f", "g")
    else:
        __slots__ += ["alpha", "omega"]

    __slots__ = {"not": "sorted", "but": "includes", **a_kwarg_splat}

__slots__ = ("b", "a", "e", "d")
__slots__ = ["b", "a", "e", "d"]
__slots__ = ["foo", "bar", "antipasti"]

class Klass6:
    __slots__ = (9, 8, 7)
    __slots__ = (  # This is just an empty tuple,
        # but,
        # it's very well
    )  # documented

    # We don't deduplicate elements;
    # this just ensures that duplicate elements aren't unnecessarily
    # reordered by an autofix:
    __slots__ = (
        "duplicate_element",  # comment1
        "duplicate_element",  # comment3
        "duplicate_element",  # comment2
        "duplicate_element",  # comment0
    )

    __slots__ = "foo", "an" "implicitly_concatenated_second_item", not_a_string_literal

    __slots__ =[
        []
    ]
    __slots__ = [
        ()
    ]
    __slots__ = (
        ()
    )
    __slots__ = (
        []
    )
    __slots__ = (
        (),
    )
    __slots__ = (
        [],
    )
    __slots__ = (
        "foo", [], "bar"
    )
    __slots__ = [
        "foo", (), "bar"
    ]
