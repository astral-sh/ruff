"""This module provides access to the Windows registry API.

Functions:

CloseKey() - Closes a registry key.
ConnectRegistry() - Establishes a connection to a predefined registry handle
                    on another computer.
CreateKey() - Creates the specified key, or opens it if it already exists.
DeleteKey() - Deletes the specified key.
DeleteValue() - Removes a named value from the specified registry key.
DeleteTree() - Deletes the specified key and all its subkeys and values recursively.
EnumKey() - Enumerates subkeys of the specified open registry key.
EnumValue() - Enumerates values of the specified open registry key.
ExpandEnvironmentStrings() - Expand the env strings in a REG_EXPAND_SZ
                             string.
FlushKey() - Writes all the attributes of the specified key to the registry.
LoadKey() - Creates a subkey under HKEY_USER or HKEY_LOCAL_MACHINE and
            stores registration information from a specified file into that
            subkey.
OpenKey() - Opens the specified key.
OpenKeyEx() - Alias of OpenKey().
QueryValue() - Retrieves the value associated with the unnamed value for a
               specified key in the registry.
QueryValueEx() - Retrieves the type and data for a specified value name
                 associated with an open registry key.
QueryInfoKey() - Returns information about the specified key.
SaveKey() - Saves the specified key, and all its subkeys a file.
SetValue() - Associates a value with a specified key.
SetValueEx() - Stores data in the value field of an open registry key.

Special objects:

HKEYType -- type object for HKEY objects
error -- exception raised for Win32 errors

Integer constants:
Many constants are defined - see the documentation for each function
to see what constants are used, and where.
"""

import sys
from _typeshed import ReadableBuffer, Unused
from types import TracebackType
from typing import Any, Final, Literal, final, overload
from typing_extensions import Self, TypeAlias

if sys.platform == "win32":
    _KeyType: TypeAlias = HKEYType | int
    def CloseKey(hkey: _KeyType, /) -> None:
        """Closes a previously opened registry key.

          hkey
            A previously opened key.

        Note that if the key is not closed using this method, it will be
        closed when the hkey object is destroyed by Python.
        """

    def ConnectRegistry(computer_name: str | None, key: _KeyType, /) -> HKEYType:
        """Establishes a connection to the registry on another computer.

          computer_name
            The name of the remote computer, of the form r"\\\\computername".  If
            None, the local computer is used.
          key
            The predefined key to connect to.

        The return value is the handle of the opened key.
        If the function fails, an OSError exception is raised.
        """

    def CreateKey(key: _KeyType, sub_key: str | None, /) -> HKEYType:
        """Creates or opens the specified key.

          key
            An already open key, or one of the predefined HKEY_* constants.
          sub_key
            The name of the key this method opens or creates.

        If key is one of the predefined keys, sub_key may be None. In that case,
        the handle returned is the same key handle passed in to the function.

        If the key already exists, this function opens the existing key.

        The return value is the handle of the opened key.
        If the function fails, an OSError exception is raised.
        """

    def CreateKeyEx(key: _KeyType, sub_key: str | None, reserved: int = 0, access: int = 131078) -> HKEYType:
        """Creates or opens the specified key.

          key
            An already open key, or one of the predefined HKEY_* constants.
          sub_key
            The name of the key this method opens or creates.
          reserved
            A reserved integer, and must be zero.  Default is zero.
          access
            An integer that specifies an access mask that describes the
            desired security access for the key. Default is KEY_WRITE.

        If key is one of the predefined keys, sub_key may be None. In that case,
        the handle returned is the same key handle passed in to the function.

        If the key already exists, this function opens the existing key

        The return value is the handle of the opened key.
        If the function fails, an OSError exception is raised.
        """

    def DeleteKey(key: _KeyType, sub_key: str, /) -> None:
        """Deletes the specified key.

          key
            An already open key, or any one of the predefined HKEY_* constants.
          sub_key
            A string that must be the name of a subkey of the key identified by
            the key parameter. This value must not be None, and the key may not
            have subkeys.

        This method can not delete keys with subkeys.

        If the function succeeds, the entire key, including all of its values,
        is removed.  If the function fails, an OSError exception is raised.
        """

    def DeleteKeyEx(key: _KeyType, sub_key: str, access: int = 256, reserved: int = 0) -> None:
        """Deletes the specified key (intended for 64-bit OS).

          key
            An already open key, or any one of the predefined HKEY_* constants.
          sub_key
            A string that must be the name of a subkey of the key identified by
            the key parameter. This value must not be None, and the key may not
            have subkeys.
          access
            An integer that specifies an access mask that describes the
            desired security access for the key. Default is KEY_WOW64_64KEY.
          reserved
            A reserved integer, and must be zero.  Default is zero.

        While this function is intended to be used for 64-bit OS, it is also
         available on 32-bit systems.

        This method can not delete keys with subkeys.

        If the function succeeds, the entire key, including all of its values,
        is removed.  If the function fails, an OSError exception is raised.
        On unsupported Windows versions, NotImplementedError is raised.
        """

    def DeleteValue(key: _KeyType, value: str, /) -> None:
        """Removes a named value from a registry key.

        key
          An already open key, or any one of the predefined HKEY_* constants.
        value
          A string that identifies the value to remove.
        """

    def EnumKey(key: _KeyType, index: int, /) -> str:
        """Enumerates subkeys of an open registry key.

          key
            An already open key, or any one of the predefined HKEY_* constants.
          index
            An integer that identifies the index of the key to retrieve.

        The function retrieves the name of one subkey each time it is called.
        It is typically called repeatedly until an OSError exception is
        raised, indicating no more values are available.
        """

    def EnumValue(key: _KeyType, index: int, /) -> tuple[str, Any, int]:
        """Enumerates values of an open registry key.

          key
            An already open key, or any one of the predefined HKEY_* constants.
          index
            An integer that identifies the index of the value to retrieve.

        The function retrieves the name of one subkey each time it is called.
        It is typically called repeatedly, until an OSError exception
        is raised, indicating no more values.

        The result is a tuple of 3 items:
          value_name
            A string that identifies the value.
          value_data
            An object that holds the value data, and whose type depends
            on the underlying registry type.
          data_type
            An integer that identifies the type of the value data.
        """

    def ExpandEnvironmentStrings(string: str, /) -> str:
        """Expand environment vars."""

    def FlushKey(key: _KeyType, /) -> None:
        """Writes all the attributes of a key to the registry.

          key
            An already open key, or any one of the predefined HKEY_* constants.

        It is not necessary to call FlushKey to change a key.  Registry changes
        are flushed to disk by the registry using its lazy flusher.  Registry
        changes are also flushed to disk at system shutdown.  Unlike
        CloseKey(), the FlushKey() method returns only when all the data has
        been written to the registry.

        An application should only call FlushKey() if it requires absolute
        certainty that registry changes are on disk.  If you don't know whether
        a FlushKey() call is required, it probably isn't.
        """

    def LoadKey(key: _KeyType, sub_key: str, file_name: str, /) -> None:
        """Insert data into the registry from a file.

          key
            An already open key, or any one of the predefined HKEY_* constants.
          sub_key
            A string that identifies the sub-key to load.
          file_name
            The name of the file to load registry data from.  This file must
            have been created with the SaveKey() function.  Under the file
            allocation table (FAT) file system, the filename may not have an
            extension.

        Creates a subkey under the specified key and stores registration
        information from a specified file into that subkey.

        A call to LoadKey() fails if the calling process does not have the
        SE_RESTORE_PRIVILEGE privilege.

        If key is a handle returned by ConnectRegistry(), then the path
        specified in fileName is relative to the remote computer.

        The MSDN docs imply key must be in the HKEY_USER or HKEY_LOCAL_MACHINE
        tree.
        """

    def OpenKey(key: _KeyType, sub_key: str | None, reserved: int = 0, access: int = 131097) -> HKEYType:
        """Opens the specified key.

          key
            An already open key, or any one of the predefined HKEY_* constants.
          sub_key
            A string that identifies the sub_key to open.
          reserved
            A reserved integer that must be zero.  Default is zero.
          access
            An integer that specifies an access mask that describes the desired
            security access for the key.  Default is KEY_READ.

        The result is a new handle to the specified key.
        If the function fails, an OSError exception is raised.
        """

    def OpenKeyEx(key: _KeyType, sub_key: str | None, reserved: int = 0, access: int = 131097) -> HKEYType:
        """Opens the specified key.

          key
            An already open key, or any one of the predefined HKEY_* constants.
          sub_key
            A string that identifies the sub_key to open.
          reserved
            A reserved integer that must be zero.  Default is zero.
          access
            An integer that specifies an access mask that describes the desired
            security access for the key.  Default is KEY_READ.

        The result is a new handle to the specified key.
        If the function fails, an OSError exception is raised.
        """

    def QueryInfoKey(key: _KeyType, /) -> tuple[int, int, int]:
        """Returns information about a key.

          key
            An already open key, or any one of the predefined HKEY_* constants.

        The result is a tuple of 3 items:
        An integer that identifies the number of sub keys this key has.
        An integer that identifies the number of values this key has.
        An integer that identifies when the key was last modified (if available)
        as 100's of nanoseconds since Jan 1, 1600.
        """

    def QueryValue(key: _KeyType, sub_key: str | None, /) -> str:
        """Retrieves the unnamed value for a key.

          key
            An already open key, or any one of the predefined HKEY_* constants.
          sub_key
            A string that holds the name of the subkey with which the value
            is associated.  If this parameter is None or empty, the function
            retrieves the value set by the SetValue() method for the key
            identified by key.

        Values in the registry have name, type, and data components. This method
        retrieves the data for a key's first value that has a NULL name.
        But since the underlying API call doesn't return the type, you'll
        probably be happier using QueryValueEx; this function is just here for
        completeness.
        """

    def QueryValueEx(key: _KeyType, name: str, /) -> tuple[Any, int]:
        """Retrieves the type and value of a specified sub-key.

          key
            An already open key, or any one of the predefined HKEY_* constants.
          name
            A string indicating the value to query.

        Behaves mostly like QueryValue(), but also returns the type of the
        specified value name associated with the given open registry key.

        The return value is a tuple of the value and the type_id.
        """

    def SaveKey(key: _KeyType, file_name: str, /) -> None:
        """Saves the specified key, and all its subkeys to the specified file.

          key
            An already open key, or any one of the predefined HKEY_* constants.
          file_name
            The name of the file to save registry data to.  This file cannot
            already exist. If this filename includes an extension, it cannot be
            used on file allocation table (FAT) file systems by the LoadKey(),
            ReplaceKey() or RestoreKey() methods.

        If key represents a key on a remote computer, the path described by
        file_name is relative to the remote computer.

        The caller of this method must possess the SeBackupPrivilege
        security privilege.  This function passes NULL for security_attributes
        to the API.
        """

    def SetValue(key: _KeyType, sub_key: str | None, type: int, value: str, /) -> None:
        """Associates a value with a specified key.

          key
            An already open key, or any one of the predefined HKEY_* constants.
          sub_key
            A string that names the subkey with which the value is associated.
          type
            An integer that specifies the type of the data.  Currently this must
            be REG_SZ, meaning only strings are supported.
          value
            A string that specifies the new value.

        If the key specified by the sub_key parameter does not exist, the
        SetValue function creates it.

        Value lengths are limited by available memory. Long values (more than
        2048 bytes) should be stored as files with the filenames stored in
        the configuration registry to help the registry perform efficiently.

        The key identified by the key parameter must have been opened with
        KEY_SET_VALUE access.
        """

    @overload  # type=REG_DWORD|REG_QWORD
    def SetValueEx(key: _KeyType, value_name: str | None, reserved: Unused, type: Literal[4, 5], value: int | None, /) -> None:
        """Stores data in the value field of an open registry key.

          key
            An already open key, or any one of the predefined HKEY_* constants.
          value_name
            A string containing the name of the value to set, or None.
          reserved
            Can be anything - zero is always passed to the API.
          type
            An integer that specifies the type of the data, one of:
            REG_BINARY -- Binary data in any form.
            REG_DWORD -- A 32-bit number.
            REG_DWORD_LITTLE_ENDIAN -- A 32-bit number in little-endian format. Equivalent to REG_DWORD
            REG_DWORD_BIG_ENDIAN -- A 32-bit number in big-endian format.
            REG_EXPAND_SZ -- A null-terminated string that contains unexpanded
                             references to environment variables (for example,
                             %PATH%).
            REG_LINK -- A Unicode symbolic link.
            REG_MULTI_SZ -- A sequence of null-terminated strings, terminated
                            by two null characters.  Note that Python handles
                            this termination automatically.
            REG_NONE -- No defined value type.
            REG_QWORD -- A 64-bit number.
            REG_QWORD_LITTLE_ENDIAN -- A 64-bit number in little-endian format. Equivalent to REG_QWORD.
            REG_RESOURCE_LIST -- A device-driver resource list.
            REG_SZ -- A null-terminated string.
          value
            A string that specifies the new value.

        This method can also set additional value and type information for the
        specified key.  The key identified by the key parameter must have been
        opened with KEY_SET_VALUE access.

        To open the key, use the CreateKeyEx() or OpenKeyEx() methods.

        Value lengths are limited by available memory. Long values (more than
        2048 bytes) should be stored as files with the filenames stored in
        the configuration registry to help the registry perform efficiently.
        """

    @overload  # type=REG_SZ|REG_EXPAND_SZ
    def SetValueEx(
        key: _KeyType, value_name: str | None, reserved: Unused, type: Literal[1, 2], value: str | None, /
    ) -> None: ...
    @overload  # type=REG_MULTI_SZ
    def SetValueEx(
        key: _KeyType, value_name: str | None, reserved: Unused, type: Literal[7], value: list[str] | None, /
    ) -> None: ...
    @overload  # type=REG_BINARY and everything else
    def SetValueEx(
        key: _KeyType,
        value_name: str | None,
        reserved: Unused,
        type: Literal[0, 3, 8, 9, 10, 11],
        value: ReadableBuffer | None,
        /,
    ) -> None: ...
    @overload  # Unknown or undocumented
    def SetValueEx(
        key: _KeyType,
        value_name: str | None,
        reserved: Unused,
        type: int,
        value: int | str | list[str] | ReadableBuffer | None,
        /,
    ) -> None: ...
    def DisableReflectionKey(key: _KeyType, /) -> None:
        """Disables registry reflection for 32bit processes running on a 64bit OS.

          key
            An already open key, or any one of the predefined HKEY_* constants.

        Will generally raise NotImplementedError if executed on a 32bit OS.

        If the key is not on the reflection list, the function succeeds but has
        no effect.  Disabling reflection for a key does not affect reflection
        of any subkeys.
        """

    def EnableReflectionKey(key: _KeyType, /) -> None:
        """Restores registry reflection for the specified disabled key.

          key
            An already open key, or any one of the predefined HKEY_* constants.

        Will generally raise NotImplementedError if executed on a 32bit OS.
        Restoring reflection for a key does not affect reflection of any
        subkeys.
        """

    def QueryReflectionKey(key: _KeyType, /) -> bool:
        """Returns the reflection state for the specified key as a bool.

          key
            An already open key, or any one of the predefined HKEY_* constants.

        Will generally raise NotImplementedError if executed on a 32bit OS.
        """
    HKEY_CLASSES_ROOT: Final[int]
    HKEY_CURRENT_USER: Final[int]
    HKEY_LOCAL_MACHINE: Final[int]
    HKEY_USERS: Final[int]
    HKEY_PERFORMANCE_DATA: Final[int]
    HKEY_CURRENT_CONFIG: Final[int]
    HKEY_DYN_DATA: Final[int]

    KEY_ALL_ACCESS: Final = 983103
    KEY_WRITE: Final = 131078
    KEY_READ: Final = 131097
    KEY_EXECUTE: Final = 131097
    KEY_QUERY_VALUE: Final = 1
    KEY_SET_VALUE: Final = 2
    KEY_CREATE_SUB_KEY: Final = 4
    KEY_ENUMERATE_SUB_KEYS: Final = 8
    KEY_NOTIFY: Final = 16
    KEY_CREATE_LINK: Final = 32

    KEY_WOW64_64KEY: Final = 256
    KEY_WOW64_32KEY: Final = 512

    REG_BINARY: Final = 3
    REG_DWORD: Final = 4
    REG_DWORD_LITTLE_ENDIAN: Final = 4
    REG_DWORD_BIG_ENDIAN: Final = 5
    REG_EXPAND_SZ: Final = 2
    REG_LINK: Final = 6
    REG_MULTI_SZ: Final = 7
    REG_NONE: Final = 0
    REG_QWORD: Final = 11
    REG_QWORD_LITTLE_ENDIAN: Final = 11
    REG_RESOURCE_LIST: Final = 8
    REG_FULL_RESOURCE_DESCRIPTOR: Final = 9
    REG_RESOURCE_REQUIREMENTS_LIST: Final = 10
    REG_SZ: Final = 1

    REG_CREATED_NEW_KEY: Final = 1  # undocumented
    REG_LEGAL_CHANGE_FILTER: Final = 268435471  # undocumented
    REG_LEGAL_OPTION: Final = 31  # undocumented
    REG_NOTIFY_CHANGE_ATTRIBUTES: Final = 2  # undocumented
    REG_NOTIFY_CHANGE_LAST_SET: Final = 4  # undocumented
    REG_NOTIFY_CHANGE_NAME: Final = 1  # undocumented
    REG_NOTIFY_CHANGE_SECURITY: Final = 8  # undocumented
    REG_NO_LAZY_FLUSH: Final = 4  # undocumented
    REG_OPENED_EXISTING_KEY: Final = 2  # undocumented
    REG_OPTION_BACKUP_RESTORE: Final = 4  # undocumented
    REG_OPTION_CREATE_LINK: Final = 2  # undocumented
    REG_OPTION_NON_VOLATILE: Final = 0  # undocumented
    REG_OPTION_OPEN_LINK: Final = 8  # undocumented
    REG_OPTION_RESERVED: Final = 0  # undocumented
    REG_OPTION_VOLATILE: Final = 1  # undocumented
    REG_REFRESH_HIVE: Final = 2  # undocumented
    REG_WHOLE_HIVE_VOLATILE: Final = 1  # undocumented

    error = OSError

    # Though this class has a __name__ of PyHKEY, it's exposed as HKEYType for some reason
    @final
    class HKEYType:
        """PyHKEY Object - A Python object, representing a win32 registry key.

        This object wraps a Windows HKEY object, automatically closing it when
        the object is destroyed.  To guarantee cleanup, you can call either
        the Close() method on the PyHKEY, or the CloseKey() method.

        All functions which accept a handle object also accept an integer --
        however, use of the handle object is encouraged.

        Functions:
        Close() - Closes the underlying handle.
        Detach() - Returns the integer Win32 handle, detaching it from the object

        Properties:
        handle - The integer Win32 handle.

        Operations:
        __bool__ - Handles with an open object return true, otherwise false.
        __int__ - Converting a handle to an integer returns the Win32 handle.
        __enter__, __exit__ - Context manager support for 'with' statement,
        automatically closes handle.
        """

        def __bool__(self) -> bool:
            """True if self else False"""

        def __int__(self) -> int:
            """int(self)"""

        def __enter__(self) -> Self: ...
        def __exit__(
            self, exc_type: type[BaseException] | None, exc_value: BaseException | None, traceback: TracebackType | None, /
        ) -> bool | None: ...
        def Close(self) -> None:
            """Closes the underlying Windows handle.

            If the handle is already closed, no error is raised.
            """

        def Detach(self) -> int:
            """Detaches the Windows handle from the handle object.

            The result is the value of the handle before it is detached.  If the
            handle is already detached, this will return zero.

            After calling this function, the handle is effectively invalidated,
            but the handle is not closed.  You would call this function when you
            need the underlying win32 handle to exist beyond the lifetime of the
            handle object.
            """

        def __hash__(self) -> int: ...
        @property
        def handle(self) -> int: ...
