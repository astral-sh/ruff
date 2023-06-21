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
