use std::future::Future;
use std::sync::{Arc, Mutex};
use std::task::{Context, Waker};

use futures_executor::block_on;
use serde_json::{json, Value};
use tostao::api::{ApiError, ApiV1, HttpResponse, ServerError, Transport};
use tostao::knowledge::BudgetKnowledge;
use tostao::protocol::*;
use tostao::sync::*;

const SCHEMA: i64 = 42;

struct FakeTransport {
    reply: Result<Value, ServerError>,
    sent: Vec<CatalogRequest>,
    /// Simula uma edição local enquanto a requisição está em voo.
    bump_during_request: Option<Arc<Mutex<BudgetKnowledge>>>,
}

impl Transport for FakeTransport {
    async fn post(&mut self, path: &str, body: &CatalogRequest) -> Result<HttpResponse, ApiError> {
        assert_eq!(path, "/api/v1/catalog");
        self.sent.push(body.clone());
        if let Some(k) = &self.bump_during_request {
            k.lock().unwrap().current_device_knowledge += 1;
        }
        match &self.reply {
            Ok(data) => Ok(HttpResponse {
                headers: vec![("X-Tostao-Server-Version".into(), "9.9.9".into())],
                data: data.clone(),
            }),
            Err(e) => Err(ApiError::Server(e.clone())),
        }
    }
}

#[derive(Default)]
struct FakeHost {
    persisted: Vec<BudgetKnowledge>,
    handled: usize,
    imported: usize,
    permission_denied: usize,
    completed: usize,
}

impl BudgetHost for FakeHost {
    fn device_id(&self) -> Option<&str> {
        Some("device-1")
    }
    fn is_logged_in(&self) -> bool {
        true
    }
    fn persist_budget_knowledge_values(&mut self, _: &str, k: &BudgetKnowledge) {
        self.persisted.push(*k);
    }
    fn get_changed_budget_entities_to_sync(&mut self, _: &str, _: &BudgetKnowledge) -> OutgoingChangedEntities {
        OutgoingChangedEntities {
            be_monthly_subcategory_budgets: Some(vec![json!({"id": "msb-1", "budgeted": 1000})]),
            ..Default::default()
        }
    }
    fn handle_sync_budget_data_with_server_response(
        &mut self,
        _: &Arc<Mutex<BudgetKnowledge>>,
        _: &SyncBudgetDataRequest,
        _: &SyncBudgetDataResponse,
    ) -> HandledResponse {
        self.handled += 1;
        HandledResponse { server_had_changed_entities: true, should_persist_server_knowledge: true }
    }
    fn possibly_import_transactions_after_budget_sync(&mut self, _: &SyncBudgetDataResponse, _: bool) {
        self.imported += 1;
    }
    fn handle_permission_denied_sync_error(&mut self, _: &ServerError) {
        self.permission_denied += 1;
    }
    fn budget_sync_completed(&mut self) {
        self.completed += 1;
    }
}

fn response(schema: i64, server_knowledge: i64, server_knowledge_of_device: i64) -> Value {
    json!({
        "changed_entities": {"be_monthly_subcategory_budgets": [{"id": "msb-1"}]},
        "current_server_knowledge": server_knowledge,
        "error": null,
        "schema_version_of_response": schema,
        "schema_version_of_server": schema,
        "server_knowledge_of_device": server_knowledge_of_device
    })
}

fn knowledge(current: i64, sk_of_device: i64, dk_of_server: i64, schema: i64) -> Arc<Mutex<BudgetKnowledge>> {
    Arc::new(Mutex::new(BudgetKnowledge {
        current_device_knowledge: current,
        server_knowledge_of_device: sk_of_device,
        device_knowledge_of_server: dk_of_server,
        schema_version_of_knowledge: schema,
        queue_calculations_for_server_entities: 0,
    }))
}

fn engine(reply: Result<Value, ServerError>) -> BudgetSync<FakeTransport, FakeHost> {
    let mut api = ApiV1::new(FakeTransport { reply, sent: vec![], bump_during_request: None });
    api.session_token = Some("token".into());
    BudgetSync::new(api, FakeHost::default(), SCHEMA)
}

/// A sincronização é `async`; estes testes a esperam aqui, com `block_on`.
trait SincronizarEsperando {
    fn sincronizar_esperando(
        &mut self,
        budget_version_id: Option<&str>,
        knowledge: Option<&Arc<Mutex<BudgetKnowledge>>>,
        sync_type: SyncType,
    ) -> Result<SyncResult, SyncError>;
}

impl<T: Transport, H: BudgetHost> SincronizarEsperando for BudgetSync<T, H> {
    fn sincronizar_esperando(
        &mut self,
        budget_version_id: Option<&str>,
        knowledge: Option<&Arc<Mutex<BudgetKnowledge>>>,
        sync_type: SyncType,
    ) -> Result<SyncResult, SyncError> {
        block_on(self.internal_sync_budget_data_with_server(budget_version_id, knowledge, sync_type))
    }
}

#[test]
fn request_keeps_bundle_field_order() {
    let mut e = engine(Ok(response(SCHEMA, 120, 7)));
    let k = knowledge(7, 5, 100, SCHEMA);
    e.sincronizar_esperando(Some("bv-1"), Some(&k), SyncType::Delta).unwrap();

    let sent = &e.api.transport.sent[0];
    assert_eq!(sent.operation_name, "syncBudgetData");
    let body: Value = serde_json::from_str(&sent.request_data).unwrap();
    let keys: Vec<_> = body.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        [
            "budget_version_id", "sync_type", "starting_device_knowledge", "ending_device_knowledge",
            "device_knowledge_of_server", "calculated_entities_included", "schema_version",
            "schema_version_of_knowledge", "changed_entities",
        ]
    );
    assert_eq!(body["sync_type"], "delta");
    assert_eq!(body["starting_device_knowledge"], 5);
    assert_eq!(body["ending_device_knowledge"], 7);
    assert_eq!(body["calculated_entities_included"], false);
}

#[test]
fn successful_delta_persists_server_knowledge_and_flags_collision() {
    let mut e = engine(Ok(response(SCHEMA, 120, 7)));
    let k = knowledge(7, 5, 100, SCHEMA);
    let r = e.sincronizar_esperando(Some("bv-1"), Some(&k), SyncType::Delta).unwrap();

    assert_eq!(r.server_app_version.as_deref(), Some("9.9.9"));
    let k = *k.lock().unwrap();
    assert_eq!(k.server_knowledge_of_device, 7);
    assert_eq!(k.device_knowledge_of_server, 120);
    assert_eq!(k.queue_calculations_for_server_entities, 1);
    assert_eq!((e.host.handled, e.host.imported, e.host.completed), (1, 1, 1));
}

#[test]
fn backfill_zeroes_knowledge_window() {
    let mut e = engine(Ok(response(SCHEMA, 120, 7)));
    let k = knowledge(7, 5, 100, SCHEMA);
    e.sincronizar_esperando(Some("bv-1"), Some(&k), SyncType::Backfill).unwrap();
    let body: Value = serde_json::from_str(&e.api.transport.sent[0].request_data).unwrap();
    assert_eq!(body["starting_device_knowledge"], 0);
    assert_eq!(body["ending_device_knowledge"], 0);
    assert_eq!(body["changed_entities"], json!({}));
}

#[test]
fn rejects_unexpected_schema_version() {
    let mut e = engine(Ok(response(SCHEMA + 1, 120, 7)));
    let k = knowledge(7, 5, 100, SCHEMA);
    let err = e.sincronizar_esperando(Some("bv-1"), Some(&k), SyncType::Delta).unwrap_err();
    assert!(matches!(err, SyncError::UnexpectedSchemaVersion { expected: SCHEMA, received: 43 }));
    assert_eq!(e.host.completed, 0);
}

#[test]
fn rejects_response_older_than_knowledge_schema() {
    let mut e = engine(Ok(response(SCHEMA, 120, 7)));
    let k = knowledge(7, 5, 100, SCHEMA + 5);
    let err = e.sincronizar_esperando(Some("bv-1"), Some(&k), SyncType::Delta).unwrap_err();
    assert!(matches!(err, SyncError::SchemaVersionBelowKnowledge { response: SCHEMA, knowledge: 47 }));
}

#[test]
fn server_knowing_more_than_device_is_fatal_unless_device_is_fresh() {
    let mut e = engine(Ok(response(SCHEMA, 120, 50)));
    let k = knowledge(7, 5, 100, SCHEMA);
    let err = e.sincronizar_esperando(Some("bv-1"), Some(&k), SyncType::Delta).unwrap_err();
    assert!(matches!(err, SyncError::ServerKnowsMoreAboutDevice));

    let mut e = engine(Ok(response(SCHEMA, 120, 50)));
    let k = knowledge(0, 0, 0, SCHEMA);
    e.sincronizar_esperando(Some("bv-1"), Some(&k), SyncType::Delta).unwrap();
    assert_eq!(k.lock().unwrap().current_device_knowledge, 50);
}

#[test]
fn delta_ignores_incoming_when_local_changes_happened_in_flight() {
    let mut e = engine(Ok(response(SCHEMA, 120, 7)));
    let k = knowledge(7, 5, 100, SCHEMA);
    e.api.transport.bump_during_request = Some(k.clone());
    e.sincronizar_esperando(Some("bv-1"), Some(&k), SyncType::Delta).unwrap();
    assert_eq!(e.host.handled, 0);
    assert_eq!(k.lock().unwrap().device_knowledge_of_server, 100);
}

#[test]
fn swallows_knowledge_exceeds_error_when_device_knows_enough() {
    let err = ServerError {
        status_code: 409,
        id: ERR_SERVER_KNOWLEDGE_OF_DEVICE_EXCEEDS.into(),
        data: Some(json!({"server_knowledge_of_device": 7})),
    };
    let mut e = engine(Err(err));
    let k = knowledge(7, 5, 100, SCHEMA);
    let r = e.sincronizar_esperando(Some("bv-1"), Some(&k), SyncType::Delta).unwrap();
    assert!(r.server_had_changed_entities);
    assert_eq!(e.host.completed, 0);
}

#[test]
fn permission_denied_is_handled_and_401_clears_token() {
    let err = ServerError { status_code: 403, id: ERR_NO_READ_PERMISSIONS.into(), data: None };
    let mut e = engine(Err(err));
    let k = knowledge(7, 5, 100, SCHEMA);
    e.sincronizar_esperando(Some("bv-1"), Some(&k), SyncType::Delta).unwrap();
    assert_eq!(e.host.permission_denied, 1);

    let err = ServerError { status_code: 401, id: "unauthorized".into(), data: None };
    let mut e = engine(Err(err));
    assert!(e.sincronizar_esperando(Some("bv-1"), Some(&k), SyncType::Delta).is_err());
    assert!(e.api.session_token.is_none());
}

#[test]
fn read_only_blocks_writes_but_allows_pulls() {
    let mut e = engine(Ok(response(SCHEMA, 120, 7)));
    e.api.is_read_only = true;
    let k = knowledge(7, 5, 100, SCHEMA);
    let err = e.sincronizar_esperando(Some("bv-1"), Some(&k), SyncType::Delta).unwrap_err();
    assert!(matches!(err, SyncError::Api(ApiError::WriteRequestWhileReadOnly(_))));

    // sem mudanças a enviar (ending == starting): puxar é permitido
    e.api.transport.reply = Ok(response(SCHEMA, 120, 5));
    let k = knowledge(5, 5, 100, SCHEMA);
    e.sincronizar_esperando(Some("bv-1"), Some(&k), SyncType::Delta).unwrap();
    assert_eq!(e.host.imported, 0, "não importa transações em só-leitura");
}

#[test]
fn missing_budget_version_returns_early() {
    let mut e = engine(Ok(response(SCHEMA, 120, 7)));
    let r = e.sincronizar_esperando(None, None, SyncType::Delta).unwrap();
    assert!(r.server_had_changed_entities);
    assert!(e.api.transport.sent.is_empty());
}

/// Transporte que trava na primeira chamada (a future fica pendente para
/// sempre) e responde nas seguintes.
struct TravadoNaPrimeira {
    reply: Value,
    chamadas: usize,
}

impl Transport for TravadoNaPrimeira {
    async fn post(&mut self, _: &str, _: &CatalogRequest) -> Result<HttpResponse, ApiError> {
        self.chamadas += 1;
        if self.chamadas == 1 {
            std::future::pending::<()>().await;
        }
        Ok(HttpResponse { headers: vec![], data: self.reply.clone() })
    }
}

/// Uma future de sincronização descartada no meio do `await` não pode deixar
/// a sincronização seguinte presa como "outra em andamento".
#[test]
fn future_descartada_no_meio_nao_trava_a_proxima_sincronizacao() {
    let mut api = ApiV1::new(TravadoNaPrimeira { reply: response(SCHEMA, 120, 7), chamadas: 0 });
    api.session_token = Some("token".into());
    let mut e = BudgetSync::new(api, FakeHost::default(), SCHEMA);
    let k = knowledge(7, 5, 100, SCHEMA);
    {
        let mut futura = Box::pin(e.internal_sync_budget_data_with_server(Some("bv-1"), Some(&k), SyncType::Delta));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(futura.as_mut().poll(&mut cx).is_pending());
    }

    let r = e.sincronizar_esperando(Some("bv-1"), Some(&k), SyncType::Delta).unwrap();
    assert!(!r.sync_cancelled_because_another_in_flight, "a flag ficou presa depois de descartar a future");
    assert_eq!(e.host.completed, 1);
}
