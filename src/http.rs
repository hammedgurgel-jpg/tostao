//! `POST /api/v1/catalog` por HTTP de verdade: o lado do servidor (com
//! `tiny_http`, porque o [`SyncServer`] é síncrono e atende um pedido por
//! vez) e o [`Transport`] do aparelho (com `ureq`, sem TLS).
//!
//! A codificação no fio não foi observada no YNAB (ver `lib.rs`). Escolhas
//! feitas aqui, **não conferidas**:
//! - corpo da requisição: o [`CatalogRequest`] como JSON
//!   (`{"operation_name": …, "request_data": "<texto JSON>"}`),
//!   `Content-Type: application/json`;
//! - resposta de sucesso: status 200, o `data` como JSON no corpo e a
//!   versão do servidor em [`crate::protocol::SERVER_VERSION_HEADER`];
//! - erro: o status do [`ServerError`] e o corpo
//!   `{"error": {"id": …, "data": …}}`;
//! - autenticação: `Authorization: Bearer <token>`, um token só, que vale
//!   para um usuário fixo; token errado ou ausente dá 401 `unauthorized`;
//! - o id do aparelho vai no cabeçalho [`DEVICE_ID_HEADER`].
//!
//! O servidor cria o orçamento na primeira sincronização do usuário, porque
//! a operação de criar orçamento do YNAB não foi lida.

#[cfg(feature = "servidor")]
use std::io::Read;

#[cfg(feature = "servidor")]
use serde_json::json;
use serde_json::Value;

use crate::api::{ApiError, HttpResponse, ServerError, Transport};
#[cfg(feature = "servidor")]
use crate::protocol::operation;
use crate::protocol::CatalogRequest;
#[cfg(feature = "servidor")]
use crate::protocol::CATALOG_PATH;
#[cfg(feature = "servidor")]
use crate::server::{SyncServer, ERR_INVALID_REQUEST};

/// Cabeçalho com o id do aparelho (nome escolhido aqui).
pub const DEVICE_ID_HEADER: &str = "X-Tostao-Device-Id";

#[cfg(feature = "servidor")]
pub const ERR_UNAUTHORIZED: &str = "unauthorized";
#[cfg(feature = "servidor")]
pub const ERR_NOT_FOUND: &str = "not_found";

/// Corpo maior do que isto é recusado.
#[cfg(feature = "servidor")]
const MAX_BODY: u64 = 64 * 1024 * 1024;

/// Quem pode usar o servidor: um usuário fixo e o token dele.
#[cfg(feature = "servidor")]
#[derive(Debug, Clone)]
pub struct Acesso {
    pub usuario: String,
    pub token: String,
}

/// Atende os pedidos, um de cada vez, até o `tiny_http` parar.
#[cfg(feature = "servidor")]
pub fn servir(http: &tiny_http::Server, servidor: &mut SyncServer, acesso: &Acesso) {
    for request in http.incoming_requests() {
        atender(servidor, acesso, request);
    }
}

/// Atende um pedido e responde.
#[cfg(feature = "servidor")]
pub fn atender(servidor: &mut SyncServer, acesso: &Acesso, mut request: tiny_http::Request) {
    let (status, headers, body) = match responder(servidor, acesso, &mut request) {
        Ok(r) => (200, r.headers, r.data),
        Err(e) => (e.status_code, Vec::new(), json!({ "error": { "id": e.id, "data": e.data } })),
    };
    let mut response = tiny_http::Response::from_string(body.to_string()).with_status_code(status);
    for (k, v) in headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).chain([("Content-Type", "application/json")]) {
        if let Ok(h) = tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()) {
            response.add_header(h);
        }
    }
    let _ = request.respond(response);
}

#[cfg(feature = "servidor")]
fn responder(
    servidor: &mut SyncServer,
    acesso: &Acesso,
    request: &mut tiny_http::Request,
) -> Result<HttpResponse, ServerError> {
    if request.url() != CATALOG_PATH || *request.method() != tiny_http::Method::Post {
        return Err(erro(404, ERR_NOT_FOUND, json!({ "path": request.url() })));
    }
    let header = |name: &str| {
        request.headers().iter().find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|h| h.value.as_str().to_owned())
    };
    if header("Authorization").as_deref() != Some(format!("Bearer {}", acesso.token).as_str()) {
        return Err(ServerError { status_code: 401, id: ERR_UNAUTHORIZED.into(), data: None });
    }
    let device_id = header(DEVICE_ID_HEADER)
        .filter(|d| !d.is_empty())
        .ok_or_else(|| erro(400, ERR_INVALID_REQUEST, json!({ "message": format!("falta o cabeçalho {DEVICE_ID_HEADER}") })))?;

    let mut body = String::new();
    request
        .as_reader()
        .take(MAX_BODY)
        .read_to_string(&mut body)
        .map_err(|e| erro(400, ERR_INVALID_REQUEST, json!({ "message": e.to_string() })))?;
    let catalog: CatalogRequest = serde_json::from_str(&body)
        .map_err(|e| erro(400, ERR_INVALID_REQUEST, json!({ "message": e.to_string() })))?;

    if catalog.operation_name == operation::SYNC_BUDGET_DATA {
        let budget = serde_json::from_str::<Value>(&catalog.request_data)
            .ok()
            .and_then(|v| v.get("budget_version_id")?.as_str().map(str::to_owned));
        if let Some(budget) = budget.filter(|b| !b.is_empty()) {
            let interno = |e: rusqlite::Error| erro(500, crate::server::ERR_INTERNAL, json!({ "message": e.to_string() }));
            if servidor.current_server_knowledge(&budget).map_err(interno)?.is_none() {
                servidor.create_budget(&budget, &acesso.usuario).map_err(interno)?;
            }
        }
    }
    servidor.handle_catalog(&acesso.usuario, &device_id, &catalog)
}

#[cfg(feature = "servidor")]
fn erro(status_code: u16, id: &str, data: Value) -> ServerError {
    ServerError { status_code, id: id.into(), data: Some(data) }
}

/// [`Transport`] por HTTP, para um aparelho.
#[cfg(feature = "http")]
pub struct HttpTransport {
    /// Ex.: `http://127.0.0.1:8080`, sem a barra final.
    pub servidor: String,
    pub token: String,
    pub device_id: String,
    agent: ureq::Agent,
}

#[cfg(feature = "http")]
impl HttpTransport {
    pub fn new(servidor: &str, token: &str, device_id: &str) -> Self {
        Self {
            servidor: servidor.trim_end_matches('/').to_owned(),
            token: token.to_owned(),
            device_id: device_id.to_owned(),
            agent: ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(60)).build(),
        }
    }
}

#[cfg(feature = "http")]
impl Transport for HttpTransport {
    fn post(&mut self, path: &str, body: &CatalogRequest) -> Result<HttpResponse, ApiError> {
        let body = serde_json::to_string(body).map_err(|e| ApiError::Transport(e.to_string()))?;
        let result = self
            .agent
            .post(&format!("{}{path}", self.servidor))
            .set("Authorization", &format!("Bearer {}", self.token))
            .set(DEVICE_ID_HEADER, &self.device_id)
            .set("Content-Type", "application/json")
            .send_string(&body);
        match result {
            Ok(response) => {
                let headers =
                    response.headers_names().into_iter().filter_map(|n| Some((n.clone(), response.header(&n)?.to_owned()))).collect();
                let text = response.into_string().map_err(|e| ApiError::Transport(e.to_string()))?;
                let data = serde_json::from_str(&text).map_err(|e| ApiError::Transport(e.to_string()))?;
                Ok(HttpResponse { headers, data })
            }
            Err(ureq::Error::Status(status_code, response)) => {
                let text = response.into_string().unwrap_or_default();
                let error = serde_json::from_str::<Value>(&text).ok().and_then(|v| v.get("error").cloned());
                let id = error.as_ref().and_then(|e| e.get("id")?.as_str().map(str::to_owned));
                let data = error.and_then(|e| e.get("data").cloned()).filter(|d| !d.is_null());
                Err(ApiError::Server(ServerError {
                    status_code,
                    id: id.unwrap_or_else(|| format!("http_{status_code}")),
                    data,
                }))
            }
            Err(e) => Err(ApiError::Transport(e.to_string())),
        }
    }
}
