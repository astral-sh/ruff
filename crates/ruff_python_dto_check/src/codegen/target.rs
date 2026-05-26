//! Target spec: a data-driven description of how a [`HandlerKind`] maps to
//! target-language source. The spec is loaded from a TOML/JSON file (the
//! `--target` argument) so a new framework or language slots in by adding a
//! spec entry, **not** new Rust per kind.
//!
//! The first target is `rust-axum-seaorm`; its built-in default spec is
//! returned by [`TargetSpec::rust_axum_seaorm`].

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::contract::HandlerKind;

/// A model mapping entry: how a Python model class resolves to a target model
/// module path.
///
/// `nested` controls the systematic-bug guard from CODEGEN-DESIGN.md: flat
/// models live at `crate::models::<module>::Model` (NOT
/// `crate::models::<module>::<module>::Model`), while ERP models are genuinely
/// nested at `crate::models::erp::<k>::<inner>::Model`.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelMapping {
    /// Path fragment after `crate::models::` up to (but not including) `Model`.
    /// For a flat model this is just the module (`customer`); for a nested ERP
    /// model it is `erp::k6_cash::cash_journal`.
    pub module_path: String,
}

impl ModelMapping {
    /// Full type path to the model's `Model` struct.
    pub fn model_type(&self, root: &str) -> String {
        format!("{root}::{}::Model", self.module_path)
    }
    /// Full path to the `Entity`.
    pub fn entity_path(&self, root: &str) -> String {
        format!("{root}::{}::Entity", self.module_path)
    }
    /// Full path to the `Column` enum.
    pub fn column_path(&self, root: &str) -> String {
        format!("{root}::{}::Column", self.module_path)
    }
}

/// The full target spec.
#[derive(Debug, Clone, Deserialize)]
pub struct TargetSpec {
    /// Stable identifier (e.g. `rust-axum-seaorm`).
    pub id: String,
    /// Root module path for models (e.g. `crate::models`).
    pub models_root: String,
    /// Python-model-name → target module mapping.
    #[serde(default)]
    pub models: BTreeMap<String, ModelMapping>,
    /// Tenant filter column name on the target model (`TenantId`).
    #[serde(default = "default_tenant_column")]
    pub tenant_column: String,
    /// Filesystem root of the source jinja templates (e.g.
    /// `/home/user/WoA/templates`). When set, the list/detail view emitters
    /// resolve the contract's `output` template path under this root and
    /// extract real columns; otherwise they emit a faithful skeleton. Project
    /// specific, so it lives in the spec / config — never hardcoded in the
    /// crate.
    #[serde(default)]
    pub templates_root: Option<String>,
    /// Which handler kinds this target can emit end-to-end. Kinds not listed
    /// emit a documented stub (so the engine is general but honest about
    /// coverage). Stored as the kind's `snake_case` string.
    #[serde(default)]
    pub emit_kinds: Vec<String>,
}

fn default_tenant_column() -> String {
    "TenantId".to_string()
}

impl TargetSpec {
    /// Built-in `rust-axum-seaorm` target with the `WoA` model mappings.
    ///
    /// The flat models here resolve to `crate::models::<module>::Model`
    /// (single segment) — this is the *correct* path; the Sonnet drafts
    /// doubled it to `crate::models::customer::customer::Model`. ERP models
    /// are genuinely nested.
    pub fn rust_axum_seaorm() -> Self {
        let mut models = BTreeMap::new();
        // Flat WoA core models: module == snake(class). Single segment.
        for (class, module) in [
            ("Customer", "customer"),
            ("WorkOrder", "work_order"),
            ("Article", "article"),
            ("Device", "device"),
            ("LogbookEntry", "logbook_entry"),
            ("KummerkastenEntry", "kummerkasten_entry"),
            ("RecurringInvoice", "recurring_invoice"),
            ("MaintenanceContract", "maintenance_contract"),
            ("Project", "project"),
            ("Reminder", "reminder"),
            ("TimeSheet", "time_sheet"),
            ("SecurityAudit", "security_audit"),
            ("ReferralLog", "referral_log"),
            ("SalesPartner", "sales_partner"),
            ("ColdLead", "cold_lead"),
            ("ColdCampaign", "cold_campaign"),
            ("AppVersion", "app_version"),
            ("CustomerPortalUser", "customer_portal_user"),
            ("Document", "document"),
            ("Activity", "activity"),
            ("Picture", "picture"),
            ("Setting", "setting"),
            ("Tenant", "tenant"),
            ("ServicePackage", "service_package"),
            ("RentedServer", "rented_server"),
            ("PasswordEntry", "password_entry"),
            ("Position", "position"),
            ("ProjectNote", "project_note"),
            ("ServiceContract", "service_contract"),
            ("ServiceContractItem", "service_contract_item"),
            ("User", "user"),
            ("AcceptanceItem", "acceptance_item"),
            ("AcceptanceDefect", "acceptance_defect"),
            ("TimesheetActivity", "timesheet_activity"),
            ("HandbookFeature", "handbook_feature"),
            ("IpBlacklist", "ip_blacklist"),
            ("ScopeAuditBlock", "scope_audit_block"),
        ] {
            models.insert(
                class.to_string(),
                ModelMapping {
                    module_path: module.to_string(),
                },
            );
        }
        // ERP models: genuinely nested at erp::<k>::<inner>.
        for (class, k, inner) in [
            ("ErpAuditTrail", "k0_foundation", "erp_audit_trail"),
            ("ErpLedgerLock", "k0_foundation", "erp_ledger_lock"),
            ("ErpAccount", "k1_accounts", "account"),
            ("ErpCostCenter", "k1_accounts", "cost_center"),
            ("ErpFiscalYear", "k1_accounts", "fiscal_year"),
            ("ErpPeriod", "k1_accounts", "period"),
            ("ErpTaxAccountMap", "k1_accounts", "tax_account_map"),
            ("ErpJournal", "k2_journal", "journal"),
            ("ErpDebtor", "k3_debitors", "debtor"),
            ("ErpOpenItemAR", "k3_debitors", "open_item_ar"),
            ("ErpDunningRun", "k3_debitors", "dunning_run"),
            ("ErpCreditor", "k4_creditors", "creditor"),
            ("ErpOpenItemAP", "k4_creditors", "open_item_ap"),
            ("ErpPaymentRun", "k4_creditors", "payment_run"),
            ("ErpBankAccount", "k5_bank", "bank_account"),
            ("ErpBankStatement", "k5_bank", "bank_statement"),
            ("ErpBankMatch", "k5_bank", "bank_match"),
            ("ErpCashJournal", "k6_cash", "cash_journal"),
            ("ErpUstCode", "k7_ust", "ust_code"),
            ("ErpUstVaFiling", "k7_ust", "ust_va_filing"),
            ("ErpFiscalYearClose", "k8_close", "erp_fiscal_year_close"),
            ("ErpAsset", "k9_assets", "asset"),
            ("ErpDepreciation", "k9_assets", "depreciation"),
            ("ErpWarehouse", "k10_inventory", "warehouse"),
            ("ErpInventory", "k10_inventory", "inventory"),
            ("ErpStockMovement", "k10_inventory", "stock_movement"),
            ("ErpSerial", "k10_inventory", "serial"),
            ("ErpPurchaseOrder", "k11_purchase", "purchase_order"),
            ("ErpGoodsReceipt", "k11_purchase", "goods_receipt"),
            ("ErpSupplierInvoice", "k11_purchase", "supplier_invoice"),
        ] {
            models.insert(
                class.to_string(),
                ModelMapping {
                    module_path: format!("erp::{k}::{inner}"),
                },
            );
        }

        TargetSpec {
            id: "rust-axum-seaorm".to_string(),
            models_root: "crate::models".to_string(),
            models,
            tenant_column: "TenantId".to_string(),
            templates_root: None,
            emit_kinds: vec![
                HandlerKind::ListForTenant.as_str().to_string(),
                HandlerKind::SoftDelete.as_str().to_string(),
                HandlerKind::DetailForTenant.as_str().to_string(),
                HandlerKind::TemplateGet.as_str().to_string(),
                HandlerKind::GetRedirectShortcut.as_str().to_string(),
                HandlerKind::ToggleBoolField.as_str().to_string(),
                HandlerKind::CsrfFormPostEngineCall.as_str().to_string(),
                HandlerKind::FormGetPost.as_str().to_string(),
                HandlerKind::AjaxJson.as_str().to_string(),
                HandlerKind::DownloadBlob.as_str().to_string(),
                HandlerKind::PdfRender.as_str().to_string(),
                HandlerKind::SaAdminView.as_str().to_string(),
                HandlerKind::SignedLinkAction.as_str().to_string(),
            ],
        }
    }

    /// Resolve a Python model class to its mapping, if known.
    pub fn resolve_model(&self, python_class: &str) -> Option<&ModelMapping> {
        self.models.get(python_class)
    }

    /// True if this target emits the given kind end-to-end (vs a stub).
    pub fn can_emit(&self, kind: HandlerKind) -> bool {
        self.emit_kinds.iter().any(|k| k == kind.as_str())
    }

    /// Load a target spec from a TOML or JSON file. Extension decides parser.
    pub fn from_path(path: &std::path::Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        if path.extension().is_some_and(|e| e == "toml") {
            Ok(toml_lite::parse_target(&text))
        } else {
            Ok(serde_json::from_str(&text)?)
        }
    }
}

/// A vendored, dependency-free reader for the small subset of TOML the target
/// spec uses (top-level scalars + `[models.<Class>]` tables with a
/// `module_path` string + a top-level `emit_kinds` array). Avoids adding a
/// `toml` workspace dependency for one config file.
mod toml_lite {
    use super::{ModelMapping, TargetSpec};
    use std::collections::BTreeMap;

    pub(super) fn parse_target(text: &str) -> TargetSpec {
        let mut id = String::new();
        let mut models_root = "crate::models".to_string();
        let mut tenant_column = "TenantId".to_string();
        let mut templates_root: Option<String> = None;
        let mut emit_kinds: Vec<String> = Vec::new();
        let mut models: BTreeMap<String, ModelMapping> = BTreeMap::new();

        let mut current_model: Option<String> = None;

        for raw in text.lines() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(table) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                if let Some(cls) = table.strip_prefix("models.") {
                    current_model = Some(cls.trim().to_string());
                } else {
                    current_model = None;
                }
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            match (current_model.as_deref(), key) {
                (Some(cls), "module_path") => {
                    models.insert(
                        cls.to_string(),
                        ModelMapping {
                            module_path: unquote(value),
                        },
                    );
                }
                (None, "id") => id = unquote(value),
                (None, "models_root") => models_root = unquote(value),
                (None, "tenant_column") => tenant_column = unquote(value),
                (None, "templates_root") => templates_root = Some(unquote(value)),
                (None, "emit_kinds") => emit_kinds = parse_array(value),
                _ => {}
            }
        }

        TargetSpec {
            id,
            models_root,
            models,
            tenant_column,
            templates_root,
            emit_kinds,
        }
    }

    fn strip_comment(line: &str) -> &str {
        // Only strip `#` outside of quotes — our values never contain `#`.
        match line.find('#') {
            Some(i) if !line[..i].contains('"') => &line[..i],
            _ => line,
        }
    }

    fn unquote(v: &str) -> String {
        let v = v.trim();
        v.strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(v)
            .to_string()
    }

    fn parse_array(v: &str) -> Vec<String> {
        let inner = v
            .trim()
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or(v);
        inner
            .split(',')
            .map(|s| unquote(s.trim()))
            .filter(|s| !s.is_empty())
            .collect()
    }
}
