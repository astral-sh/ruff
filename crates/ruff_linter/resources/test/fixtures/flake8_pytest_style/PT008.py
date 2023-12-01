# OK

mocker.patch("module.name", not_lambda)
module_mocker.patch("module.name", not_lambda)
mocker.patch.object(obj, "attr", not_lambda)
module_mocker.patch.object(obj, "attr", not_lambda)

mocker.patch("module.name", return_value=None)
module_mocker.patch("module.name", return_value=None)
mocker.patch.object(obj, "attr", return_value=None)
module_mocker.patch.object(obj, "attr", return_value=None)

mocker.patch("module.name", lambda x, y: x)
module_mocker.patch("module.name", lambda x, y: x)
mocker.patch.object(obj, "attr", lambda x, y: x)
module_mocker.patch.object(obj, "attr", lambda x, y: x)

mocker.patch("module.name", lambda *args: args)
module_mocker.patch("module.name", lambda *args: args)
mocker.patch.object(obj, "attr", lambda *args: args)
module_mocker.patch.object(obj, "attr", lambda *args: args)

mocker.patch("module.name", lambda **kwargs: kwargs)
module_mocker.patch("module.name", lambda **kwargs: kwargs)
mocker.patch.object(obj, "attr", lambda **kwargs: kwargs)
module_mocker.patch.object(obj, "attr", lambda **kwargs: kwargs)

mocker.patch("module.name", lambda x, /, y: x)
module_mocker.patch("module.name", lambda x, /, y: x)
mocker.patch.object(obj, "attr", lambda x, /, y: x)
module_mocker.patch.object(obj, "attr", lambda x, /, y: x)

# Error

mocker.patch("module.name", lambda: None)
module_mocker.patch("module.name", lambda: None)
mocker.patch.object(obj, "attr", lambda: None)
module_mocker.patch.object(obj, "attr", lambda: None)

mocker.patch("module.name", lambda x, y: None)
module_mocker.patch("module.name", lambda x, y: None)
mocker.patch.object(obj, "attr", lambda x, y: None)
module_mocker.patch.object(obj, "attr", lambda x, y: None)

mocker.patch("module.name", lambda *args, **kwargs: None)
module_mocker.patch("module.name", lambda *args, **kwargs: None)
mocker.patch.object(obj, "attr", lambda *args, **kwargs: None)
module_mocker.patch.object(obj, "attr", lambda *args, **kwargs: None)
