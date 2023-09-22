def func():
    import numpy as np

    np.round_(np.random.rand(5, 5), 2)
    np.product(np.random.rand(5, 5))
    np.cumproduct(np.random.rand(5, 5))
    np.sometrue(np.random.rand(5, 5))
    np.alltrue(np.random.rand(5, 5))


def func():
    from numpy import round_, product, cumproduct, sometrue, alltrue

    round_(np.random.rand(5, 5), 2)
    product(np.random.rand(5, 5))
    cumproduct(np.random.rand(5, 5))
    sometrue(np.random.rand(5, 5))
    alltrue(np.random.rand(5, 5))
