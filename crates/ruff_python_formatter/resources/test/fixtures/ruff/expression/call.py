from unittest.mock import MagicMock


def f(*args, **kwargs):
    pass

this_is_a_very_long_argument_asökdhflakjslaslhfdlaffahflahsöfdhasägporejfäalkdsjäfalisjäfdlkasjd = 1
session = MagicMock()
models = MagicMock()

f()

f(1)

f(x=2)

f(1, x=2)

f(
    this_is_a_very_long_argument_asökdhflakjslaslhfdlaffahflahsöfdhasägporejfäalkdsjäfalisjäfdlkasjd
)
f(
    this_is_a_very_long_keyword_argument_asökdhflakjslaslhfdlaffahflahsöfdhasägporejfäalkdsjäfalisjäfdlkasjd=1
)

f(
    1,
    mixed_very_long_arguments=1,
)

f(
    this_is_a_very_long_argument_asökdhflakjslaslhfdlaffahflahsöfdhasägporejfäalkdsjäfalisjäfdlkasjd,
    these_arguments_have_values_that_need_to_break_because_they_are_too_long1=(100000 - 100000000000),
    these_arguments_have_values_that_need_to_break_because_they_are_too_long2="akshfdlakjsdfad" + "asdfasdfa",
    these_arguments_have_values_that_need_to_break_because_they_are_too_long3=session,
)

f(
    # dangling comment
)


f(
    only=1, short=1, arguments=1
)

f(
    hey_this_is_a_long_call, it_has_funny_attributes_that_breaks_into_three_lines=1
)

f(
    hey_this_is_a_very_long_call=1, it_has_funny_attributes_asdf_asdf=1, too_long_for_the_line=1, really=True
)

# Call chains/fluent interface (https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#call-chains)
result = (
    session.query(models.Customer.id)
    .filter(
        models.Customer.account_id == 10000,
        models.Customer.email == "user@example.org",
    )
    .order_by(models.Customer.id.asc())
    .all()
)
# TODO(konstin): Black has this special case for comment placement where everything stays in one line
f(
    "aaaaaaaa", "aaaaaaaa", "aaaaaaaa", "aaaaaaaa", "aaaaaaaa", "aaaaaaaa", "aaaaaaaa"
)

f(
    session,
    b=1,
    ** # oddly placed end-of-line comment
    dict()
)
f(
    session,
    b=1,
    **
    # oddly placed own line comment
    dict()
)
f(
    session,
    b=1,
    **(# oddly placed own line comment
    dict()
    )
)
f(
    session,
    b=1,
    **(
        # oddly placed own line comment
    dict()
    )
)

# Don't add a magic trailing comma when there is only one entry
# Minimized from https://github.com/django/django/blob/7eeadc82c2f7d7a778e3bb43c34d642e6275dacf/django/contrib/admin/checks.py#L674-L681
f(
    a.very_long_function_function_that_is_so_long_that_it_expands_the_parent_but_its_only_a_single_argument()
)

f( # abc
)

f( # abc
    # abc
)

f(
    # abc
)

f ( # abc
    1
)

f (
    # abc
    1
)

f (
    1
    # abc
)

threshold_date = datetime.datetime.now() - datetime.timedelta(  # comment
    days=threshold_days_threshold_days
)

# Parenthesized and opening-parenthesis comments
func(
    (x for x in y)
)

func(  # outer comment
    (x for x in y)
)

func(
    ( # inner comment
        x for x in y
    )
)

func(
    (
        # inner comment
        x for x in y
    )
)

func(  # outer comment
    (  # inner comment
        1
    )
)

func(
    # outer comment
    ( # inner comment
        x for x in y
    )
)


func(
    ( # inner comment
        []
    )
)

func(
    # outer comment
    ( # inner comment
        []
    )
)

# Comments between the function and its arguments
aaa = (
    bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
    # awkward comment
    ()
    .bbbbbbbbbbbbbbbb
)

aaa = (
    # bar
    bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
    # awkward comment
    ()
    .bbbbbbbbbbbbbbbb
)


aaa = (
    # bar
    bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb # baz
    # awkward comment
    ()
    .bbbbbbbbbbbbbbbb
)

aaa = (
    (foo # awkward comment
        )
    ()
    .bbbbbbbbbbbbbbbb
)

aaa = (
    (
        bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
        # awkward comment
    )
    ()
    .bbbbbbbbbbbbbbbb
)

# Comments around keywords
f(x= # comment
  1)

f(x # comment
 =
  1)

f(x=
  # comment
  1)

f(x=(# comment
  1
))


f(x=(
  # comment
  1
))

args = [2]
args2 = [3]
kwargs = {"4": 5}

# https://github.com/astral-sh/ruff/issues/6498
f(a=1, *args, **kwargs)
f(*args, a=1, **kwargs)
f(*args, a=1, *args2, **kwargs)
f(  # a
    *  # b
    args
    # c
    ,  # d
    a=1,
    # e
    *  # f
    args2
    # g
    **  # h
    kwargs,
)

# Regression test for: https://github.com/astral-sh/ruff/issues/7370
result = (
    f(111111111111111111111111111111111111111111111111111111111111111111111111111111111)
    + 1
)()


result = (object[complicate_caller])("argument").a["b"].test(argument)
