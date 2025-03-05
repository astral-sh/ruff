############# Warning ############
# This file contains form feeds. #
############# Warning ############


# Errors

  # hereY

		

def _():
		pass

if False:
    print('F')
    print('T')


# No errors





  

def _():
    pass

def f():
	pass 

# From https://github.com/astral-sh/ruff/issues/16139#issuecomment-2692317519

# these should raise errors
if True:
  print("!")

if True:
 print("!")

# this should not raise an error as is a logical line
def f():\
    print("!")


