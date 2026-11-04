# Scalar and tuple indices retain their meaning.
data[(index)]
data[(first, second)]
data[(only,)]
data[()]
data[(first, second), (third, fourth)]
data[:, (first, second)]
data[((first, second),)]

# Keep unpacking compatible with Python versions before PEP 646.
data[(*values,)]
data[(first, *values)]
data[(value := result,)]

# Expression grammar and comments can require parentheses.
data[(lambda: result)]
data[(value := result)]
data[(first if condition else second)]
data[(  # comment on the opening parenthesis
    index
)]

def generate():
    data[(yield item)]
    data[((yield item),)]
