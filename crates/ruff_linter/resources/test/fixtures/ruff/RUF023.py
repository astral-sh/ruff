#########################
# Single-line definitions
#########################

class Klass:
    __slots__ = ["d", "c", "b", "a"]  # a comment that is untouched
    __match_args__ = ("d", "c", "b", "a")

    # Quoting style is retained,
    # but unnecessary parens are not
    __slots__: set = {'b', "c", ((('a')))}
    # Trailing commas are also not retained in the fix
    __match_args__: tuple = ("b", "c", "a",)

class Klass2:
    if bool():
        __slots__ = {"x": "docs for x", "m": "docs for m", "a": "docs for a"}
    else:
        __slots__ = "foo3", "foo2", "foo1"  # NB: an implicit tuple (without parens)

    __match_args__: list[str] = ["the", "three", "little", "pigs"]

    __slots__ = ("parenthesized_item"), "in", ("an_unparenthesized_tuple")

##################################################
# Multiline definitions are flagged, but not fixed
##################################################

class Klass3:
    __slots__ = (
        "d0",
        "c0",  # a comment regarding 'c0'
        "b0",
        # a comment regarding 'a0':
        "a0"
    )

    __match_args__ = [
        "d",
        "c",  # a comment regarding 'c'
        "b",
        # a comment regarding 'a':
        "a"
    ]

class Klass4:
    # we use natural sort,
    # not alphabetical sort.
    __slots__ = {"aadvark237", "aadvark10092", "aadvark174", "aadvark532"}

    __match_args__ = (
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

    __slots__ = ("don't" "care" "about", "__all__" "with", "concatenated" "strings")

###################################
# These should all not get flagged:
###################################

class Klass5:
    __slots__ = ()
    __match_args__ = []
    __slots__ = ("single_item",)
    __match_args__ = (
        "single_item_multiline",
    )
    __slots__ = {"single_item",}
    __slots__ = {"single_item_no_trailing_comma": "docs for that"}
    __match_args__ = [
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

__slots__ = ("b", "a", "e", "d")
__slots__ = ["b", "a", "e", "d"]
__match_args__ = ["foo", "bar", "antipasti"]

class Klass6:
    __slots__ = (9, 8, 7)
    __match_args__ = (  # This is just an empty tuple,
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

    __slots__ =[
        []
    ]
    __slots__ = [
        ()
    ]
    __match_args__ = (
        ()
    )
    __match_args__ = (
        []
    )
    __slots__ = (
        (),
    )
    __slots__ = (
        [],
    )
    __match_args__ = (
        "foo", [], "bar"
    )
    __match_args__ = [
        "foo", (), "bar"
    ]

    __match_args__ = {"a", "set", "for", "__match_args__", "is invalid"}
    __match_args__ = {"this": "is", "also": "invalid"}
