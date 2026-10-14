//! Adaptador da API v1: envelope do catálogo e bloqueio de escrita em só-leitura.

use std::future::Future;

use crate::protocol::{operation, CatalogRequest, SyncBudgetDataRequest, CATALOG_PATH};
use serde_json::Value;

/// Resposta HTTP já decodificada.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub headers: Vec<(String, String)>,
    pub data: Value,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Erro devolvido pelo servidor (`Xe.o` no bundle).
#[derive(Debug, Clone, PartialEq)]
pub struct ServerError {
    pub status_code: u16,
    pub id: String,
    /// `getDataObject()`.
    pub data: Option<Value>,
}

#[derive(Debug)]
pub enum ApiError {
    Server(ServerError),
    /// `WriteRequestWhileReadOnlyError`.
    WriteRequestWhileReadOnly(String),
    Transport(String),
}

/// Transporte HTTP. A codificação do corpo no fio não foi observada
/// (a captura não extraiu o formato da requisição); fica a cargo de quem
/// implementa.
///
/// `async` porque, no navegador, o envio é um `fetch`. No binário, o
/// [`HttpTransport`](crate::http::HttpTransport) bloqueia dentro da espera.
pub trait Transport {
    fn post(&mut self, path: &str, body: &CatalogRequest) -> impl Future<Output = Result<HttpResponse, ApiError>>;
}

pub struct ApiV1<T: Transport> {
    pub transport: T,
    pub is_read_only: bool,
    pub session_token: Option<String>,
}

impl<T: Transport> ApiV1<T> {
    pub fn new(transport: T) -> Self {
        Self { transport, is_read_only: false, session_token: None }
    }

    /// `sendCatalogRequest`: checa só-leitura, depois envia
    /// `{operation_name, request_data: JSON.stringify(e)}`.
    pub async fn send_catalog_request(
        &mut self,
        operation_name: &str,
        request_data: &Value,
    ) -> Result<HttpResponse, ApiError> {
        self.throw_if_write_request_while_read_only(operation_name, request_data)?;
        let body = CatalogRequest {
            operation_name: operation_name.to_owned(),
            request_data: request_data.to_string(),
        };
        self.transport.post(CATALOG_PATH, &body).await
    }

    pub async fn sync_budget_data_with_server(
        &mut self,
        request: &SyncBudgetDataRequest,
    ) -> Result<HttpResponse, ApiError> {
        let data = serde_json::to_value(request).map_err(|e| ApiError::Transport(e.to_string()))?;
        self.send_catalog_request(operation::SYNC_BUDGET_DATA, &data).await
    }

    /// `throwIfWriteRequestWhileReadOnly`.
    fn throw_if_write_request_while_read_only(
        &self,
        operation_name: &str,
        request_data: &Value,
    ) -> Result<(), ApiError> {
        if !self.is_read_only {
            return Ok(());
        }
        match operation_name {
            operation::LOGIN_USER | operation::LOGOUT_USER | operation::GET_INITIAL_USER_DATA => {
                return Ok(())
            }
            operation::SYNC_CATALOG_DATA | operation::SYNC_BUDGET_DATA | operation::SYNC_FAMILY_DATA => {
                if !has_changed_entities(request_data) {
                    return Ok(());
                }
            }
            _ => {}
        }
        Err(ApiError::WriteRequestWhileReadOnly(format!(
            "APIv1.sendCatalogRequest \"{operation_name}\" write request blocked while read-only"
        )))
    }
}

/// `hasChangedEntities`: `(ending_device_knowledge ?? 0) > (starting_device_knowledge ?? 0)`.
pub fn has_changed_entities(request_data: &Value) -> bool {
    let read = |k: &str| request_data.get(k).and_then(Value::as_i64).unwrap_or(0);
    read("ending_device_knowledge") > read("starting_device_knowledge")
}

/// Erro do servidor a partir do status e do corpo `{"error": {"id", "data"}}`.
/// Sem um corpo desses, o id é `http_<status>`. É o mesmo formato para
/// qualquer transporte (ureq ou `fetch`).
pub fn erro_do_servidor(status_code: u16, corpo: &str) -> ServerError {
    let error = serde_json::from_str::<Value>(corpo).ok().and_then(|v| v.get("error").cloned());
    let id = error.as_ref().and_then(|e| e.get("id")?.as_str().map(str::to_owned));
    let data = error.and_then(|e| e.get("data").cloned()).filter(|d| !d.is_null());
    ServerError { status_code, id: id.unwrap_or_else(|| format!("http_{status_code}")), data }
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    #[test]
    fn erro_do_corpo_traz_id_e_dados() {
        let corpo = r#"{"error":{"id":"unauthorized","data":{"message":"x"}}}"#;
        let erro = erro_do_servidor(401, corpo);
        assert_eq!(erro, ServerError { status_code: 401, id: "unauthorized".into(), data: Some(json!({"message": "x"})) });
    }

    #[test]
    fn corpo_que_nao_e_erro_vira_id_pelo_status() {
        let erro = erro_do_servidor(502, "<html>");
        assert_eq!(erro, ServerError { status_code: 502, id: "http_502".into(), data: None });
    }
}
