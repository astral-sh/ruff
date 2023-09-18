try:
    ...
except:
    ...

try:
    ...
except (KeyError):  # should remove brackets and be a single line
    ...


try: # try
    ...
    # end of body
# before except
except (Exception, ValueError) as exc:  # except line
    ...
# before except 2
except KeyError as key:  # except line 2
    ...
    # in body 2
# before else
else:
    ...
# before finally
finally:
    ...



# with line breaks
try: # try
    ...
    # end of body

# before except
except (Exception, ValueError) as exc:  # except line
    ...

# before except 2
except KeyError as key:  # except line 2
    ...
    # in body 2

# before else
else:
    ...

# before finally
finally:
    ...


# with line breaks
try:
    ...

except:
    ...


try:
    ...
except (Exception, Exception, Exception, Exception, Exception, Exception, Exception) as exc:  # splits exception over multiple lines
    ...


try:
    ...
except:
    a = 10 # trailing comment1
    b = 11 # trailing comment2


# try/except*, mostly the same as try
try: # try
    ...
    # end of body
# before except
except* (Exception, ValueError) as exc:  # except line
    ...
# before except 2
except* KeyError as key:  # except line 2
    ...
    # in body 2
# before else
else:
    ...
# before finally
finally:
    ...

# try and try star are statements with body
# Minimized from https://github.com/python/cpython/blob/99b00efd5edfd5b26bf9e2a35cbfc96277fdcbb1/Lib/getpass.py#L68-L91
try:
    try:
        pass
    finally:
        print(1)  # issue7208
except A:
    pass

try:
    f()  # end-of-line last comment
except RuntimeError:
    raise

try:
    def f():
        pass
        # a
except:
    def f():
        pass
        # b
else:
    def f():
        pass
        # c
finally:
    def f():
        pass
        # d

try: pass # a
except ZeroDivisionError: pass # b
except: pass # c
else: pass # d
finally: pass # e

try:  # 1 preceding: any, following: first in body, enclosing: try
    print(1)  # 2 preceding: last in body, following: fist in alt body, enclosing: try
except ZeroDivisionError:  # 3 preceding: test, following: fist in alt body, enclosing: try
    print(2)  # 4 preceding: last in body, following: fist in alt body, enclosing: exc
except:  # 5 preceding: last in body, following: fist in alt body, enclosing: try
    print(2)  # 6 preceding: last in body, following: fist in alt body, enclosing: exc
else:  # 7 preceding: last in body, following: fist in alt body, enclosing: exc
    print(3)  # 8 preceding: last in body, following: fist in alt body, enclosing: try
finally:  # 9 preceding: last in body, following: fist in alt body, enclosing: try
    print(3)  # 10 preceding: last in body, following: any, enclosing: try

try:
    pass
except (
    ZeroDivisionError
    # comment
):
    pass


try:
    pass

finally:
    pass


try:
    pass

except ZeroDivisonError:
    pass

else:
    pass

finally:
    pass
