import unittest
from unittest import TestCase


class DatabaseTestCase(unittest.TestCase):
    def setUp(self):
        # Direct TestCase subclass: no violation expected.
        self.db = "db"

    def tearDown(self):
        del self.db


class GoodChild(DatabaseTestCase):
    def setUp(self):
        super().setUp()
        self.fixture = 1

    def tearDown(self):
        del self.fixture
        super().tearDown()

    @classmethod
    def setUpClass(cls):
        super().setUpClass()
        cls.shared = True

    @classmethod
    def tearDownClass(cls):
        del cls.shared
        super().tearDownClass()


class BadChild(DatabaseTestCase):
    def setUp(self):
        # RUF077
        self.fixture = 1

    def tearDown(self):
        # RUF077
        del self.fixture

    @classmethod
    def setUpClass(cls):
        # RUF077
        cls.shared = True

    @classmethod
    def tearDownClass(cls):
        # RUF077
        del cls.shared


class NestedSuper(DatabaseTestCase):
    def setUp(self):
        with open(__file__) as f:
            super().setUp()
            self.data = f.read()


class Mid(DatabaseTestCase):
    def setUp(self):
        super().setUp()
        self.mid = True


class GrandChild(Mid):
    def setUp(self):
        # RUF077 — Mid is a custom TestCase subclass in this file
        self.leaf = True


class DirectOnly(unittest.TestCase):
    def setUp(self):
        # Direct unittest.TestCase: no violation
        self.x = 1


class DirectAlias(TestCase):
    def setUp(self):
        # Direct alias of unittest.TestCase: no violation
        self.x = 1


class NotATest:
    def setUp(self):
        # Not a TestCase subclass
        self.x = 1
