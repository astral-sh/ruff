# Basic: general exception before specific
try:
    pass
except Exception:
    pass
except ValueError:  # PLE0701
    pass

# Multi-level hierarchy
try:
    pass
except LookupError:
    pass
except KeyError:  # PLE0701
    pass

# Deep hierarchy: BaseException -> Exception -> RuntimeError -> NotImplementedError
try:
    pass
except BaseException:
    pass
except NotImplementedError:  # PLE0701
    pass

# Tuple handler: general exception in tuple
try:
    pass
except (Exception, TypeError):
    pass
except ValueError:  # PLE0701
    pass

# Multiple specific after general
try:
    pass
except Exception:
    pass
except ValueError:  # PLE0701
    pass
except TypeError:  # PLE0701
    pass

# OSError hierarchy
try:
    pass
except OSError:
    pass
except ConnectionError:  # PLE0701
    pass

# ConnectionError -> BrokenPipeError
try:
    pass
except ConnectionError:
    pass
except BrokenPipeError:  # PLE0701
    pass

# SyntaxError -> IndentationError -> TabError
try:
    pass
except SyntaxError:
    pass
except TabError:  # PLE0701
    pass

# ValueError -> UnicodeError -> UnicodeDecodeError
try:
    pass
except ValueError:
    pass
except UnicodeDecodeError:  # PLE0701
    pass

# Warning hierarchy
try:
    pass
except Warning:
    pass
except DeprecationWarning:  # PLE0701
    pass

# Exception aliases: IOError is an alias for OSError
try:
    pass
except IOError:
    pass
except ConnectionError:  # PLE0701
    pass

# Exception aliases: EnvironmentError is an alias for OSError
try:
    pass
except EnvironmentError:
    pass
except FileNotFoundError:  # PLE0701
    pass

# Specific caught in tuple of later handler
try:
    pass
except Exception:
    pass
except (ValueError, KeyError):  # PLE0701 (for each)
    pass

# OK: correct order (specific before general)
try:
    pass
except ValueError:
    pass
except Exception:
    pass

# OK: unrelated exceptions
try:
    pass
except ValueError:
    pass
except TypeError:
    pass

# OK: same exception (handled by B025)
try:
    pass
except ValueError:
    pass
except ValueError:
    pass

# OK: user-defined exceptions (can't determine hierarchy)
try:
    pass
except MyCustomError:
    pass
except MyOtherError:
    pass

# OK: builtin before user-defined (can't determine hierarchy)
try:
    pass
except Exception:
    pass
except MyCustomError:
    pass

# OK: user-defined before builtin (can't determine hierarchy)
try:
    pass
except MyCustomError:
    pass
except ValueError:
    pass

# OK: bare except last (handled by F707 if not last)
try:
    pass
except ValueError:
    pass
except:
    pass

# Bare except before specific
try:
    pass
except:
    pass
except ValueError:  # PLE0701
    pass

# OK: single handler
try:
    pass
except Exception:
    pass

# Alias of same class: OSError and IOError are the same
try:
    pass
except OSError:
    pass
except IOError:  # PLE0701
    pass

# Alias of same class: EnvironmentError and IOError
try:
    pass
except EnvironmentError:
    pass
except IOError:  # PLE0701
    pass

# OK: builtins access (resolve_builtin_symbol handles this)
import builtins

try:
    pass
except builtins.Exception:
    pass
except builtins.ValueError:  # PLE0701
    pass

# ExceptionGroup diamond: BaseExceptionGroup -> ExceptionGroup
try:
    pass
except BaseExceptionGroup:
    pass
except ExceptionGroup:  # PLE0701
    pass

# except* support
try:
    pass
except* Exception:
    pass
except* ValueError:  # PLE0701
    pass

# OK: except* with user-defined exceptions (can't determine hierarchy)
try:
    pass
except* MyCustomError:
    pass
except* MyOtherError:
    pass

# OK: except* with builtin before user-defined
try:
    pass
except* Exception:
    pass
except* MyCustomError:
    pass

# Binding to a variable with `as` doesn't affect detection
try:
    pass
except Exception as e:
    pass
except ValueError as e:  # PLE0701
    pass

# OK: exception assigned to a variable (no longer a builtin reference)
MyException = Exception

try:
    pass
except MyException:
    pass
except ValueError:
    pass

# OK: shadowed builtin should NOT be flagged
# (must be last - shadows ValueError for rest of file)
ValueError = type("ValueError", (Exception,), {})

try:
    pass
except ValueError:
    pass
except Exception:
    pass
