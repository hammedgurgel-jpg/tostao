//! CORS do `tostao-servidor`: a página no navegador (outra origem) só lê a
//! resposta se o servidor disser que aceita a origem dela. Sobe o servidor
//! numa porta livre e fala HTTP cru, para ver os cabeçalhos como o navegador.

use std::io::{Read, Write};
use std::net::TcpStream;

use serde_json::json;
use tostao::http::{servir, Acesso};
use tostao::protocol::{SERVER_VERSION_HEADER, TOSTAO_SCHEMA_VERSION};
use tostao::server::SyncServer;

const ORIGEM: &str = "http://127.0.0.1:8000";
const TOKEN: &str = "segredo-de-teste";

/// Sobe o servidor, que aceita só a [`ORIGEM`], e devolve a porta.
fn subir() -> u16 {
    let http = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let porta = http.server_addr().to_ip().unwrap().port();
    let acesso = Acesso { usuario: "familia".into(), token: TOKEN.into(), origens: vec![ORIGEM.into()] };
    let mut servidor = SyncServer::open_in_memory(TOSTAO_SCHEMA_VERSION).unwrap();
    std::thread::spawn(move || servir(&http, &mut servidor, &acesso));
    porta
}

struct Resposta {
    status: u16,
    cabecalhos: Vec<(String, String)>,
    corpo: String,
}

impl Resposta {
    fn cabecalho(&self, nome: &str) -> Option<&str> {
        self.cabecalhos.iter().find(|(k, _)| k.eq_ignore_ascii_case(nome)).map(|(_, v)| v.as_str())
    }
}

fn pedir(porta: u16, metodo: &str, cabecalhos: &[(&str, &str)], corpo: &str) -> Resposta {
    let mut pedido = format!("{metodo} /api/v1/catalog HTTP/1.1\r\nHost: 127.0.0.1:{porta}\r\nConnection: close\r\n");
    pedido += &format!("Content-Length: {}\r\n", corpo.len());
    for (nome, valor) in cabecalhos {
        pedido += &format!("{nome}: {valor}\r\n");
    }
    pedido += "\r\n";
    pedido += corpo;

    let mut conexao = TcpStream::connect(("127.0.0.1", porta)).unwrap();
    conexao.write_all(pedido.as_bytes()).unwrap();
    let mut texto = String::new();
    conexao.read_to_string(&mut texto).unwrap();

    let (cabecalho, corpo) = texto.split_once("\r\n\r\n").expect("resposta HTTP");
    let mut linhas = cabecalho.lines();
    let status = linhas.next().unwrap().split_whitespace().nth(1).unwrap().parse().unwrap();
    let cabecalhos = linhas
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect();
    Resposta { status, cabecalhos, corpo: corpo.to_owned() }
}

/// Uma sincronização de orçamento novo, como o aparelho manda.
fn sincronizacao() -> String {
    json!({
        "operation_name": "syncBudgetData",
        "request_data": json!({
            "budget_version_id": "orcamento-cors",
            "sync_type": "delta",
            "starting_device_knowledge": 0,
            "ending_device_knowledge": 0,
            "device_knowledge_of_server": 0,
            "calculated_entities_included": false,
            "schema_version": TOSTAO_SCHEMA_VERSION,
            "schema_version_of_knowledge": TOSTAO_SCHEMA_VERSION,
            "changed_entities": {}
        })
        .to_string()
    })
    .to_string()
}

fn post_autorizado<'a>(origem: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("Origin", origem),
        ("Authorization", "Bearer segredo-de-teste"),
        ("X-Tostao-Device-Id", "aparelho-1"),
        ("Content-Type", "application/json"),
    ]
}

#[test]
fn preflight_libera_a_origem_da_pagina() {
    let porta = subir();
    let r = pedir(
        porta,
        "OPTIONS",
        &[
            ("Origin", ORIGEM),
            ("Access-Control-Request-Method", "POST"),
            ("Access-Control-Request-Headers", "authorization, content-type, x-tostao-device-id"),
        ],
        "",
    );
    assert_eq!(r.status, 204);
    assert_eq!(r.cabecalho("Access-Control-Allow-Origin"), Some(ORIGEM));
    // Um cabeçalho repetido, mesmo com o mesmo valor, o navegador recusa.
    assert_eq!(r.cabecalhos.iter().filter(|(k, _)| k.eq_ignore_ascii_case("Access-Control-Allow-Origin")).count(), 1);
    assert!(r.cabecalho("Access-Control-Allow-Methods").unwrap_or_default().contains("POST"));
    let permitidos = r.cabecalho("Access-Control-Allow-Headers").unwrap_or_default().to_ascii_lowercase();
    for nome in ["authorization", "content-type", "x-tostao-device-id"] {
        assert!(permitidos.contains(nome), "faltou {nome} em {permitidos}");
    }
}

#[test]
fn resposta_de_sincronizacao_traz_cors_e_expoe_a_versao_do_servidor() {
    let porta = subir();
    let r = pedir(porta, "POST", &post_autorizado(ORIGEM), &sincronizacao());
    assert_eq!(r.status, 200, "{}", r.corpo);
    assert_eq!(r.cabecalho("Access-Control-Allow-Origin"), Some(ORIGEM));
    // Sem isto, o navegador não deixa a página ler o cabeçalho da versão.
    let expostos = r.cabecalho("Access-Control-Expose-Headers").unwrap_or_default();
    assert!(expostos.contains(SERVER_VERSION_HEADER), "{expostos}");
}

#[test]
fn erro_de_token_tambem_traz_cors_para_a_pagina_ler() {
    let porta = subir();
    let mut cabecalhos = post_autorizado(ORIGEM);
    cabecalhos[1] = ("Authorization", "Bearer token-errado");
    let r = pedir(porta, "POST", &cabecalhos, &sincronizacao());
    assert_eq!(r.status, 401);
    assert_eq!(r.cabecalho("Access-Control-Allow-Origin"), Some(ORIGEM));
}

#[test]
fn origem_que_nao_esta_na_lista_nao_ganha_cors() {
    let porta = subir();
    let r = pedir(porta, "POST", &post_autorizado("http://outro.example"), &sincronizacao());
    assert_eq!(r.cabecalho("Access-Control-Allow-Origin"), None);
}
