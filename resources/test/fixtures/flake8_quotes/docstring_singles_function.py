def foo():
    '''function without params, single line docstring'''
    ''' not a docstring'''
    return


def foo2():
    '''
        function without params, multiline docstring
    '''
    ''' not a docstring'''
    return


def fun_with_params_no_docstring(a, b='''
    not a
''' '''docstring'''):
    pass


def fun_with_params_no_docstring2(a, b=c[foo():], c=\
    ''' not a docstring '''):
    pass


def function_with_single_docstring(a):
    'Single line docstring'


def double_inside_single(a):
    "Double inside 'single '"
