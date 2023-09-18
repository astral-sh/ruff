pass

# fmt: off
  # A comment that falls into the verbatim range
a +   b # a trailing comment

# in between comments

# function comment
def test():
    pass

  # under indent

    def nested():
        ...

        # trailing comment that falls into the verbatim range
      # trailing outer comment
  # fmt: on

a +   b

def test():
    pass
    # fmt: off
    # a trailing comment

