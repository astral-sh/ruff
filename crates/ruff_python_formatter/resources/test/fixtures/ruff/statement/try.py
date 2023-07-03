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
