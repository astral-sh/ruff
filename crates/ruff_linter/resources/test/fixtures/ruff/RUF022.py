##################################################
# Single-line __all__ definitions (nice 'n' easy!)
##################################################

__all__ = ["d", "c", "b", "a"]  # a comment that is untouched
__all__ += ["foo", "bar", "antipasti"]
__all__ = ("d", "c", "b", "a")

if bool():
    __all__ += ("x", "m", "a", "s")
else:
    __all__ += "foo3", "foo2", "foo1"  # NB: an implicit tuple (without parens)

####################################
# Neat multiline __all__ definitions
####################################

__all__ = (
    "d",
    "c",  # a comment regarding 'c'
    "b",
    # a comment regarding 'a':
    "a"
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

###################################
# These should all not get flagged:
###################################

__all__ = ()
__all__ = []
__all__ = ("single_item",)
__all__ = ["single_item",]
__all__ = ("not_a_tuple_just_a_string")
__all__ = ["a", "b", "c", "d"]
__all__ += ["e", "f", "g"]
__all__ = ("a", "b", "c", "d")

if bool():
    __all__ += ("e", "f", "g")
else:
    __all__ += ["omega", "alpha"]

class IntroducesNonModuleScope:
    __all__ = ("b", "a", "e", "d")
    __all__ = ["b", "a", "e", "d"]
    __all__ += ["foo", "bar", "antipasti"]

__all__ = {"look", "a", "set"}
__all__ = {"very": "strange", "not": "sorted", "we don't": "care"}
["not", "an", "assignment", "just", "an", "expression"]
__all__ = (9, 8, 7)
__all__ = ("don't" "care" "about", "__all__" "with", "concatenated" "strings")
