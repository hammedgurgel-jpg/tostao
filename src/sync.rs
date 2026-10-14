//! `BaseStore::internalSyncBudgetDataWithServer`, passo a passo, na mesma
//! ordem de leituras e checks do bundle.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::api::{ApiError, ApiV1, ServerError, Transport};
use crate::knowledge::BudgetKnowledge;
use crate::protocol::{
    OutgoingChangedEntities, SyncBudgetDataRequest, SyncBudgetDataResponse, SyncType,
    SERVER_VERSION_HEADER,
};

pub const ERR_SERVER_KNOWLEDGE_OF_DEVICE_EXCEEDS: &str =
    "server_knowledge_of_device_exceeds_device_knowledge";
pub const ERR_NO_READ_PERMISSIONS: &str = "user_does_not_have_read_permissions";

/// Resultado de uma sincronização (`i` no bundle).
#[derive(Debug, Clone, Default)]
pub struct SyncResult {
    pub server_had_changed_entities: bool,
    pub sync_cancelled_because_another_in_flight: bool,
    pub server_app_version: Option<String>,
    pub server_response: Option<SyncBudgetDataResponse>,
}

/// Retorno de `handleSyncBudgetDataWithServerResponse`.
#[derive(Debug, Clone, Copy, Default)]
pub struct HandledResponse {
    pub server_had_changed_entities: bool,
    pub should_persist_server_knowledge: bool,
}

#[derive(Debug)]
pub enum SyncError {
    DeviceIdNotSet,
    NotLoggedIn,
    KnowledgeNotSpecified,
    UnexpectedSchemaVersion { expected: i64, received: i64 },
    SchemaVersionBelowKnowledge { response: i64, knowledge: i64 },
    ServerKnowsMoreAboutDevice,
    Decode(String),
    Api(ApiError),
}

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Info,
    Warning,
}

/// O que o `BaseStore` delega ao resto do app: banco local, aplicação das
/// entidades recebidas e eventos.
pub trait BudgetHost {
    fn device_id(&self) -> Option<&str>;
    fn is_logged_in(&self) -> bool;
    fn persist_budget_knowledge_values(&mut self, budget_version_id: &str, knowledge: &BudgetKnowledge);
    fn get_changed_budget_entities_to_sync(
        &mut self,
        budget_version_id: &str,
        knowledge: &BudgetKnowledge,
    ) -> OutgoingChangedEntities;
    fn handle_sync_budget_data_with_server_response(
        &mut self,
        knowledge: &Arc<Mutex<BudgetKnowledge>>,
        request: &SyncBudgetDataRequest,
        response: &SyncBudgetDataResponse,
    ) -> HandledResponse;
    fn possibly_import_transactions_after_budget_sync(
        &mut self,
        response: &SyncBudgetDataResponse,
        is_backfill: bool,
    );
    fn handle_permission_denied_sync_error(&mut self, error: &ServerError);
    fn budget_sync_completed(&mut self) {}
    fn log(&mut self, _level: LogLevel, _message: &str) {}
}

pub struct BudgetSync<T: Transport, H: BudgetHost> {
    pub api: ApiV1<T>,
    pub host: H,
    pub budget_schema_version: i64,
    budget_sync_in_progress: Arc<AtomicBool>,
}

impl<T: Transport, H: BudgetHost> BudgetSync<T, H> {
    pub fn new(api: ApiV1<T>, host: H, budget_schema_version: i64) -> Self {
        Self { api, host, budget_schema_version, budget_sync_in_progress: Arc::new(AtomicBool::new(false)) }
    }

    pub async fn internal_sync_budget_data_with_server(
        &mut self,
        budget_version_id: Option<&str>,
        knowledge: Option<&Arc<Mutex<BudgetKnowledge>>>,
        sync_type: SyncType,
    ) -> Result<SyncResult, SyncError> {
        let mut result = SyncResult::default();
        let Some(andamento) = Andamento::entrar(&self.budget_sync_in_progress) else {
            self.host.log(
                LogLevel::Info,
                "BaseStore::internalSyncBudgetDataWithServer:: Another sync is already in progress so the current sync request will be ignored.",
            );
            result.sync_cancelled_because_another_in_flight = true;
            return Ok(result);
        };
        // `finally { this._budgetSyncInProgress = false }`
        let outcome = self.sync_inner(budget_version_id, knowledge, sync_type, &mut result).await;
        drop(andamento);

        match outcome {
            Ok(Flow::EarlyReturn) => Ok(result),
            Ok(Flow::Completed) => {
                self.host.log(LogLevel::Info, "BaseStore::internalSyncBudgetDataWithServer::Completed.");
                self.host.budget_sync_completed();
                Ok(result)
            }
            Err(err) => Err(err),
        }
    }

    async fn sync_inner(
        &mut self,
        budget_version_id: Option<&str>,
        knowledge: Option<&Arc<Mutex<BudgetKnowledge>>>,
        sync_type: SyncType,
        result: &mut SyncResult,
    ) -> Result<Flow, SyncError> {
        let Some(budget_version_id) = budget_version_id.filter(|id| !id.is_empty()) else {
            result.server_had_changed_entities = true;
            return Ok(Flow::EarlyReturn);
        };
        if self.host.device_id().map_or(true, str::is_empty) {
            return Err(SyncError::DeviceIdNotSet);
        }
        if self.api.session_token.is_none() || !self.host.is_logged_in() {
            return Err(SyncError::NotLoggedIn);
        }
        let Some(knowledge) = knowledge else {
            return Err(SyncError::KnowledgeNotSpecified);
        };

        match self.send_and_apply(budget_version_id, knowledge, sync_type, result).await {
            Ok(()) => Ok(Flow::Completed),
            Err(SyncError::Api(api_error)) => self.recover_from_api_error(api_error, knowledge, result),
            Err(other) => Err(other),
        }
    }

    async fn send_and_apply(
        &mut self,
        budget_version_id: &str,
        knowledge: &Arc<Mutex<BudgetKnowledge>>,
        sync_type: SyncType,
        result: &mut SyncResult,
    ) -> Result<(), SyncError> {
        // const {currentDeviceKnowledge:r, currentDeviceKnowledge:s,
        //        serverKnowledgeOfDevice:o, deviceKnowledgeOfServer:u,
        //        schemaVersionOfKnowledge:l} = e
        let snapshot = *lock(knowledge);
        let r = snapshot.current_device_knowledge;
        let s = snapshot.current_device_knowledge;
        let o = snapshot.server_knowledge_of_device;
        let u = snapshot.device_knowledge_of_server;
        let l = snapshot.schema_version_of_knowledge;
        let c = if sync_type == SyncType::Backfill { 0 } else { o };
        let h = if sync_type == SyncType::Backfill { 0 } else { r };

        self.host.log(
            LogLevel::Info,
            &format!(
                "BaseStore::internalSyncBudgetDataWithServer::Called. (budgetVersionid = {budget_version_id} | schemaVersion = {} | schemaVersionOfKnowledge = {l} | currentDeviceKnowledge = {r} | serverKnowledgeOfDevice = {o} | deviceKnowledgeOfServer = {u})",
                self.budget_schema_version
            ),
        );
        self.host.persist_budget_knowledge_values(budget_version_id, &snapshot);

        let changed_entities = if h > 0 {
            self.host.get_changed_budget_entities_to_sync(budget_version_id, &snapshot)
        } else {
            OutgoingChangedEntities::default()
        };

        let request = SyncBudgetDataRequest {
            budget_version_id: budget_version_id.to_owned(),
            sync_type,
            starting_device_knowledge: c,
            ending_device_knowledge: h,
            device_knowledge_of_server: u,
            calculated_entities_included: false,
            schema_version: self.budget_schema_version,
            schema_version_of_knowledge: l,
            changed_entities,
        };
        let http = self.api.sync_budget_data_with_server(&request).await.map_err(SyncError::Api)?;
        result.server_app_version = http.header(SERVER_VERSION_HEADER).map(str::to_owned);
        let response: SyncBudgetDataResponse =
            serde_json::from_value(http.data).map_err(|e| SyncError::Decode(e.to_string()))?;

        // y: houve mudança local enquanto a requisição estava em voo?
        let had_local_changes_in_flight = lock(knowledge).current_device_knowledge > s;

        // Checks de versão, nesta ordem.
        if response.schema_version_of_response != self.budget_schema_version {
            return Err(SyncError::UnexpectedSchemaVersion {
                expected: self.budget_schema_version,
                received: response.schema_version_of_response,
            });
        }
        let knowledge_schema = lock(knowledge).schema_version_of_knowledge;
        if response.schema_version_of_response < knowledge_schema {
            return Err(SyncError::SchemaVersionBelowKnowledge {
                response: response.schema_version_of_response,
                knowledge: knowledge_schema,
            });
        }

        let device_knowledge_of_server = lock(knowledge).device_knowledge_of_server;
        let m = response.current_server_knowledge - device_knowledge_of_server;
        let message = if m > 0 {
            format!("BaseStore::internalSyncBudgetDataWithServer::Server knowledge has gone up by {m}. We should be getting back entities from the server.")
        } else {
            format!("BaseStore::internalSyncBudgetDataWithServer::Server knowledge has remained at {}. We should be getting back 0 entities from the server.", response.current_server_knowledge)
        };
        self.host.log(LogLevel::Info, &message);

        {
            let mut k = lock(knowledge);
            k.server_knowledge_of_device = response.server_knowledge_of_device;
            if k.current_device_knowledge < response.server_knowledge_of_device {
                if k.current_device_knowledge != 0 {
                    return Err(SyncError::ServerKnowsMoreAboutDevice);
                }
                k.current_device_knowledge = response.server_knowledge_of_device;
            }
            update_queue_calculations_for_server_entities_flag(&mut k, &request.changed_entities, &response);
        }

        if sync_type == SyncType::Delta && had_local_changes_in_flight {
            self.host.log(
                LogLevel::Info,
                "BaseStore::internalSyncBudgetDataWithServer::Incoming changes from delta sync were ignored because we had new local changes.",
            );
        } else {
            let handled =
                self.host.handle_sync_budget_data_with_server_response(knowledge, &request, &response);
            result.server_had_changed_entities = handled.server_had_changed_entities;
            if handled.should_persist_server_knowledge && sync_type != SyncType::Bootstrap {
                let persisted = {
                    let mut k = lock(knowledge);
                    k.schema_version_of_knowledge = response.schema_version_of_response;
                    k.device_knowledge_of_server = response.current_server_knowledge;
                    *k
                };
                self.host.persist_budget_knowledge_values(budget_version_id, &persisted);
            }
            if sync_type != SyncType::Bootstrap && !self.api.is_read_only {
                self.host.possibly_import_transactions_after_budget_sync(
                    &response,
                    sync_type == SyncType::Backfill,
                );
            }
        }
        result.server_response = Some(response);
        Ok(())
    }

    /// O bloco `catch`: decide se o erro sobe ou é engolido.
    fn recover_from_api_error(
        &mut self,
        error: ApiError,
        knowledge: &Arc<Mutex<BudgetKnowledge>>,
        result: &mut SyncResult,
    ) -> Result<Flow, SyncError> {
        let mut rethrow = true;
        if let ApiError::Server(server_error) = &error {
            if server_error.status_code == 401 && self.api.session_token.is_some() {
                self.api.session_token = None;
            } else {
                match server_error.id.as_str() {
                    ERR_SERVER_KNOWLEDGE_OF_DEVICE_EXCEEDS => {
                        let reported = server_error
                            .data
                            .as_ref()
                            .and_then(|d| d.get("server_knowledge_of_device"))
                            .and_then(Value::as_i64)
                            .filter(|v| *v != 0);
                        if let Some(v) = reported {
                            if v <= lock(knowledge).current_device_knowledge {
                                self.host.log(
                                    LogLevel::Warning,
                                    &format!("BaseStore::internalSyncBudgetDataWithServer::Gracefully ignoring error id={} because this client knows as much as (or more than) the server even though the server thought it knew more.", server_error.id),
                                );
                                rethrow = false;
                            }
                        }
                    }
                    ERR_NO_READ_PERMISSIONS => {
                        rethrow = false;
                        self.host.handle_permission_denied_sync_error(server_error);
                    }
                    _ => rethrow = true,
                }
            }
        }
        if rethrow {
            return Err(SyncError::Api(error));
        }
        result.server_had_changed_entities = true;
        Ok(Flow::EarlyReturn)
    }
}

/// A marca de "sincronização em andamento". Solta a marca ao sair, inclusive
/// quando a future é descartada no meio de um `await` (aí não há `finally`).
struct Andamento(Arc<AtomicBool>);

impl Andamento {
    /// `None` se outra sincronização já está em andamento.
    fn entrar(marca: &Arc<AtomicBool>) -> Option<Self> {
        marca.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).ok().map(|_| Self(Arc::clone(marca)))
    }
}

impl Drop for Andamento {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

enum Flow {
    /// Retorno antes do fim: não dispara `budgetSyncCompleted`.
    EarlyReturn,
    Completed,
}

fn lock(k: &Arc<Mutex<BudgetKnowledge>>) -> std::sync::MutexGuard<'_, BudgetKnowledge> {
    k.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `updateQueueCalculationsForServerEntitiesFlag`: liga o flag se alguma
/// entidade enviada voltar do servidor com o mesmo `id`.
pub fn update_queue_calculations_for_server_entities_flag(
    knowledge: &mut BudgetKnowledge,
    outgoing: &OutgoingChangedEntities,
    response: &SyncBudgetDataResponse,
) {
    if knowledge.queue_calculations_for_server_entities == 1 {
        return;
    }
    let incoming = &response.changed_entities;
    let collides = |sent: &Option<Vec<Value>>, received: &Option<Vec<Value>>| -> bool {
        let (Some(sent), Some(received)) = (sent, received) else { return false };
        sent.iter().any(|s| received.iter().any(|r| s.get("id") == r.get("id")))
    };
    let r = collides(&outgoing.be_accounts, &incoming.be_accounts)
        || collides(&outgoing.be_monthly_subcategory_budgets, &incoming.be_monthly_subcategory_budgets)
        || collides(&outgoing.be_scheduled_transaction_groups, &incoming.be_scheduled_transactions)
        || collides(&outgoing.be_subcategories, &incoming.be_subcategories)
        || collides(&outgoing.be_transaction_groups, &incoming.be_transactions);
    if r {
        knowledge.queue_calculations_for_server_entities = 1;
    }
}
