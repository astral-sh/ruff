# E201: Whitespace after '(', '[' or '{'

def foo():
    return (1, 2)

x = [1, 2]
x2 = [ 1, 2]

y = {1: 2}
y2 = { 1: 2}

z = (1, 2)
z2 = ( 1, 2)


def fun( x, y):
    pass

fun(1, 2)
fun( 1, 3)

my_dict = {'key': 'value'}
my_dict2 = { 'key': 'value' }
# trailing whitespace after '{' below
my_dict3 = { 
    'key': 'value'
}