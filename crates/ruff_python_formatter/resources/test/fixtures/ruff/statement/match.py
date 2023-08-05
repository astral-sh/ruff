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
