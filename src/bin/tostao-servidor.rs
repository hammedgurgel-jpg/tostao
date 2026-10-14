//! Servidor do Tostão: atende `POST /api/v1/catalog` por HTTP e guarda os
//! orçamentos num arquivo SQLite. Ver `README.md`.

use std::io::Write;
use std::process::ExitCode;

use tostao::http::{servir, Acesso};
use tostao::protocol::TOSTAO_SCHEMA_VERSION;
use tostao::server::SyncServer;

const AJUDA: &str = "\
tostao-servidor: servidor de sincronização do Tostão

Uso: tostao-servidor [opções]

Opções (ou variáveis de ambiente):
  --token <texto>       TOSTAO_TOKEN     token que os aparelhos enviam (obrigatório)
  --banco <caminho>     TOSTAO_BANCO     arquivo SQLite (padrão: tostao.db)
  --porta <número>      TOSTAO_PORTA     porta (padrão: 8080; 0 = qualquer livre)
  --endereco <ip>       TOSTAO_ENDERECO  endereço (padrão: 127.0.0.1, só esta máquina)
  --usuario <nome>      TOSTAO_USUARIO   usuário dono dos orçamentos (padrão: familia)
  --origem <url>        TOSTAO_ORIGEM    origem da página no navegador, ex.: http://127.0.0.1:8000
                                         (pode repetir; no ambiente, separe por vírgula)
  --ajuda                                mostra esta ajuda
";

fn main() -> ExitCode {
    match executar() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erro: {e}");
            ExitCode::FAILURE
        }
    }
}

fn executar() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let ambiente = |nome: &str| std::env::var(nome).ok().filter(|v| !v.is_empty());
    let mut token = ambiente("TOSTAO_TOKEN");
    let mut banco = ambiente("TOSTAO_BANCO").unwrap_or_else(|| "tostao.db".into());
    let mut porta = ambiente("TOSTAO_PORTA").unwrap_or_else(|| "8080".into());
    let mut endereco = ambiente("TOSTAO_ENDERECO").unwrap_or_else(|| "127.0.0.1".into());
    let mut usuario = ambiente("TOSTAO_USUARIO").unwrap_or_else(|| "familia".into());
    let mut origens: Vec<String> = ambiente("TOSTAO_ORIGEM")
        .map(|v| v.split(',').map(|o| o.trim().trim_end_matches('/').to_owned()).filter(|o| !o.is_empty()).collect())
        .unwrap_or_default();

    while let Some(arg) = args.next() {
        let mut valor = || args.next().ok_or_else(|| format!("falta o valor de {arg}"));
        match arg.as_str() {
            "--token" => token = Some(valor()?),
            "--banco" => banco = valor()?,
            "--porta" => porta = valor()?,
            "--endereco" => endereco = valor()?,
            "--usuario" => usuario = valor()?,
            "--origem" => origens.push(valor()?.trim_end_matches('/').to_owned()),
            "--ajuda" | "-h" | "--help" => {
                print!("{AJUDA}");
                return Ok(());
            }
            outro => return Err(format!("opção desconhecida: {outro}\n\n{AJUDA}")),
        }
    }
    let token = token.ok_or_else(|| format!("defina o token com --token ou TOSTAO_TOKEN\n\n{AJUDA}"))?;
    let porta: u16 = porta.parse().map_err(|_| format!("porta inválida: {porta}"))?;

    let mut servidor = SyncServer::open(&banco, TOSTAO_SCHEMA_VERSION)
        .map_err(|e| format!("não deu para abrir o banco {banco}: {e}"))?;
    let http = tiny_http::Server::http((endereco.as_str(), porta))
        .map_err(|e| format!("não deu para ouvir em {endereco}:{porta}: {e}"))?;
    let ouvindo = http.server_addr().to_ip().map_or(format!("{endereco}:{porta}"), |a| a.to_string());

    if !(endereco == "127.0.0.1" || endereco == "localhost" || endereco == "::1") {
        eprintln!("aviso: ouvindo em {endereco}, fora desta máquina; o token é a única proteção e o tráfego não é cifrado");
    }
    println!("Tostão: servidor em http://{ouvindo} (banco: {banco}, usuário: {usuario})");
    if origens.is_empty() {
        println!("Nenhuma origem de página liberada (use --origem para a página no navegador).");
    } else {
        println!("Páginas liberadas (CORS): {}", origens.join(", "));
    }
    println!("Ctrl+C para parar.");
    let _ = std::io::stdout().flush();

    servir(&http, &mut servidor, &Acesso { usuario, token, origens });
    Ok(())
}
