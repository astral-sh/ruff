pub(crate) use assert_used::{assert_used, Assert};
pub(crate) use bad_file_permissions::{bad_file_permissions, BadFilePermissions};
pub(crate) use exec_used::{exec_used, ExecBuiltin};
pub(crate) use hardcoded_bind_all_interfaces::{
    hardcoded_bind_all_interfaces, HardcodedBindAllInterfaces,
};
pub(crate) use hardcoded_password_default::{hardcoded_password_default, HardcodedPasswordDefault};
pub(crate) use hardcoded_password_func_arg::{
    hardcoded_password_func_arg, HardcodedPasswordFuncArg,
};
pub(crate) use hardcoded_password_string::{
    assign_hardcoded_password_string, compare_to_hardcoded_password_string, HardcodedPasswordString,
};
pub(crate) use hardcoded_sql_expression::{hardcoded_sql_expression, HardcodedSQLExpression};
pub(crate) use hardcoded_tmp_directory::{hardcoded_tmp_directory, HardcodedTempFile};
pub(crate) use hashlib_insecure_hash_functions::{
    hashlib_insecure_hash_functions, HashlibInsecureHashFunction,
};
pub(crate) use jinja2_autoescape_false::{jinja2_autoescape_false, Jinja2AutoescapeFalse};
pub(crate) use logging_config_insecure_listen::{
    logging_config_insecure_listen, LoggingConfigInsecureListen,
};
pub(crate) use paramiko_calls::{paramiko_call, ParamikoCall};
pub(crate) use request_with_no_cert_validation::{
    request_with_no_cert_validation, RequestWithNoCertValidation,
};
pub(crate) use request_without_timeout::{request_without_timeout, RequestWithoutTimeout};
pub(crate) use shell_injection::{
    shell_injection, CallWithShellEqualsTrue, StartProcessWithAShell, StartProcessWithNoShell,
    StartProcessWithPartialPath, SubprocessPopenWithShellEqualsTrue,
    SubprocessWithoutShellEqualsTrue,
};
pub(crate) use snmp_insecure_version::{snmp_insecure_version, SnmpInsecureVersion};
pub(crate) use snmp_weak_cryptography::{snmp_weak_cryptography, SnmpWeakCryptography};
pub(crate) use suspicious_function_call::{
    suspicious_function_call, SuspiciousEvalUsage, SuspiciousFTPLibUsage,
    SuspiciousInsecureCipherModeUsage, SuspiciousInsecureCipherUsage, SuspiciousInsecureHashUsage,
    SuspiciousMarkSafeUsage, SuspiciousMarshalUsage, SuspiciousMktempUsage,
    SuspiciousNonCryptographicRandomUsage, SuspiciousPickleUsage, SuspiciousTelnetUsage,
    SuspiciousURLOpenUsage, SuspiciousUnverifiedContextUsage, SuspiciousXMLCElementTreeUsage,
    SuspiciousXMLETreeUsage, SuspiciousXMLElementTreeUsage, SuspiciousXMLExpatBuilderUsage,
    SuspiciousXMLExpatReaderUsage, SuspiciousXMLMiniDOMUsage, SuspiciousXMLPullDOMUsage,
    SuspiciousXMLSaxUsage,
};
pub(crate) use try_except_continue::{try_except_continue, TryExceptContinue};
pub(crate) use try_except_pass::{try_except_pass, TryExceptPass};
pub(crate) use unsafe_yaml_load::{unsafe_yaml_load, UnsafeYAMLLoad};

mod assert_used;
mod bad_file_permissions;
mod exec_used;
mod hardcoded_bind_all_interfaces;
mod hardcoded_password_default;
mod hardcoded_password_func_arg;
mod hardcoded_password_string;
mod hardcoded_sql_expression;
mod hardcoded_tmp_directory;
mod hashlib_insecure_hash_functions;
mod jinja2_autoescape_false;
mod logging_config_insecure_listen;
mod paramiko_calls;
mod request_with_no_cert_validation;
mod request_without_timeout;
mod shell_injection;
mod snmp_insecure_version;
mod snmp_weak_cryptography;
mod suspicious_function_call;
mod try_except_continue;
mod try_except_pass;
mod unsafe_yaml_load;
