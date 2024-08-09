def arbitrary_fun(*args, **kwargs): ...


def broken():
    try:
        ...
    except Exception as e:
        e.add_note("...")  # error

def not_broken():
    try:
        ...
    except Exception as e:  # safe (other linters will catch this)
        pass

def not_broken2():
    try:
        ...
    except Exception as e:
        e.add_note("...")
        raise  # safe

def broken2():
    # other exception raised
    try:
        ...
    except Exception as e:
        f = ValueError()
        e.add_note("...")  # error
        raise f

def not_broken3():
    # raised as cause
    try:
        ...
    except Exception as e:
        f = ValueError()
        e.add_note("...")  # safe
        raise f from e

def not_broken4():
    # assigned to other variable
    try:
        ...
    except Exception as e:
        e.add_note("...")  # safe
        foo = e

def not_broken5():
    # "used" in function call
    try:
        ...
    except Exception as e:
        e.add_note("...")  # safe
        # not that printing the exception is actually using it, but we treat
        # it being used as a parameter to any function as "using" it
        print(e)

def not_broken6():
    try:
        ...
    except Exception as e:
        e.add_note("...")  # safe
        list(e)

def not_broken7():
    # kwarg
    try:
        ...
    except Exception as e:
        e.add_note("...")  # safe
        arbitrary_fun(kwarg=e)

def not_broken8():
    # stararg
    try:
        ...
    except Exception as e:
        e.add_note("...")  # safe
        arbitrary_fun(*(e,))

def not_broken9():
    try:
        ...
    except Exception as e:
        e.add_note("...")  # safe
        arbitrary_fun(**{"e": e})


def broken3():
    # multiple ExceptHandlers
    try:
        ...
    except ValueError as e:
        e.add_note("")  # error
    except TypeError as e:
        raise e

def not_broken10():
    # exception variable used before `add_note`
    mylist = []
    try:
        ...
    except Exception as e: # safe
        mylist.append(e)
        e.add_note("")

def not_broken11():
    # AnnAssign
    try:
        ...
    except Exception as e:  # safe
        e.add_note("")
        ann_assign_target: Exception = e

def broken4():
    # special case: e is only used in the `add_note` call itself
    try:
        ...
    except Exception as e:  # error
        e.add_note(str(e))
        e.add_note(str(e))

def broken5():
    # check nesting
    try:
        ...
    except Exception as e:  # error
        e.add_note("")
        try:
            ...
        except ValueError as e:
            raise

def broken6():
    # questionable if this should error
    try:
        ...
    except Exception as e:
        e.add_note("")
        e = ValueError()



def broken7():
    exc = ValueError()
    exc.add_note("")


def not_broken12(exc: ValueError):
    exc.add_note("")



def not_broken13():
    e2 = ValueError()
    try:
        ...
    except Exception as e:
        e2.add_note(str(e))  # safe
    raise e2


def broken8():
    try:
        ...
    except Exception as e:  # should error
        e.add_note("")
        f = lambda e: e

def broken9():
    try:
        ...
    except Exception as e:  # should error
        e.add_note("")

        def habla(e):
            raise e


# *** unhandled cases ***
# We also don't track other variables
try:
    ...
except Exception as e:
    e3 = e
    e3.add_note("")  # should error

# we currently only check the target of the except handler, even though this clearly
# is hitting the same general pattern. But handling this case without a lot of false
# alarms is very tricky without more infrastructure.
try:
    ...
except Exception as e:
    e2 = ValueError()  # should error
    e2.add_note(str(e))
