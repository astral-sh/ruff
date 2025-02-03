This directory vendors some files from actual projects.
This is to benchmark Ruff's performance against real-world
code instead of synthetic benchmarks.

The following files are included:

* [`numpy/globals`](https://github.com/numpy/numpy/blob/89d64415e349ca75a25250f22b874aa16e5c0973/numpy/_globals.py)
* [`numpy/ctypeslib.py`](https://github.com/numpy/numpy/blob/e42c9503a14d66adfd41356ef5640c6975c45218/numpy/ctypeslib.py)
* [`pypinyin.py`](https://github.com/mozillazg/python-pinyin/blob/9521e47d96e3583a5477f5e43a2e82d513f27a3f/pypinyin/standard.py)
* [`pydantic/types.py`](https://github.com/pydantic/pydantic/blob/83b3c49e99ceb4599d9286a3d793cea44ac36d4b/pydantic/types.py)
* [`large/dataset.py`](https://github.com/DHI/mikeio/blob/b7d26418f4db2909b0aa965253dbe83194d7bb5b/tests/test_dataset.py)
* [`tomllib`](https://github.com/python/cpython/tree/8e8a4baf652f6e1cee7acde9d78c4b6154539748/Lib/tomllib) (3.12)

The files are included in the `resources` directory to allow
running benchmarks offline and for simplicity. They're licensed
according to their original licenses (see link).
