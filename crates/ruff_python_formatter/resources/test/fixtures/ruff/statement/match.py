# leading match comment
match foo:  # dangling match comment
    case "bar":
        pass


# leading match comment
match (  # leading expr comment
    # another leading expr comment
    foo  # trailing expr comment
    # another trailing expr comment
):  # dangling match comment
    case "bar":
        pass


# leading match comment
match (  # hello
    foo  # trailing expr comment
    ,  # another
):  # dangling match comment
    case "bar":
        pass


match [  # comment
    first,
    second,
    third
]:  # another comment
    case ["a", "b", "c"]:
        pass

match (  # comment
    "a b c"
).split():  # another comment
    case ["a", "b", "c"]:
        pass


match (  # comment
    # let's go
    yield foo
):  # another comment
    case ["a", "b", "c"]:
        pass


match aaaaaaaaahhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhh:  # comment
    case "sshhhhhhhh":
        pass


def foo():
    match inside_func:  # comment
        case "bar":
            pass


match newlines:

    # case 1 leading comment


    case "top level case comment with newlines":  # case dangling comment
        # pass leading comment
        pass
        # pass trailing comment


    # case 2 leading comment



    case "case comment with newlines" if foo == 2:  # second
        pass

    case "one", "newline" if (foo := 1):  # third
        pass


    case "two newlines":
        pass



    case "three newlines":
        pass
    case _:
        pass


match long_lines:
    case "this is a long line for if condition" if aaaaaaaaahhhhhhhh == 1 and bbbbbbaaaaaaaaaaa == 2:  # comment
        pass

    case "this is a long line for if condition with parentheses" if (aaaaaaaaahhhhhhhh == 1 and bbbbbbaaaaaaaaaaa == 2):  # comment
        pass

    case "named expressions aren't special" if foo := 1:
        pass

    case "named expressions aren't that special" if (foo := 1):
        pass

    case "but with already broken long lines" if (
        aaaaaaahhhhhhhhhhh == 1 and
        bbbbbbbbaaaaaahhhh == 2
    ):  # another comment
        pass


match pattern_comments:
    case (
    only_trailing  # trailing 1
    # trailing 2
# trailing 3
    ):
        pass


match pattern_comments:
    case (  # leading
    only_leading
    ):
        pass


match pattern_comments:
    case (
    # leading
    leading_and_trailing  # trailing 1
    # trailing 2
# trailing 3
    ):
        pass


match pattern_comments:
    case (
    no_comments
    ):
        pass


match pattern_comments:
    case (
        # 1
        pattern  # 2
        # 3
        as  # 4
        # 5
        name  # 6
        # 7
    ):
        pass


match pattern_comments:
    case (
        pattern
        # 1
		as # 2
        # 3
		name #4
        # 5
    ):
        pass


match pattern_comments:
    case (
        pattern
        # 1
		as
        # 2
		name
        # 3
    ):
        pass


match subject:
    case (
        pattern # 1
        as # 2
        name # 3
    ):
        pass


match x:
    case (a as b) as c:
        pass


match pattern_singleton:
    case (
        # leading 1
        # leading 2
        None  # trailing
        # trailing own 1
        # trailing own 2
    ):
        pass
    case (
        True  # trailing
    ):
        ...
    case False:
        ...


match foo:
    case "a", "b":
        pass
    case "a", "b",:
        pass
    case ("a", "b"):
        pass
    case ["a", "b"]:
        pass
    case (["a", "b"]):
        pass


match foo:
    case [  # leading
# leading
    # leading
        # leading
        "a",  # trailing
# trailing
    # trailing
        # trailing
        "b",
    ]:
        pass

match foo:
    case 1:
        y = 0
    case (1):
        y = 1
    case (("a")):
        y = 1
    case (  # comment
        1
    ):
        y = 1
    case (
        # comment
        1
    ):
        y = 1
    case (
        1  # comment
    ):
        y = 1
    case (
        1
        # comment
    ):
        y = 1



match foo:
    case [1, 2, *rest]:
        pass
    case [1, 2, *_]:
        pass
    case [*rest, 1, 2]:
        pass
    case [*_, 1, 2]:
        pass
    case [
        1,
        2,
        *rest,
    ]:
        pass
    case [1, 2, * # comment
        rest]:
        pass
    case [1, 2, * # comment
        _]:
        pass
    case [* # comment
        rest, 1, 2]:
        pass
    case [* # comment
        _, 1, 2]:
        pass
    case [* # end of line
        # own line
        _, 1, 2]:
        pass
    case [* # end of line
        # own line
        _, 1, 2]:
        pass


match foo:
    case (1):
        pass
    case ((1)):
        pass
    case [(1), 2]:
        pass
    case [(  # comment
        1
      ), 2]:
        pass
    case [  # outer
        (  # inner
        1
      ), 2]:
        pass
    case [
		( # outer
			[ # inner
				1,
			]
		)
	]:
        pass
    case [ # outer
		( # inner outer
			[ # inner
				1,
			]
		)
	]:
        pass
    case [ # outer
        # own line
		( # inner outer
			[ # inner
				1,
			]
		)
	]:
        pass
    case [(*rest), (a as b)]:
        pass


match foo:
    case {"a": 1, "b": 2}:
        pass

    case {
        # own line
        "a": 1,  # end-of-line
        # own line
        "b": 2,
    }:
        pass

    case {  # open
        1  # key
        :  # colon
            value  # value
    }:
        pass

    case {**d}:
        pass

    case {
        **  # middle with single item
        b
    }:
        pass

    case {
        # before
        **  # between
        b,
    }:
        pass

    case {
        1: x,
        # foo
        ** # bop
        # before
        b, # boo
        # baz
    }:
        pass

    case {
        1: x
        # foo
        ,
        **
        b,
    }:
        pass


match pattern_match_class:
    case Point2D(
            # own line
            ):
        ...

    case (
        Point2D
        # own line
        ()
    ):
        ...

    case Point2D(  # end of line line
            ):
        ...

    case Point2D(  # end of line
        0, 0
    ):
        ...

    case Point2D(0, 0):
        ...

    case Point2D(
        (  # end of line
        # own line
        0
        ), 0):
        ...

    case Point3D(x=0, y=0, z=000000000000000000000000000000000000000000000000000000000000000000000000000000000):
        ...

    case Bar(0, a=None, b="hello"):
        ...

    case FooBar(# leading
# leading
    # leading
        # leading
            0 # trailing
# trailing
    # trailing
        # trailing
            ):
        ...

    case A(
        b # b
        = # c
        2 # d
        # e
    ):
        pass

    case A(
        # a
        b # b
        = # c
        2 # d
        # e
    ):
        pass


match pattern_match_or:
    case ( # leading 1
          a # trailing 1
          # own line 1
          | # trailing 2
          # own line 2
          b # trailing 3
          # own line 3
          | # trailing 4
          # own line 4
          c # trailing 5
            ):
        ...

    case (
        (a)
        | # trailing
        ( b )
    ):
        ...

    case (a|b|c):
        ...

    case foo | bar | aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaahhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhh:
        ...

    case ( # end of line
          a | b
    # own line
    ):
        ...


# Single-element tuples.
match pattern:
    case (a,):
        pass

    case (a, b):
        pass

    case (a, b,):
        pass

    case a,:
        pass

    case a, b:
        pass

    case a, b,:
        pass

    case (a,  # comment
        ):
        pass

    case (a, b  # comment
            ):
        pass

    case (a, b,  # comment
            ):
        pass

    case (  # comment
        a,
    ):
        pass

    case (  # comment
        a, b
    ):
        pass

    case (  # comment
        a, b,
    ):
        pass

    case (
        # comment
        a,):
        pass

    case (
        # comment
        a, b):
        pass

    case (
        # comment
        a, b,):
        pass
