"""Test that a global import which is redefined locally, but used later in another scope does not generate a warning."""

import unittest, transport


class GetTransportTestCase(unittest.TestCase):
    def test_get_transport(self):
        transport = 'transport'
        self.assertIsNotNone(transport)


class TestTransportMethodArgs(unittest.TestCase):
    def test_send_defaults(self):
        transport.Transport()
