class MyClass:
  """ Multiline
  class docstring
  """

  def method(self):
    """Multiline
    method docstring
    """
    pass


def foo():
  """This is a docstring with             
  some lines of text here
  """
  return


def bar():
  '''This is another docstring
  with more lines of text
  '''
  return


def baz():
  '''"This" is a string with some
  embedded "quotes"'''
  return


def troz():
	'''Indentation with tabs
	is just as OK
	'''
	return


def zort():
        """Another
        multiline
        docstring
        """
        pass

def poit():
  """
  Lorem ipsum dolor sit amet.       

  Consectetur adipiscing elit:
   - sed do eiusmod tempor incididunt ut labore
   - dolore magna aliqua
     - enim ad minim veniam
     - quis nostrud exercitation ullamco laboris nisi
   - aliquip ex ea commodo consequat
  """
  pass


def under_indent():
  """
  These lines are indented in a way that does not
make sense.
  """
  pass


def over_indent():
  """
  This has a shallow indent
    - But some lines are deeper
    - And the closing quote is too deep
    """
  pass


def single_line():
    """But with a newline after it!

    """
    pass


def this():
    r"""
    'hey ho'
    """


def that():
  """ "hey yah" """


def and_that():
  """
  "hey yah" """


def and_this():
  ''' 
  "hey yah"'''


def multiline_whitespace():
    '''
    
    
    
    
    '''


def oneline_whitespace():
    '''      '''


def empty():
    """"""


def single_quotes():
    'testing'


def believe_it_or_not_this_is_in_the_py_stdlib(): ''' 
"hey yah"'''


def ignored_docstring():
    """a => \
b"""  

def single_line_docstring_with_whitespace():
    """   This should be stripped """

def docstring_with_inline_tabs_and_space_indentation():
    """hey

    tab	separated	value
    	tab at start of line and then a tab	separated	value
    				multiple tabs at the beginning	and	inline
    	 	  	mixed tabs and spaces at beginning. next line has mixed tabs and spaces only.
    			 	  		
    line ends with some tabs		
    """


def docstring_with_inline_tabs_and_tab_indentation():
	"""hey

	tab	separated	value
		tab at start of line and then a tab	separated	value
					multiple tabs at the beginning	and	inline
		 	  	mixed tabs and spaces at beginning. next line has mixed tabs and spaces only.
				 	  		
	line ends with some tabs		
	"""
	pass


def backslash_space():
    """\ """


def multiline_backslash_1():
  '''
  hey\there\
  \ '''


def multiline_backslash_2():
  '''
  hey there \ '''

# Regression test for #3425
def multiline_backslash_really_long_dont_crash():
    """
    hey there hello guten tag hi hoow are you ola zdravstvuyte ciao como estas ca va \ """


def multiline_backslash_3():
  '''
  already escaped \\ '''


def my_god_its_full_of_stars_1():
    "I'm sorry Dave\u2001"


# the space below is actually a \u2001, removed in output
def my_god_its_full_of_stars_2():
    "I'm sorry Dave "


def docstring_almost_at_line_limit():
    """long docstring................................................................."""


def docstring_almost_at_line_limit2():
    """long docstring.................................................................

    ..................................................................................
    """


def docstring_at_line_limit():
    """long docstring................................................................"""


def multiline_docstring_at_line_limit():
    """first line-----------------------------------------------------------------------

    second line----------------------------------------------------------------------"""


def stable_quote_normalization_with_immediate_inner_single_quote(self):
    ''''<text here>

    <text here, since without another non-empty line black is stable>
    '''


def foo():
    """
    Docstring with a backslash followed by a space\ 
    and then another line
    """
