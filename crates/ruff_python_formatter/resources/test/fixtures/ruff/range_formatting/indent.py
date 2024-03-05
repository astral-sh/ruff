# Formats the entire function with tab or 4 space indentation
# because the statement indentations don't match the preferred indentation.
def test  ():
  print("before" )
  <RANGE_START>1 +  2
  if   True:
      pass
  print("Done" )<RANGE_END>

  print("formatted" )

print("not formatted" )

def test2  ():
  print("before" )
  <RANGE_START>1 +  2
  (
3 + 2
  )
  print("Done" )<RANGE_END>

  print("formatted" )

print("not formatted" )

def test3  ():
  print("before" )
  <RANGE_START>1 +  2
  """A Multiline string
that starts at the beginning of the line and we need to preserve the leading spaces"""

  """A Multiline string
  that has some indentation on the second line and we need to preserve the leading spaces"""

  print("Done" )<RANGE_END>


def test4  ():
  print("before" )
  <RANGE_START>1 +  2
  """A Multiline string
    that uses the same indentation as the formatted code will. This should not be dedented."""

  print("Done" )<RANGE_END>

def test5 ():
  print("before" )
  if True:
      print("Format to fix indentation" )
      print(<RANGE_START>1 +  2)

  else:
      print(3 +  4)<RANGE_END>
      print("Format to fix indentation" )

  pass


def test6 ():
    <RANGE_START>
    print("Format" )
    print(3 +  4)<RANGE_END>
    print("Format to fix indentation" )
