//! Ida e volta: aparelhos com `BudgetSync` falando com um `SyncServer` em
//! memória pelo `InMemoryTransport`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};
use tostao::api::{ApiError, ApiV1, ServerError};
use tostao::knowledge::BudgetKnowledge;
use tostao::protocol::*;
use tostao::server::{InMemoryTransport, SyncServer};
use tostao::sync::*;

const SCHEMA: i64 = 42;
const BUDGET: &str = "orcamento-1";

/// Cópia local do orçamento num aparelho.
#[derive(Default)]
struct LocalStore {
    /// tipo -> id -> (entidade, conhecimento local em que mudou; 0 = veio do servidor)
    entities: BTreeMap<String, BTreeMap<String, (Value, i64)>>,
    permission_denied: usize,
}

impl BudgetHost for LocalStore {
    fn device_id(&self) -> Option<&str> {
        Some("ignorado: o transporte leva o id")
    }
    fn is_logged_in(&self) -> bool {
        true
    }
    fn persist_budget_knowledge_values(&mut self, _: &str, _: &BudgetKnowledge) {}
    fn get_changed_budget_entities_to_sync(&mut self, _: &str, k: &BudgetKnowledge) -> OutgoingChangedEntities {
        let mut out = Map::new();
        for (kind, by_id) in &self.entities {
            let pending: Vec<Value> = by_id
                .values()
                .filter(|(_, local)| *local > k.server_knowledge_of_device)
                .map(|(v, _)| v.clone())
                .collect();
            if !pending.is_empty() {
                out.insert(kind.clone(), Value::Array(pending));
            }
        }
        serde_json::from_value(Value::Object(out)).unwrap()
    }
    fn handle_sync_budget_data_with_server_response(
        &mut self,
        _: &Arc<Mutex<BudgetKnowledge>>,
        _: &SyncBudgetDataRequest,
        response: &SyncBudgetDataResponse,
    ) -> HandledResponse {
        let Value::Object(incoming) = serde_json::to_value(&response.changed_entities).unwrap() else {
            unreachable!()
        };
        let mut changed = false;
        for (kind, value) in incoming {
            let list = match value {
                Value::Array(list) => list,
                Value::Null => continue,
                single => vec![single],
            };
            for entity in list {
                let id = entity["id"].as_str().unwrap().to_owned();
                self.entities.entry(kind.clone()).or_default().insert(id, (entity, 0));
                changed = true;
            }
        }
        HandledResponse { server_had_changed_entities: changed, should_persist_server_knowledge: true }
    }
    fn possibly_import_transactions_after_budget_sync(&mut self, _: &SyncBudgetDataResponse, _: bool) {}
    fn handle_permission_denied_sync_error(&mut self, _: &ServerError) {
        self.permission_denied += 1;
    }
}

struct Device {
    sync: BudgetSync<InMemoryTransport, LocalStore>,
    knowledge: Arc<Mutex<BudgetKnowledge>>,
}

impl Device {
    fn new(server: &Arc<Mutex<SyncServer>>, user: &str, device_id: &str, schema: i64) -> Self {
        let mut api = ApiV1::new(InMemoryTransport::new(server.clone(), user, device_id));
        api.session_token = Some("sessao".into());
        let knowledge = Arc::new(Mutex::new(BudgetKnowledge { schema_version_of_knowledge: schema, ..Default::default() }));
        Self { sync: BudgetSync::new(api, LocalStore::default(), schema), knowledge }
    }

    /// Mudança local: sobe o conhecimento do aparelho e marca a entidade.
    fn change(&mut self, kind: &str, entity: Value) {
        let local = {
            let mut k = self.knowledge.lock().unwrap();
            k.current_device_knowledge += 1;
            k.current_device_knowledge
        };
        let id = entity["id"].as_str().unwrap().to_owned();
        self.sync.host.entities.entry(kind.to_owned()).or_default().insert(id, (entity, local));
    }

    fn assign(&mut self, id: &str, category: &str, month: &str, budgeted: i64) {
        self.change(
            "be_monthly_subcategory_budgets",
            json!({"id": id, "entities_subcategory_id": category, "month": month, "budgeted": budgeted, "is_tombstone": false}),
        );
    }

    fn sync(&mut self, sync_type: SyncType) -> Result<SyncResult, SyncError> {
        self.sync.internal_sync_budget_data_with_server(Some(BUDGET), Some(&self.knowledge), sync_type)
    }

    fn get(&self, kind: &str, id: &str) -> Option<&Value> {
        self.sync.host.entities.get(kind)?.get(id).map(|(v, _)| v)
    }

    fn knowledge(&self) -> BudgetKnowledge {
        *self.knowledge.lock().unwrap()
    }
}

fn server(schema: i64) -> Arc<Mutex<SyncServer>> {
    let mut s = SyncServer::open_in_memory(schema).unwrap();
    s.create_budget(BUDGET, "familia").unwrap();
    Arc::new(Mutex::new(s))
}

#[test]
fn assignment_from_device_a_reaches_device_b() {
    let srv = server(SCHEMA);
    let mut a = Device::new(&srv, "familia", "aparelho-a", SCHEMA);
    let mut b = Device::new(&srv, "familia", "aparelho-b", SCHEMA);

    a.assign("msb-mercado-2026-10", "cat-mercado", "2026-10-01", 500_000);
    a.sync(SyncType::Delta).unwrap();

    let ka = a.knowledge();
    assert_eq!(ka.server_knowledge_of_device, 1);
    assert_eq!(ka.device_knowledge_of_server, 1);
    assert!(!ka.has_changes_to_send());
    assert_eq!(srv.lock().unwrap().current_server_knowledge(BUDGET).unwrap(), Some(1));
    assert_eq!(srv.lock().unwrap().server_knowledge_of_device(BUDGET, "aparelho-a").unwrap(), 1);

    let r = b.sync(SyncType::Delta).unwrap();
    assert!(r.server_had_changed_entities);
    let got = b.get("be_monthly_subcategory_budgets", "msb-mercado-2026-10").unwrap();
    assert_eq!(got["budgeted"], 500_000);
    assert_eq!(b.knowledge().device_knowledge_of_server, 1);
    assert_eq!(b.knowledge().server_knowledge_of_device, 0);
}

#[test]
fn later_change_replaces_earlier_one() {
    let srv = server(SCHEMA);
    let mut a = Device::new(&srv, "familia", "aparelho-a", SCHEMA);
    let mut b = Device::new(&srv, "familia", "aparelho-b", SCHEMA);

    a.assign("msb-1", "cat-mercado", "2026-10-01", 500_000);
    a.sync(SyncType::Delta).unwrap();
    b.sync(SyncType::Delta).unwrap();

    b.assign("msb-1", "cat-mercado", "2026-10-01", 650_000);
    b.sync(SyncType::Delta).unwrap();
    a.sync(SyncType::Delta).unwrap();

    assert_eq!(a.get("be_monthly_subcategory_budgets", "msb-1").unwrap()["budgeted"], 650_000);
    assert_eq!(srv.lock().unwrap().current_server_knowledge(BUDGET).unwrap(), Some(2));
}

#[test]
fn delta_without_changes_returns_empty_lists() {
    let srv = server(SCHEMA);
    let mut a = Device::new(&srv, "familia", "aparelho-a", SCHEMA);
    a.assign("msb-1", "cat-mercado", "2026-10-01", 500_000);
    a.sync(SyncType::Delta).unwrap();

    let r = a.sync(SyncType::Delta).unwrap();
    assert!(!r.server_had_changed_entities);
    let resp = r.server_response.unwrap();
    assert_eq!(resp.current_server_knowledge, 1);
    let e = &resp.changed_entities;
    assert_eq!(e.be_monthly_subcategory_budgets, Some(vec![]));
    assert_eq!(e.be_transactions, Some(vec![]));
    assert_eq!(e.be_accounts, Some(vec![]));
    assert_eq!(e.be_budget, None);
    assert_eq!(srv.lock().unwrap().current_server_knowledge(BUDGET).unwrap(), Some(1));
}

#[test]
fn resending_the_same_batch_is_not_applied_twice() {
    let mut srv = SyncServer::open_in_memory(SCHEMA).unwrap();
    srv.create_budget(BUDGET, "familia").unwrap();
    let request = SyncBudgetDataRequest {
        budget_version_id: BUDGET.into(),
        sync_type: SyncType::Delta,
        starting_device_knowledge: 0,
        ending_device_knowledge: 1,
        device_knowledge_of_server: 0,
        calculated_entities_included: false,
        schema_version: SCHEMA,
        schema_version_of_knowledge: SCHEMA,
        changed_entities: OutgoingChangedEntities {
            be_accounts: Some(vec![json!({"id": "conta-1", "account_name": "Corrente"})]),
            ..Default::default()
        },
    };
    srv.sync_budget_data("familia", "aparelho-a", &request).unwrap();
    let again = srv.sync_budget_data("familia", "aparelho-a", &request).unwrap();
    assert_eq!(again.current_server_knowledge, 1);
    assert_eq!(again.server_knowledge_of_device, 1);
}

#[test]
fn schema_version_mismatch_is_rejected_by_client_and_not_applied() {
    let srv = server(SCHEMA + 1);
    let mut a = Device::new(&srv, "familia", "aparelho-a", SCHEMA);
    a.assign("msb-1", "cat-mercado", "2026-10-01", 500_000);

    let err = a.sync(SyncType::Delta).unwrap_err();
    assert!(matches!(err, SyncError::UnexpectedSchemaVersion { expected: SCHEMA, received: 43 }));
    assert_eq!(srv.lock().unwrap().current_server_knowledge(BUDGET).unwrap(), Some(0));
    assert_eq!(srv.lock().unwrap().server_knowledge_of_device(BUDGET, "aparelho-a").unwrap(), 0);
    assert!(a.knowledge().has_changes_to_send(), "a mudança continua pendente no aparelho");
}

#[test]
fn backfill_returns_everything_regardless_of_device_knowledge() {
    let srv = server(SCHEMA);
    let mut a = Device::new(&srv, "familia", "aparelho-a", SCHEMA);
    let mut b = Device::new(&srv, "familia", "aparelho-b", SCHEMA);

    a.assign("msb-1", "cat-mercado", "2026-10-01", 500_000);
    a.sync(SyncType::Delta).unwrap();
    a.assign("msb-2", "cat-aluguel", "2026-10-01", 2_000_000);
    a.sync(SyncType::Delta).unwrap();

    b.sync(SyncType::Delta).unwrap();
    assert_eq!(b.knowledge().device_knowledge_of_server, 2);

    // B perde a cópia local; um delta não traria nada de volta.
    b.sync.host.entities.clear();
    let r = b.sync(SyncType::Delta).unwrap();
    assert!(!r.server_had_changed_entities);

    let r = b.sync(SyncType::Backfill).unwrap();
    assert!(r.server_had_changed_entities);
    assert!(b.get("be_monthly_subcategory_budgets", "msb-1").is_some());
    assert!(b.get("be_monthly_subcategory_budgets", "msb-2").is_some());
    assert_eq!(b.knowledge().device_knowledge_of_server, 2);
}

#[test]
fn backfill_does_not_send_pending_changes() {
    let srv = server(SCHEMA);
    let mut a = Device::new(&srv, "familia", "aparelho-a", SCHEMA);
    a.assign("msb-1", "cat-mercado", "2026-10-01", 500_000);
    a.sync(SyncType::Backfill).unwrap();
    assert_eq!(srv.lock().unwrap().current_server_knowledge(BUDGET).unwrap(), Some(0));
    assert!(a.knowledge().has_changes_to_send());
}

#[test]
fn bootstrap_brings_budget_but_does_not_record_server_knowledge() {
    let srv = server(SCHEMA);
    let mut a = Device::new(&srv, "familia", "aparelho-a", SCHEMA);
    a.assign("msb-1", "cat-mercado", "2026-10-01", 500_000);
    a.sync(SyncType::Delta).unwrap();

    let mut b = Device::new(&srv, "familia", "aparelho-b", SCHEMA);
    b.sync(SyncType::Bootstrap).unwrap();
    assert!(b.get("be_monthly_subcategory_budgets", "msb-1").is_some());
    assert_eq!(b.knowledge().device_knowledge_of_server, 0);
}

#[test]
fn transaction_groups_come_back_as_transactions_and_subtransactions() {
    let srv = server(SCHEMA);
    let mut a = Device::new(&srv, "familia", "aparelho-a", SCHEMA);
    let mut b = Device::new(&srv, "familia", "aparelho-b", SCHEMA);

    a.change(
        "be_transaction_groups",
        json!({
            "id": "tx-1",
            "be_transaction": {"id": "tx-1", "amount": -150_000, "entities_account_id": "conta-1"},
            "be_subtransactions": [
                {"id": "sub-1", "transaction_id": "tx-1", "amount": -100_000},
                {"id": "sub-2", "transaction_id": "tx-1", "amount": -50_000}
            ]
        }),
    );
    a.sync(SyncType::Delta).unwrap();
    assert_eq!(a.knowledge().queue_calculations_for_server_entities, 1, "o grupo voltou como transação");

    b.sync(SyncType::Delta).unwrap();
    assert_eq!(b.get("be_transactions", "tx-1").unwrap()["amount"], -150_000);
    assert_eq!(b.get("be_subtransactions", "sub-2").unwrap()["amount"], -50_000);
}

#[test]
fn server_knowing_more_about_device_returns_error_with_value() {
    let srv = server(SCHEMA);
    let mut a = Device::new(&srv, "familia", "aparelho-a", SCHEMA);
    for i in 0..3 {
        a.assign(&format!("msb-{i}"), "cat-mercado", "2026-10-01", 1_000);
    }
    a.sync(SyncType::Delta).unwrap();
    assert_eq!(a.knowledge().server_knowledge_of_device, 3);

    // O aparelho volta com um conhecimento local menor (ex.: restaurado de backup).
    {
        let mut k = a.knowledge.lock().unwrap();
        k.current_device_knowledge = 1;
        k.server_knowledge_of_device = 0;
    }
    a.assign("msb-novo", "cat-mercado", "2026-10-01", 1_000);
    let err = a.sync(SyncType::Delta).unwrap_err();
    let SyncError::Api(ApiError::Server(e)) = err else { panic!("esperava erro do servidor: {err:?}") };
    assert_eq!(e.id, ERR_SERVER_KNOWLEDGE_OF_DEVICE_EXCEEDS);
    assert_eq!(e.data, Some(json!({"server_knowledge_of_device": 3})));
}

#[test]
fn user_without_access_gets_permission_error() {
    let srv = server(SCHEMA);
    let mut visitor = Device::new(&srv, "visita", "aparelho-v", SCHEMA);
    let r = visitor.sync(SyncType::Delta).unwrap();
    assert!(r.server_had_changed_entities);
    assert_eq!(visitor.sync.host.permission_denied, 1);

    srv.lock().unwrap().grant_read(BUDGET, "visita").unwrap();
    visitor.sync(SyncType::Delta).unwrap();
    assert_eq!(visitor.sync.host.permission_denied, 1);
}

#[test]
fn unknown_operation_is_an_error() {
    let mut srv = SyncServer::open_in_memory(SCHEMA).unwrap();
    let err = srv
        .handle_catalog("familia", "aparelho-a", &CatalogRequest { operation_name: "loginUser".into(), request_data: "{}".into() })
        .unwrap_err();
    assert_eq!(err.id, tostao::server::ERR_UNKNOWN_OPERATION);
}

#[test]
fn budget_survives_reopening_the_database_file() {
    let path = std::env::temp_dir().join(format!("tostao-teste-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    {
        let mut s = SyncServer::open(&path, SCHEMA).unwrap();
        s.create_budget(BUDGET, "familia").unwrap();
        let srv = Arc::new(Mutex::new(s));
        let mut a = Device::new(&srv, "familia", "aparelho-a", SCHEMA);
        a.assign("msb-1", "cat-mercado", "2026-10-01", 500_000);
        a.sync(SyncType::Delta).unwrap();
    }

    let srv = Arc::new(Mutex::new(SyncServer::open(&path, SCHEMA).unwrap()));
    assert_eq!(srv.lock().unwrap().current_server_knowledge(BUDGET).unwrap(), Some(1));
    assert_eq!(srv.lock().unwrap().server_knowledge_of_device(BUDGET, "aparelho-a").unwrap(), 1);
    let mut b = Device::new(&srv, "familia", "aparelho-b", SCHEMA);
    b.sync(SyncType::Delta).unwrap();
    assert_eq!(b.get("be_monthly_subcategory_budgets", "msb-1").unwrap()["budgeted"], 500_000);

    drop(b);
    drop(srv);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn batch_with_entity_without_id_is_rejected_whole() {
    let srv = server(SCHEMA);
    let mut a = Device::new(&srv, "familia", "aparelho-a", SCHEMA);
    a.assign("msb-1", "cat-mercado", "2026-10-01", 500_000);
    a.change("be_accounts", json!({"id": "conta-1"}));
    a.sync.host.entities.get_mut("be_accounts").unwrap().get_mut("conta-1").unwrap().0 = json!({"account_name": "sem id"});

    let err = a.sync(SyncType::Delta).unwrap_err();
    let SyncError::Api(ApiError::Server(e)) = err else { panic!("esperava erro do servidor: {err:?}") };
    assert_eq!(e.id, tostao::server::ERR_INVALID_REQUEST);
    assert_eq!(srv.lock().unwrap().current_server_knowledge(BUDGET).unwrap(), Some(0));
    assert_eq!(srv.lock().unwrap().server_knowledge_of_device(BUDGET, "aparelho-a").unwrap(), 0);
}
