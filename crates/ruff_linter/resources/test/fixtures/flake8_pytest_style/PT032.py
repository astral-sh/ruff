"""Tests for PT032: Use symbolic attribute reference instead of string literal when patching."""

import module
import unittest.mock


# OK: non-string attribute references
def test_monkeypatch_setattr_symbolic(monkeypatch):
    monkeypatch.setattr(module, module.my_func.__name__, lambda: None)


def test_monkeypatch_setattr_no_attr(monkeypatch):
    # 2-arg string form (different call convention) - not flagged
    monkeypatch.setattr("module.my_func", lambda: None)


def test_monkeypatch_setattr_string_form_string_value(monkeypatch):
    # 2-arg string form where the replacement is also a string literal - must NOT be flagged
    # (the "sqlite://..." is a replacement VALUE at position 1, not an attribute name)
    monkeypatch.setattr("config.DATABASE_URL", "sqlite:///:memory:")


def test_monkeypatch_delattr_string_form(monkeypatch):
    # 1-arg string form for delattr - not flagged
    monkeypatch.delattr("module.my_func")


def test_monkeypatch_delattr_symbolic(monkeypatch):
    monkeypatch.delattr(module, module.my_func.__name__)


def test_mocker_patch_object_symbolic(mocker):
    mocker.patch.object(module, module.my_func.__name__)


def test_mocker_patch_object_symbolic_class_mocker(class_mocker):
    class_mocker.patch.object(module, module.my_func.__name__)


def test_unittest_mock_patch_object_symbolic():
    with unittest.mock.patch.object(module, module.my_func.__name__):
        pass


def test_monkeypatch_setattr_non_string(monkeypatch):
    attr_name = "my_func"
    monkeypatch.setattr(module, attr_name, lambda: None)


def test_mocker_patch_non_object(mocker):
    # mocker.patch (not .object) - not flagged by this rule
    mocker.patch("module.my_func")


def test_unrelated_setattr():
    # Not a monkeypatch call
    setattr(module, "my_func", lambda: None)


# Errors: string literals used as attribute names

def test_monkeypatch_setattr_string_literal(monkeypatch):
    monkeypatch.setattr(module, "my_func", lambda: None)  # PT032


def test_monkeypatch_setattr_string_literal_raising(monkeypatch):
    monkeypatch.setattr(module, "my_func", lambda: None, raising=False)  # PT032


def test_monkeypatch_setattr_string_keyword(monkeypatch):
    monkeypatch.setattr(module, name="my_func", value=lambda: None)  # PT032


def test_monkeypatch_delattr_string_literal(monkeypatch):
    monkeypatch.delattr(module, "my_func")  # PT032


def test_monkeypatch_delattr_string_keyword(monkeypatch):
    monkeypatch.delattr(module, name="my_func")  # PT032


def test_mocker_patch_object_string_literal(mocker):
    mocker.patch.object(module, "my_func")  # PT032


def test_mocker_patch_object_string_keyword(mocker):
    mocker.patch.object(module, attribute="my_func")  # PT032


def test_mocker_patch_object_string_literal_class_mocker(class_mocker):
    class_mocker.patch.object(module, "my_func")  # PT032


def test_mocker_patch_object_string_literal_module_mocker(module_mocker):
    module_mocker.patch.object(module, "my_func")  # PT032


def test_mocker_patch_object_string_literal_package_mocker(package_mocker):
    package_mocker.patch.object(module, "my_func")  # PT032


def test_mocker_patch_object_string_literal_session_mocker(session_mocker):
    session_mocker.patch.object(module, "my_func")  # PT032


def test_mocker_patch_object_string_literal_mock(mock):
    mock.patch.object(module, "my_func")  # PT032


def test_unittest_mock_patch_object_string_literal():
    with unittest.mock.patch.object(module, "my_func"):  # PT032
        pass
