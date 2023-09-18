use num_traits::{One, Zero};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Constant, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of SNMPv1 or SNMPv2.
///
/// ## Why is this bad?
/// The SNMPv1 and SNMPv2 protocols are considered insecure as they do
/// not support encryption. Instead, prefer SNMPv3, which supports
/// encryption.
///
/// ## Example
/// ```python
/// from pysnmp.hlapi import CommunityData
///
/// CommunityData("public", mpModel=0)
/// ```
///
/// Use instead:
/// ```python
/// from pysnmp.hlapi import CommunityData
///
/// CommunityData("public", mpModel=2)
/// ```
///
/// ## References
/// - [Cybersecurity and Infrastructure Security Agency (CISA): Alert TA17-156A](https://www.cisa.gov/news-events/alerts/2017/06/05/reducing-risk-snmp-abuse)
/// - [Common Weakness Enumeration: CWE-319](https://cwe.mitre.org/data/definitions/319.html)
#[violation]
pub struct SnmpInsecureVersion;

impl Violation for SnmpInsecureVersion {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("The use of SNMPv1 and SNMPv2 is insecure. Use SNMPv3 if able.")
    }
}

/// S508
pub(crate) fn snmp_insecure_version(checker: &mut Checker, call: &ast::ExprCall) {
    if checker
        .semantic()
        .resolve_call_path(&call.func)
        .is_some_and(|call_path| {
            matches!(call_path.as_slice(), ["pysnmp", "hlapi", "CommunityData"])
        })
    {
        if let Some(keyword) = call.arguments.find_keyword("mpModel") {
            if let Expr::Constant(ast::ExprConstant {
                value: Constant::Int(value),
                ..
            }) = &keyword.value
            {
                if value.is_zero() || value.is_one() {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(SnmpInsecureVersion, keyword.range()));
                }
            }
        }
    }
}
