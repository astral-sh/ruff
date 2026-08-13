def test():
 # fmt: off
 a_very_small_indent
 (
not_fixed
 )

 if True:
  pass
  more
 # fmt: on

 formatted

 def test():
  a_small_indent
  # fmt: off
# fix under-indented comments
  (or_the_inner_expression +
expressions
   )

  if True:
   pass
  # fmt: on


# fmt: off
def test():
  pass

  # It is necessary to indent comments because the following fmt: on comment because it otherwise becomes a trailing comment
  # of the `test` function if the "proper" indentation is larger than 2 spaces.
  # fmt: on

disabled +  formatting;

# fmt: on

formatted;

def test():
  pass
  # fmt: off
  """A multiline strings
      that should not get formatted"""

  "A single quoted multiline \
       string"

  disabled +  formatting;

# fmt: on

formatted;
