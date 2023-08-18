
@FormattedDecorator(a =b)
 # leading comment
@MyDecorator( # dangling comment
    list = [1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12], x = some_other_function_call({ "test": "value", "more": "other"})) # fmt: skip
  # leading class comment
class Test:
    pass



@FormattedDecorator(a =b)
# leading comment
@MyDecorator( # dangling comment
    list = [1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12], x = some_other_function_call({ "test": "value", "more": "other"})) # fmt: skip
# leading class comment
def test():
    pass

