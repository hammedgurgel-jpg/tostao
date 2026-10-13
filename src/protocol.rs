//! Formato das mensagens de `POST /api/v1/catalog`.
//!
//! A ordem dos campos nas structs é a mesma do literal de objeto no bundle
//! (`chunk.7bd67ec4d1cff4ed86e3.js`); serde serializa na ordem de declaração.
//! Campos `None` são omitidos, como o `JSON.stringify` faz com `undefined`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Rota única usada por todas as operações do catálogo.
pub const CATALOG_PATH: &str = "/api/v1/catalog";

/// Cabeçalho de resposta lido como versão do servidor.
pub const SERVER_VERSION_HEADER: &str = "X-YNAB-Server-Version";

/// Envelope enviado em `/api/v1/catalog`: o nome da operação e o corpo
/// serializado como texto JSON dentro de `request_data`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogRequest {
    pub operation_name: String,
    pub request_data: String,
}

/// Nomes de operação observados no bundle.
pub mod operation {
    pub const SYNC_CATALOG_DATA: &str = "syncCatalogData";
    pub const SYNC_BUDGET_DATA: &str = "syncBudgetData";
    pub const SYNC_FAMILY_DATA: &str = "syncFamilyData";
    pub const LOGIN_USER: &str = "loginUser";
    pub const LOGOUT_USER: &str = "logoutUser";
    pub const GET_INITIAL_USER_DATA: &str = "getInitialUserData";
}

/// `me` no bundle: `Bootstrap="bootstrap"`, `Backfill="backfill"`, `Delta="delta"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncType {
    Bootstrap,
    Backfill,
    Delta,
}

/// Entidades alteradas enviadas pelo aparelho, na ordem do literal
/// `{be_budget:void 0, be_expected_income:void 0, ...}` do bundle.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OutgoingChangedEntities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_budget: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_expected_income: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_accounts: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_account_mappings: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_master_categories: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_monthly_budgets: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_monthly_subcategory_budgets: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_onboarding_events: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_onboarding_targets: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_payees: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_payee_locations: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_payee_rename_conditions: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_settings: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_subcategories: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_scheduled_transaction_groups: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_transaction_groups: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_transaction_images: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_money_movements: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub be_money_movement_groups: Option<Vec<Value>>,
}

/// Corpo de `syncBudgetData`, na ordem do literal `f={budget_version_id, ...}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncBudgetDataRequest {
    pub budget_version_id: String,
    pub sync_type: SyncType,
    pub starting_device_knowledge: i64,
    pub ending_device_knowledge: i64,
    pub device_knowledge_of_server: i64,
    pub calculated_entities_included: bool,
    pub schema_version: i64,
    pub schema_version_of_knowledge: i64,
    pub changed_entities: OutgoingChangedEntities,
}

/// Entidades devolvidas pelo servidor. Nomes vistos na captura de rede
/// (`ev_f8430d8…`); a ordem real no fio não foi observada, então aqui fica
/// a alfabética. Entidades desconhecidas caem em `other`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IncomingChangedEntities {
    pub be_account_calculations: Option<Vec<Value>>,
    pub be_account_mappings: Option<Vec<Value>>,
    pub be_accounts: Option<Vec<Value>>,
    pub be_budget: Option<Value>,
    pub be_expected_income: Option<Value>,
    pub be_master_categories: Option<Vec<Value>>,
    pub be_money_movement_groups: Option<Vec<Value>>,
    pub be_money_movements: Option<Vec<Value>>,
    pub be_monthly_account_calculations: Option<Vec<Value>>,
    pub be_monthly_budget_calculations: Option<Vec<Value>>,
    pub be_monthly_budgets: Option<Vec<Value>>,
    pub be_monthly_subcategory_budget_calculations: Option<Vec<Value>>,
    pub be_monthly_subcategory_budgets: Option<Vec<Value>>,
    pub be_onboarding_events: Option<Vec<Value>>,
    pub be_onboarding_targets: Option<Vec<Value>>,
    pub be_payee_locations: Option<Vec<Value>>,
    pub be_payee_rename_conditions: Option<Vec<Value>>,
    pub be_payees: Option<Vec<Value>>,
    pub be_scheduled_subtransactions: Option<Vec<Value>>,
    pub be_scheduled_transactions: Option<Vec<Value>>,
    pub be_settings: Option<Vec<Value>>,
    pub be_subcategories: Option<Vec<Value>>,
    pub be_subtransactions: Option<Vec<Value>>,
    pub be_transaction_images: Option<Vec<Value>>,
    pub be_transactions: Option<Vec<Value>>,
    pub first_month: Option<Value>,
    pub last_month: Option<Value>,
    #[serde(flatten)]
    pub other: serde_json::Map<String, Value>,
}

/// Resposta de `syncBudgetData`. Campos de topo vistos na captura.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncBudgetDataResponse {
    pub changed_entities: IncomingChangedEntities,
    pub current_server_knowledge: i64,
    pub error: Option<Value>,
    pub schema_version_of_response: i64,
    pub schema_version_of_server: i64,
    pub server_knowledge_of_device: i64,
}
