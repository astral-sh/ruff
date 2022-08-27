#!/usr/bin/env python
"""Wrapper around Flake8 to enable multiprocessing on all operating systems.

As of Python 3.8, macOS's default "start method" for multiprocessing is `spawn`. Flake8
requires a "start method" of `fork`, and disables multiprocessing if it detects `spawn`
or some other "start method". This script enables the `fork` start method before passing
along any command-line arguments to `flake8`.

This has never caused me any problems, but note that they disabled this for a reason:
Flake8's plugin interface doesn't work with `spawn`, and the maintainer says that `fork`
is "pretty broken" on macOS.

See:

- https://github.com/pycqa/flake8/issues/955
- https://github.com/PyCQA/flake8/issues/1337
- https://github.com/PyCQA/flake8/issues/342
- https://github.com/PyCQA/flake8/pull/1621

Example usage: python -m run_flake8 --select=E501 .
"""
import multiprocessing
import sys

from flake8.main import cli

if __name__ == "__main__":
    multiprocessing.set_start_method("fork", force=True)
    cli.main(sys.argv[1:])
