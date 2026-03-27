###
# Error
###
def foo2(x, y, w, z):
    if x:  # [no-else-return]
        a = 1
        return y
    elif z:
        b = 2
        return w
    else:
        c = 3
        return z


def foo5(x, y, z):
    if x:  # [no-else-return]
        if y:
            a = 4
        else:
            b = 2
        return
    elif z:
        c = 2
    else:
        c = 3
    return


def foo2(x, y, w, z):
    if x:
        a = 1
    elif z:
        b = 2
    else:
        c = 3

    if x:  # [no-else-return]
        a = 1
        return y
    elif z:
        b = 2
        return w
    else:
        c = 3
        return z


def foo1(x, y, z):
    if x:  # [no-else-return]
        a = 1
        return y
    else:
        b = 2
        return z


def foo3(x, y, z):
    if x:
        a = 1
        if y:  # [no-else-return]
            b = 2
            return y
        else:
            c = 3
            return x
    else:
        d = 4
        return z


def foo4(x, y):
    if x:  # [no-else-return]
        if y:
            a = 4
        else:
            b = 2
        return
    else:
        c = 3
    return


def foo6(x, y):
    if x:
        if y:  # [no-else-return]
            a = 4
            return
        else:
            b = 2
    else:
        c = 3
    return


def bar4(x):
    if x:  # [no-else-return]
        return True
    else:
        try:
            return False
        except ValueError:
            return None


def fibo(n):
    if n<2:
        return n;
    else:
        last = 1;
        last2 = 0;


###
# Non-error
###
def bar1(x, y, z):
    if x:
        return y
    return z


def bar2(w, x, y, z):
    if x:
        return y
    if z:
        a = 1
    else:
        return w
    return None


def bar3(x, y, z):
    if x:
        if z:
            return y
    else:
        return z
    return None


def bar4(x):
    if True:
        return
    else:
        # comment
        pass


def bar5():
    if True:
        return
    else:  # comment
        pass


def bar6():
    if True:
        return
    else\
        :\
        # comment
        pass


def bar7():
    if True:
        return
    else\
        :  # comment
        pass


def bar8():
    if True:
        return
    else: pass


def bar9():
    if True:
        return
    else:\
        pass


x = 0

if x == 1:
    y = "a"
elif x == 2:
    y = "b"
else:
    y = "c"


# Regression test for: https://github.com/astral-sh/ruff/issues/9732
def sb(self):
    if self._sb is not None: return self._sb
    else: self._sb = '\033[01;%dm'; self._sa = '\033[0;0m';


def indent(x, y, w, z):
    if x:  # [no-else-return]
        a = 1
        return y
    else:

        c = 3
        return z


def indent(x, y, w, z):
    if x:  # [no-else-return]
        a = 1
        return y
    else:
        # comment
        c = 3
        return z


def indent(x, y, w, z):
    if x:  # [no-else-return]
        a = 1
        return y
    else:
          # comment
        c = 3
        return z


def indent(x, y, w, z):
    if x:  # [no-else-return]
        a = 1
        return y
    else:
  # comment
        c = 3
        return z

def f():
	if True:
	 return True
	else:
	 return False


def has_untracted_files():
    if b'Untracked files' in result.stdout:
        return True
    else:
\
        return False


###
# Errors (try/except/else)
###
def try_except_else_return():
    try:
        foo()
    except Exception:
        return 1
    else:
        return 2


def try_except_else_return_multiple_handlers():
    try:
        foo()
    except ValueError:
        return 1
    except TypeError:
        return 2
    else:
        return 3


# Mixed exit types across handlers - reports based on first handler's type (RET505)
def try_except_else_mixed_exits():
    try:
        foo()
    except ValueError:
        return 1
    except TypeError:
        raise RuntimeError
    else:
        return 3


# Bare except clause
def try_bare_except_else_return():
    try:
        foo()
    except:
        return 1
    else:
        return 2


# Multi-statement else body
def try_except_else_multi_stmt():
    try:
        foo()
    except Exception:
        return 1
    else:
        x = 1
        y = 2
        return x + y


# Comment on the else line
def try_except_else_comment_on_else():
    try:
        foo()
    except Exception:
        return 1
    else:  # some comment
        return 2


# Nested try/except/else inside another try/except/else
def try_except_else_nested():
    try:
        foo()
    except Exception:
        return 1
    else:
        try:
            bar()
        except Exception:
            return 2
        else:
            return 3


# try/except/else nested inside if/else body
def try_except_else_inside_if():
    if cond:
        return 1
    else:
        try:
            foo()
        except Exception:
            return 2
        else:
            return 3


###
# Non-errors (try/except/else)
###

# Has finally block - else is semantically significant
def try_except_else_finally_return():
    try:
        foo()
    except Exception:
        return 1
    else:
        return 2
    finally:
        cleanup()


# Not all handlers return
def try_except_else_partial_return():
    try:
        foo()
    except ValueError:
        return 1
    except TypeError:
        pass
    else:
        return 3


# No else block
def try_except_no_else():
    try:
        foo()
    except Exception:
        return 1


# Return is conditional within the handler - last stmt is not a return
def try_except_else_conditional_return():
    try:
        foo()
    except Exception:
        if cond:
            return 1
    else:
        return 2


# Except handler's last statement is an expression, not an exit
def try_except_else_handler_no_exit():
    try:
        foo()
    except Exception:
        log(error)
    else:
        return 1
