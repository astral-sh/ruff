# Tests for functions with parameters.
# The main difference to functions without parameters is that the return type never gets
# parenthesized for values that can't be split (NeedsParentheses::BestFit).


#########################################################################################
# Return types that use NeedsParantheses::BestFit layout with the exception of subscript
#########################################################################################
# String return type that fits on the same line
def parameters_string_return_type(a) -> "ALongIdentifierButDoesntGetParenthesized":
    pass


# String return type that exceeds the line length
def parameters_overlong_string_return_type(
    a,
) -> "ALongIdentifierButDoesntGetParenthesized":
    pass


# Name return type that fits on the same line as the function header
def parameters_name_return_type(a) -> ALongIdentifierButDoesntGetParenthesized:
    pass


# Name return type that exceeds the configured line width
def parameters_overlong_name_return_type(
    a,
) -> ALongIdentifierButDoesntGetParenthesized:
    pass


#########################################################################################
# Unions
#########################################################################################


def test_return_overlong_union(
    a,
) -> A | B | C | DDDDDDDDDDDDDDDDDDDDDDDD | EEEEEEEEEEEEEEEEEEEEEE:
    pass


def test_return_union_with_elements_exceeding_length(
    a,
) -> (
    A
    | B
    | Ccccccccccccccccccccccccccccccccc
    | DDDDDDDDDDDDDDDDDDDDDDDD
    | EEEEEEEEEEEEEEEEEEEEEE
):
    pass


#########################################################################################
# Multiline stirngs (NeedsParentheses::Never)
#########################################################################################


def test_return_multiline_string_type_annotation(a) -> """str
    | list[str]
""":
    pass


def test_return_multiline_string_binary_expression_return_type_annotation(a) -> """str
    | list[str]
""" + "b":
    pass


#########################################################################################
# Implicit concatenated strings (NeedsParentheses::Multiline)
#########################################################################################

def test_implicit_concatenated_string_return_type(a) -> "str" "bbbbbbbbbbbbbbbb":
    pass


def test_overlong_implicit_concatenated_string_return_type(
    a,
) -> "liiiiiiiiiiiisssssst[str]" "bbbbbbbbbbbbbbbb":
    pass


def test_extralong_implicit_concatenated_string_return_type(
    a,
) -> (
    "liiiiiiiiiiiisssssst[str]"
    "bbbbbbbbbbbbbbbbbbbb"
    "cccccccccccccccccccccccccccccccccccccc"
):
    pass


#########################################################################################
# Subscript
#########################################################################################
def parameters_subscript_return_type(a) -> list[str]:
    pass


# Unlike with no-parameters, the return type gets never parenthesized.
def parameters_overlong_subscript_return_type_with_single_element(
    a
) -> list[xxxxxxxxxxxxxxxxxxxxx]:
    pass


def parameters_subscript_return_type_multiple_elements(a) -> list[
    xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx,
    xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
]:
    pass


def parameters_subscript_return_type_multiple_overlong_elements(a) -> list[
    xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx,
    xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
]:
    pass


def parameters_subscriptreturn_type_with_overlong_value_(
    a
) -> liiiiiiiiiiiiiiiiiiiiist[
    xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx,
    xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
]:
    pass


def parameters_overlong_subscript_return_type_with_overlong_single_element(
    a
) -> list[
    xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
]:
    pass


# Not even in this very ridiculous case
def a():
    def b():
        def c():
            def d():
                def e():
                    def f():
                        def g():
                            def h():
                                def i():
                                    def j():
                                        def k():
                                            def l():
                                                def m():
                                                    def n():
                                                        def o():
                                                            def p():
                                                                def q():
                                                                    def r():
                                                                        def s():
                                                                            def t():
                                                                                def u():
                                                                                    def thiiiiiiiiiiiiiiiiiis_iiiiiiiiiiiiiiiiiiiiiiiiiiiiiis_veeeeeeeeeeedooooong(
                                                                                        a,
                                                                                    ) -> list[
                                                                                        int,
                                                                                        float
                                                                                    ]: ...


#########################################################################################
# Magic comma in return type
#########################################################################################

# Black only splits the return type. Ruff also breaks the parameters. This is probably a bug.
def parameters_subscriptreturn_type_with_overlong_value_(a) -> liiiiiiiiiiiiiiiiiiiiist[
    xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx,
    xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx,
]:
    pass


#########################################################################################
# can_omit_optional_parentheses_layout
#########################################################################################

def test_return_multiline_string_binary_expression_return_type_annotation(
    a,
) -> aaaaaaaaaaaaaaaaaaaaaaaaaa > [
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbbbbbbb,
]:
    pass

