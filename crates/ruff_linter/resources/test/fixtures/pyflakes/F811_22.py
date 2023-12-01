def redef(value):
    match value:
        case True:

            def fun(x, y):
                return x

        case False:

            def fun(x, y):
                return y

    return fun