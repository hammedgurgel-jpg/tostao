//! Lado servidor de `syncBudgetData`, com armazenamento em memória.
//!
//! Cada orçamento tem um contador de conhecimento do servidor que sobe uma
//! vez por lote de mudanças aceito; cada entidade guarda o valor do contador
//! em que mudou pela última vez. Um aparelho recebe as entidades com
//! conhecimento maior do que o `device_knowledge_of_server` que ele enviou.
//!
//! Entidades são `serde_json::Value` com um campo `id` texto; a última
//! versão recebida substitui a anterior inteira.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

use crate::api::{ApiError, HttpResponse, ServerError, Transport};
use crate::protocol::{
    operation, CatalogRequest, IncomingChangedEntities, SyncBudgetDataRequest,
    SyncBudgetDataResponse, SyncType, CATALOG_PATH, SERVER_VERSION_HEADER,
};
use crate::sync::{ERR_NO_READ_PERMISSIONS, ERR_SERVER_KNOWLEDGE_OF_DEVICE_EXCEEDS};

pub const ERR_INVALID_REQUEST: &str = "invalid_request";
pub const ERR_UNKNOWN_OPERATION: &str = "unknown_operation";

/// Tipos que vão e voltam com o mesmo nome e são listas.
const LIST_KINDS: &[&str] = &[
    "be_accounts",
    "be_account_mappings",
    "be_master_categories",
    "be_monthly_budgets",
    "be_monthly_subcategory_budgets",
    "be_onboarding_events",
    "be_onboarding_targets",
    "be_payees",
    "be_payee_locations",
    "be_payee_rename_conditions",
    "be_settings",
    "be_subcategories",
    "be_transaction_images",
    "be_money_movements",
    "be_money_movement_groups",
];

/// Tipos que vão e voltam como um objeto só.
const SINGLE_KINDS: &[&str] = &["be_budget", "be_expected_income"];

/// Grupos enviados pelo aparelho (`entityToServerGroup` no bundle):
/// `{id, be_transaction, be_subtransactions}`. O servidor os guarda
/// desmontados e devolve as partes em listas separadas.
struct GroupKind {
    outgoing: &'static str,
    parent_field: &'static str,
    children_field: &'static str,
    parent_kind: &'static str,
    children_kind: &'static str,
}

const GROUP_KINDS: &[GroupKind] = &[
    GroupKind {
        outgoing: "be_transaction_groups",
        parent_field: "be_transaction",
        children_field: "be_subtransactions",
        parent_kind: "be_transactions",
        children_kind: "be_subtransactions",
    },
    GroupKind {
        outgoing: "be_scheduled_transaction_groups",
        parent_field: "be_scheduled_transaction",
        children_field: "be_scheduled_subtransactions",
        parent_kind: "be_scheduled_transactions",
        children_kind: "be_scheduled_subtransactions",
    },
];

#[derive(Debug, Clone)]
struct StoredEntity {
    value: Value,
    server_knowledge: i64,
}

#[derive(Debug, Default)]
struct BudgetStore {
    readers: HashSet<String>,
    current_server_knowledge: i64,
    /// Até onde o servidor recebeu as mudanças de cada aparelho.
    knowledge_of_devices: HashMap<String, i64>,
    /// tipo -> id -> entidade
    entities: BTreeMap<&'static str, BTreeMap<String, StoredEntity>>,
}

impl BudgetStore {
    fn changed_since(&self, since: i64) -> Map<String, Value> {
        let mut out = Map::new();
        for kind in LIST_KINDS
            .iter()
            .chain(GROUP_KINDS.iter().flat_map(|g| [&g.parent_kind, &g.children_kind]))
        {
            out.insert((*kind).to_owned(), Value::Array(self.entities_since(kind, since)));
        }
        for kind in SINGLE_KINDS {
            if let Some(last) = self.entities_since(kind, since).pop() {
                out.insert((*kind).to_owned(), last);
            }
        }
        out
    }

    /// Entidades do tipo que mudaram depois de `since`, na ordem em que mudaram.
    fn entities_since(&self, kind: &str, since: i64) -> Vec<Value> {
        let Some(by_id) = self.entities.get(kind) else { return Vec::new() };
        let mut changed: Vec<_> = by_id.values().filter(|e| e.server_knowledge > since).collect();
        changed.sort_by_key(|e| e.server_knowledge);
        changed.into_iter().map(|e| e.value.clone()).collect()
    }
}

/// Servidor de sincronização de orçamentos, todo em memória.
#[derive(Debug)]
pub struct SyncServer {
    schema_version: i64,
    budgets: HashMap<String, BudgetStore>,
}

impl SyncServer {
    pub fn new(schema_version: i64) -> Self {
        Self { schema_version, budgets: HashMap::new() }
    }

    pub fn schema_version(&self) -> i64 {
        self.schema_version
    }

    /// Cria um orçamento vazio que `owner` pode ler.
    pub fn create_budget(&mut self, budget_version_id: &str, owner: &str) {
        let budget = self.budgets.entry(budget_version_id.to_owned()).or_default();
        budget.readers.insert(owner.to_owned());
    }

    pub fn grant_read(&mut self, budget_version_id: &str, user: &str) {
        if let Some(budget) = self.budgets.get_mut(budget_version_id) {
            budget.readers.insert(user.to_owned());
        }
    }

    pub fn current_server_knowledge(&self, budget_version_id: &str) -> Option<i64> {
        self.budgets.get(budget_version_id).map(|b| b.current_server_knowledge)
    }

    pub fn server_knowledge_of_device(&self, budget_version_id: &str, device_id: &str) -> Option<i64> {
        let budget = self.budgets.get(budget_version_id)?;
        Some(budget.knowledge_of_devices.get(device_id).copied().unwrap_or(0))
    }

    /// Recebe as mudanças do aparelho e devolve o que ele ainda não viu.
    ///
    /// - Orçamento inexistente ou usuário sem acesso: `user_does_not_have_read_permissions`.
    /// - O servidor já recebeu do aparelho mais do que `ending_device_knowledge`:
    ///   `server_knowledge_of_device_exceeds_device_knowledge`, com o valor
    ///   que o servidor tem em `data`. `ending_device_knowledge == 0` (aparelho
    ///   novo ou recarga completa) não é erro: a resposta leva o valor e o
    ///   aparelho o adota.
    /// - Mudanças só são aplicadas se a versão de esquema for a do servidor e
    ///   se `ending_device_knowledge` for além do que o servidor já tem; um
    ///   reenvio do mesmo lote não muda nada.
    /// - Recarga completa devolve todas as entidades; as outras devolvem as que
    ///   mudaram depois de `device_knowledge_of_server`.
    pub fn sync_budget_data(
        &mut self,
        user: &str,
        device_id: &str,
        request: &SyncBudgetDataRequest,
    ) -> Result<SyncBudgetDataResponse, ServerError> {
        let budget = self
            .budgets
            .get_mut(&request.budget_version_id)
            .filter(|b| b.readers.contains(user))
            .ok_or_else(|| ServerError { status_code: 403, id: ERR_NO_READ_PERMISSIONS.into(), data: None })?;

        let known = budget.knowledge_of_devices.get(device_id).copied().unwrap_or(0);
        if request.ending_device_knowledge > 0 && known > request.ending_device_knowledge {
            return Err(ServerError {
                status_code: 409,
                id: ERR_SERVER_KNOWLEDGE_OF_DEVICE_EXCEEDS.into(),
                data: Some(json!({ "server_knowledge_of_device": known })),
            });
        }

        let schema_matches = request.schema_version == self.schema_version;
        if schema_matches && request.ending_device_knowledge > known {
            let incoming = flatten_changed_entities(request)?;
            if !incoming.is_empty() {
                budget.current_server_knowledge += 1;
                let server_knowledge = budget.current_server_knowledge;
                for (kind, id, value) in incoming {
                    budget
                        .entities
                        .entry(kind)
                        .or_default()
                        .insert(id, StoredEntity { value, server_knowledge });
                }
            }
            budget.knowledge_of_devices.insert(device_id.to_owned(), request.ending_device_knowledge);
        }

        let changed_entities = if schema_matches {
            let since = match request.sync_type {
                SyncType::Backfill => 0,
                SyncType::Bootstrap | SyncType::Delta => request.device_knowledge_of_server,
            };
            serde_json::from_value::<IncomingChangedEntities>(Value::Object(budget.changed_since(since)))
                .map_err(|e| invalid_request(&e.to_string()))?
        } else {
            IncomingChangedEntities::default()
        };

        Ok(SyncBudgetDataResponse {
            changed_entities,
            current_server_knowledge: budget.current_server_knowledge,
            error: None,
            schema_version_of_response: self.schema_version,
            schema_version_of_server: self.schema_version,
            server_knowledge_of_device: budget.knowledge_of_devices.get(device_id).copied().unwrap_or(0),
        })
    }

    /// Atende `POST /api/v1/catalog`.
    pub fn handle_catalog(
        &mut self,
        user: &str,
        device_id: &str,
        request: &CatalogRequest,
    ) -> Result<HttpResponse, ServerError> {
        match request.operation_name.as_str() {
            operation::SYNC_BUDGET_DATA => {
                let sync: SyncBudgetDataRequest = serde_json::from_str(&request.request_data)
                    .map_err(|e| invalid_request(&e.to_string()))?;
                let response = self.sync_budget_data(user, device_id, &sync)?;
                Ok(HttpResponse {
                    headers: vec![(SERVER_VERSION_HEADER.to_owned(), env!("CARGO_PKG_VERSION").to_owned())],
                    data: serde_json::to_value(response).map_err(|e| invalid_request(&e.to_string()))?,
                })
            }
            other => Err(ServerError {
                status_code: 404,
                id: ERR_UNKNOWN_OPERATION.into(),
                data: Some(json!({ "operation_name": other })),
            }),
        }
    }
}

fn invalid_request(message: &str) -> ServerError {
    ServerError { status_code: 400, id: ERR_INVALID_REQUEST.into(), data: Some(json!({ "message": message })) }
}

/// Lista `(tipo, id, entidade)` de tudo o que o aparelho enviou, com os
/// grupos de transação desmontados. Falha sem aplicar nada se alguma
/// entidade não tiver `id`.
fn flatten_changed_entities(
    request: &SyncBudgetDataRequest,
) -> Result<Vec<(&'static str, String, Value)>, ServerError> {
    let Value::Object(sent) =
        serde_json::to_value(&request.changed_entities).map_err(|e| invalid_request(&e.to_string()))?
    else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    let mut push = |kind: &'static str, value: &Value| -> Result<(), ServerError> {
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid_request(&format!("entidade de {kind} sem id")))?;
        out.push((kind, id.to_owned(), value.clone()));
        Ok(())
    };

    for (key, value) in &sent {
        if let Some(kind) = SINGLE_KINDS.iter().find(|k| **k == key) {
            push(*kind, value)?;
        } else if let Some(kind) = LIST_KINDS.iter().find(|k| **k == key) {
            for entity in value.as_array().into_iter().flatten() {
                push(*kind, entity)?;
            }
        } else if let Some(group) = GROUP_KINDS.iter().find(|g| g.outgoing == key) {
            for entity in value.as_array().into_iter().flatten() {
                if let Some(parent) = entity.get(group.parent_field).filter(|v| !v.is_null()) {
                    push(group.parent_kind, parent)?;
                }
                for child in entity.get(group.children_field).and_then(Value::as_array).into_iter().flatten() {
                    push(group.children_kind, child)?;
                }
            }
        } else {
            return Err(invalid_request(&format!("tipo de entidade desconhecido: {key}")));
        }
    }
    Ok(out)
}

/// `Transport` que entrega as requisições direto a um [`SyncServer`] no
/// mesmo processo, como um aparelho de um usuário.
pub struct InMemoryTransport {
    pub server: Arc<Mutex<SyncServer>>,
    pub user: String,
    pub device_id: String,
}

impl InMemoryTransport {
    pub fn new(server: Arc<Mutex<SyncServer>>, user: &str, device_id: &str) -> Self {
        Self { server, user: user.to_owned(), device_id: device_id.to_owned() }
    }
}

impl Transport for InMemoryTransport {
    fn post(&mut self, path: &str, body: &CatalogRequest) -> Result<HttpResponse, ApiError> {
        if path != CATALOG_PATH {
            return Err(ApiError::Transport(format!("rota desconhecida: {path}")));
        }
        let mut server = self.server.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        server.handle_catalog(&self.user, &self.device_id, body).map_err(ApiError::Server)
    }
}
