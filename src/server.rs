//! Lado servidor de `syncBudgetData`, guardado num banco SQLite.
//!
//! Cada orçamento tem um contador de conhecimento do servidor que sobe uma
//! vez por lote de mudanças aceito; cada entidade guarda o valor do contador
//! em que mudou pela última vez. Um aparelho recebe as entidades com
//! conhecimento maior do que o `device_knowledge_of_server` que ele enviou.
//! Cada sincronização roda numa transação do banco: ou o lote todo entra,
//! ou nada entra.
//!
//! Entidades são guardadas como texto JSON com um campo `id` texto; a última
//! versão recebida substitui a anterior inteira.

use std::fmt::Display;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::api::{ApiError, HttpResponse, ServerError, Transport};
use crate::protocol::{
    operation, CatalogRequest, IncomingChangedEntities, SyncBudgetDataRequest,
    SyncBudgetDataResponse, SyncType, CATALOG_PATH, SERVER_VERSION_HEADER,
};
use crate::sync::{ERR_NO_READ_PERMISSIONS, ERR_SERVER_KNOWLEDGE_OF_DEVICE_EXCEEDS};

pub const ERR_INVALID_REQUEST: &str = "invalid_request";
pub const ERR_UNKNOWN_OPERATION: &str = "unknown_operation";
pub const ERR_INTERNAL: &str = "internal_error";

const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS budgets (
    id                       TEXT PRIMARY KEY,
    current_server_knowledge INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS budget_readers (
    budget_id TEXT NOT NULL REFERENCES budgets(id),
    user      TEXT NOT NULL,
    PRIMARY KEY (budget_id, user)
);
CREATE TABLE IF NOT EXISTS device_knowledge (
    budget_id                  TEXT NOT NULL REFERENCES budgets(id),
    device_id                  TEXT NOT NULL,
    server_knowledge_of_device INTEGER NOT NULL,
    PRIMARY KEY (budget_id, device_id)
);
CREATE TABLE IF NOT EXISTS entities (
    budget_id        TEXT NOT NULL REFERENCES budgets(id),
    kind             TEXT NOT NULL,
    id               TEXT NOT NULL,
    value            TEXT NOT NULL,
    server_knowledge INTEGER NOT NULL,
    PRIMARY KEY (budget_id, kind, id)
);
CREATE INDEX IF NOT EXISTS entities_by_knowledge ON entities (budget_id, server_knowledge);
";

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

/// Servidor de sincronização de orçamentos.
#[derive(Debug)]
pub struct SyncServer {
    schema_version: i64,
    conn: Connection,
}

impl SyncServer {
    /// Abre (ou cria) o banco no arquivo `path`.
    pub fn open(path: impl AsRef<Path>, schema_version: i64) -> rusqlite::Result<Self> {
        Self::with_connection(Connection::open(path)?, schema_version)
    }

    /// Banco só em memória, que some quando o servidor é descartado.
    pub fn open_in_memory(schema_version: i64) -> rusqlite::Result<Self> {
        Self::with_connection(Connection::open_in_memory()?, schema_version)
    }

    fn with_connection(conn: Connection, schema_version: i64) -> rusqlite::Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.execute_batch(SCHEMA_SQL)?;
        Ok(Self { schema_version, conn })
    }

    pub fn schema_version(&self) -> i64 {
        self.schema_version
    }

    /// Cria um orçamento vazio (se ainda não existir) que `owner` pode ler.
    pub fn create_budget(&mut self, budget_version_id: &str, owner: &str) -> rusqlite::Result<()> {
        self.conn.execute("INSERT OR IGNORE INTO budgets (id) VALUES (?1)", [budget_version_id])?;
        self.grant_read(budget_version_id, owner)
    }

    pub fn grant_read(&mut self, budget_version_id: &str, user: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO budget_readers (budget_id, user) VALUES (?1, ?2)",
            [budget_version_id, user],
        )?;
        Ok(())
    }

    pub fn current_server_knowledge(&self, budget_version_id: &str) -> rusqlite::Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT current_server_knowledge FROM budgets WHERE id = ?1",
                [budget_version_id],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn server_knowledge_of_device(&self, budget_version_id: &str, device_id: &str) -> rusqlite::Result<i64> {
        knowledge_of_device(&self.conn, budget_version_id, device_id)
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
        let budget_id = request.budget_version_id.as_str();
        let schema_matches = request.schema_version == self.schema_version;
        let tx = self.conn.transaction().map_err(internal)?;

        let mut current: i64 = tx
            .query_row(
                "SELECT b.current_server_knowledge FROM budgets b
                 JOIN budget_readers r ON r.budget_id = b.id
                 WHERE b.id = ?1 AND r.user = ?2",
                [budget_id, user],
                |row| row.get(0),
            )
            .optional()
            .map_err(internal)?
            .ok_or_else(|| ServerError { status_code: 403, id: ERR_NO_READ_PERMISSIONS.into(), data: None })?;

        let mut known = knowledge_of_device(&tx, budget_id, device_id).map_err(internal)?;
        if request.ending_device_knowledge > 0 && known > request.ending_device_knowledge {
            return Err(ServerError {
                status_code: 409,
                id: ERR_SERVER_KNOWLEDGE_OF_DEVICE_EXCEEDS.into(),
                data: Some(json!({ "server_knowledge_of_device": known })),
            });
        }

        if schema_matches && request.ending_device_knowledge > known {
            let incoming = flatten_changed_entities(request)?;
            if !incoming.is_empty() {
                current += 1;
                tx.execute("UPDATE budgets SET current_server_knowledge = ?2 WHERE id = ?1", params![budget_id, current])
                    .map_err(internal)?;
                let mut upsert = tx
                    .prepare(
                        "INSERT INTO entities (budget_id, kind, id, value, server_knowledge)
                         VALUES (?1, ?2, ?3, ?4, ?5)
                         ON CONFLICT (budget_id, kind, id)
                         DO UPDATE SET value = excluded.value, server_knowledge = excluded.server_knowledge",
                    )
                    .map_err(internal)?;
                for (kind, id, value) in incoming {
                    upsert.execute(params![budget_id, kind, id, value.to_string(), current]).map_err(internal)?;
                }
            }
            known = request.ending_device_knowledge;
            tx.execute(
                "INSERT INTO device_knowledge (budget_id, device_id, server_knowledge_of_device)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT (budget_id, device_id)
                 DO UPDATE SET server_knowledge_of_device = excluded.server_knowledge_of_device",
                params![budget_id, device_id, known],
            )
            .map_err(internal)?;
        }

        let changed_entities = if schema_matches {
            let since = match request.sync_type {
                SyncType::Backfill => 0,
                SyncType::Bootstrap | SyncType::Delta => request.device_knowledge_of_server,
            };
            changed_since(&tx, budget_id, since)?
        } else {
            IncomingChangedEntities::default()
        };
        tx.commit().map_err(internal)?;

        Ok(SyncBudgetDataResponse {
            changed_entities,
            current_server_knowledge: current,
            error: None,
            schema_version_of_response: self.schema_version,
            schema_version_of_server: self.schema_version,
            server_knowledge_of_device: known,
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
                    data: serde_json::to_value(response).map_err(internal)?,
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

fn knowledge_of_device(conn: &Connection, budget_id: &str, device_id: &str) -> rusqlite::Result<i64> {
    Ok(conn
        .query_row(
            "SELECT server_knowledge_of_device FROM device_knowledge WHERE budget_id = ?1 AND device_id = ?2",
            [budget_id, device_id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

/// Entidades que mudaram depois de `since`, na ordem em que mudaram. Toda
/// lista vem presente, vazia se nada mudou.
fn changed_since(conn: &Connection, budget_id: &str, since: i64) -> Result<IncomingChangedEntities, ServerError> {
    let mut out = Map::new();
    for kind in LIST_KINDS
        .iter()
        .chain(GROUP_KINDS.iter().flat_map(|g| [&g.parent_kind, &g.children_kind]))
    {
        out.insert((*kind).to_owned(), Value::Array(Vec::new()));
    }

    let mut stmt = conn
        .prepare(
            "SELECT kind, value FROM entities
             WHERE budget_id = ?1 AND server_knowledge > ?2
             ORDER BY server_knowledge, kind, id",
        )
        .map_err(internal)?;
    let rows = stmt
        .query_map(params![budget_id, since], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(internal)?;
    for row in rows {
        let (kind, text) = row.map_err(internal)?;
        let value: Value = serde_json::from_str(&text).map_err(internal)?;
        match out.get_mut(&kind) {
            Some(Value::Array(list)) => list.push(value),
            _ => {
                out.insert(kind, value);
            }
        }
    }
    serde_json::from_value(Value::Object(out)).map_err(internal)
}

fn internal(error: impl Display) -> ServerError {
    ServerError { status_code: 500, id: ERR_INTERNAL.into(), data: Some(json!({ "message": error.to_string() })) }
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
