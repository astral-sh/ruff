import unittest


def Bad():
    pass


def _Bad():
    pass


def BAD():
    pass


def BAD_FUNC():
    pass


def good():
    pass


def _good():
    pass


def good_func():
    pass


def tearDownModule():
    pass


class Test(unittest.TestCase):
    def tearDown(self):
        return super().tearDown()

    def testTest(self):
        assert True


from typing import override, overload


@override
def BAD_FUNC():
    pass


@overload
def BAD_FUNC():
    pass


import ast
from ast import NodeTransformer


class Visitor(ast.NodeVisitor):
    def visit_Constant(self, node):
        pass

    def bad_Name(self):
        pass


class Transformer(NodeTransformer):
    def visit_Constant(self, node):
        pass


from http.server import BaseHTTPRequestHandler


class MyRequestHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        pass

    def dont_GET(self):
        pass
