############# Warning ############
# This file contains form feeds. #
############# Warning ############


# Errors

 

		

def _():
		pass

if False:
    print('F')
    print('T')

# Multiple form feeds in leading whitespace (https://github.com/astral-sh/ruff/issues/16139)
if True:
 print("!")
if True:
  print("!")


# No errors



  

def _():
    pass

def f():
	pass 
