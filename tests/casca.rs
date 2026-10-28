//! A casca do app no navegador (PWA): o manifesto que deixa instalar e a
//! lista de arquivos que o service worker guarda para abrir sem rede. Estes
//! testes conferem os arquivos de `web/`; o comportamento no navegador
//! (instalar, abrir offline) é checado à mão, ver a issue #5.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

fn web() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("web")
}

fn manifesto() -> Value {
    let texto = fs::read_to_string(web().join("manifest.webmanifest")).expect("web/manifest.webmanifest");
    serde_json::from_str(&texto).expect("manifesto é JSON")
}

/// Os caminhos da constante `PRECACHE` de `web/sw.js`, na ordem em que aparecem.
fn precache() -> Vec<String> {
    let codigo = fs::read_to_string(web().join("sw.js")).expect("web/sw.js");
    let inicio = codigo.find("const PRECACHE = [").expect("const PRECACHE = [ em sw.js");
    let corpo = &codigo[inicio..];
    let corpo = &corpo[..corpo.find("];").expect("fim de PRECACHE")];
    corpo
        .split('"')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, s)| s.to_owned())
        .collect()
}

/// Largura e altura de um PNG, lidas do cabeçalho IHDR.
fn tamanho_png(caminho: &Path) -> (u32, u32) {
    let bytes = fs::read(caminho).unwrap_or_else(|e| panic!("{}: {e}", caminho.display()));
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "{} não é PNG", caminho.display());
    let largura = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let altura = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    (largura, altura)
}

#[test]
fn manifesto_instala_em_modo_standalone_com_icones_192_e_512() {
    let m = manifesto();
    assert!(m["name"].as_str().is_some_and(|n| !n.is_empty()), "name");
    assert_eq!(m["display"], "standalone");
    assert!(m["start_url"].is_string(), "start_url");
    assert_eq!(m["lang"], "pt-BR");

    let icones = m["icons"].as_array().expect("icons");
    for tamanho in [192u32, 512] {
        let icone = icones
            .iter()
            .find(|i| i["sizes"] == format!("{tamanho}x{tamanho}"))
            .unwrap_or_else(|| panic!("sem ícone {tamanho}x{tamanho}"));
        assert_eq!(icone["type"], "image/png");
        let src = icone["src"].as_str().expect("src do ícone");
        assert_eq!(tamanho_png(&web().join(src)), (tamanho, tamanho), "{src}");
    }
}

#[test]
fn service_worker_guarda_a_casca_e_os_icones_do_manifesto() {
    let guardados = precache();
    for esperado in ["index.html", "app.js", "manifest.webmanifest", "pkg/tostao.js", "pkg/tostao_bg.wasm"] {
        assert!(guardados.iter().any(|g| g == esperado), "{esperado} fora do PRECACHE");
    }
    for icone in manifesto()["icons"].as_array().expect("icons") {
        let src = icone["src"].as_str().expect("src");
        assert!(guardados.iter().any(|g| g == src), "{src} fora do PRECACHE");
    }
}

#[test]
fn pagina_carrega_o_mesmo_modulo_wasm_que_a_casca_guarda() {
    let app = fs::read_to_string(web().join("app.js")).expect("web/app.js");
    assert!(app.contains("from \"./pkg/tostao.js\""), "app.js não importa ./pkg/tostao.js");
    assert!(precache().iter().any(|g| g == "pkg/tostao.js"));
}

#[test]
fn arquivos_da_casca_existem_em_web() {
    for caminho in precache() {
        if caminho.starts_with("pkg/") {
            // Gerados por `web/construir.ps1`; não ficam no repositório.
            continue;
        }
        assert!(web().join(&caminho).is_file(), "{caminho} não existe em web/");
    }
}

/// A versão nova é instalada inteira ou não é: todos os arquivos são buscados
/// antes de gravar, ignorando o cache HTTP, e só então vem `skipWaiting`. Um
/// arquivo que falta derruba a instalação; a versão anterior segue valendo.
#[test]
fn instalacao_guarda_a_casca_inteira_ou_nada() {
    let codigo = fs::read_to_string(web().join("sw.js")).expect("web/sw.js");
    assert!(!codigo.contains("addAll"), "addAll grava em lote e não é o que queremos aqui");
    assert!(codigo.contains("cache: \"no-cache\""), "a busca da casca precisa ignorar o cache HTTP");
    assert_eq!(codigo.matches("self.skipWaiting()").count(), 1, "skipWaiting só na mensagem de ativar");
    let gravacao = codigo.find("cache.put(").expect("cache.put na instalação");
    let ativacao = codigo.find("self.skipWaiting()").expect("skipWaiting");
    let pedido = codigo.find("\"SKIP_WAITING\"").expect("ativação pedida pelo app");
    assert!(gravacao < pedido && pedido < ativacao, "a versão nova espera o pedido, depois de gravar");
    assert!(codigo.contains("Promise.all("), "a casca é buscada em conjunto");
    assert!(codigo.contains("caches.delete(CACHE)"), "cache parcial precisa ser apagado se a instalação falhar");
}

/// O cache é só para a casca: qualquer outro GET do mesmo site passa pela rede
/// sem ser guardado, e toda resposta da casca sai do cache desta versão.
#[test]
fn resposta_da_casca_sai_so_do_cache_desta_versao() {
    let codigo = fs::read_to_string(web().join("sw.js")).expect("web/sw.js");
    assert!(codigo.contains("CASCA.has(alvo)"), "resposta sem checar se o pedido é da casca");
    assert!(codigo.contains("const CACHE = `tostao-casca-${VERSAO}`"), "o cache precisa levar o nome da versão");
    assert!(!codigo.contains("stale"), "a casca não é mais stale-while-revalidate");
}

/// A versão sai do build (`construir.ps1`, hash dos arquivos da casca), nunca
/// de edição à mão. O navegador só instala a casca nova se `sw.js` mudar, por
/// isso a versão está nele.
#[test]
fn versao_da_casca_vem_do_build() {
    let codigo = fs::read_to_string(web().join("sw.js")).expect("web/sw.js");
    assert!(codigo.contains("const VERSAO = \""), "const VERSAO em sw.js");
    let construir = fs::read_to_string(web().join("construir.ps1")).expect("web/construir.ps1");
    assert!(construir.contains("const VERSAO"), "construir.ps1 precisa gravar a versão em sw.js");
    assert!(construir.contains("Get-FileHash"), "a versão é o hash dos arquivos da casca");
}

/// O navegador não pode pegar o `sw.js` antigo do cache HTTP: sem isso, a
/// checagem de versão nova demora até o max-age do Pages acabar.
#[test]
fn registro_do_service_worker_ignora_cache_http_do_script() {
    let app = fs::read_to_string(web().join("app.js")).expect("web/app.js");
    assert!(app.contains("updateViaCache: \"none\""), "register sem updateViaCache: \"none\"");
}

/// Cor do tema e ícones aparecem na página e no manifesto: os dois têm de bater.
#[test]
fn cabecalho_da_pagina_bate_com_o_manifesto() {
    let html = fs::read_to_string(web().join("index.html")).expect("web/index.html");
    let m = manifesto();
    let cor = m["theme_color"].as_str().expect("theme_color");
    assert!(html.contains(&format!("<meta name=\"theme-color\" content=\"{cor}\">")), "theme-color diferente do manifesto");
    assert!(html.contains("<link rel=\"manifest\" href=\"manifest.webmanifest\">"));
    // O iOS usa este link, não o manifesto, para o ícone da tela inicial.
    assert!(html.contains("<link rel=\"apple-touch-icon\" href=\"icons/icon-192.png\">"), "sem apple-touch-icon");
    let icone = m["icons"][0]["src"].as_str().expect("src do ícone");
    assert!(html.contains(&format!("href=\"{icone}\"")), "ícone da página diferente do manifesto");
}

/// Instalação pelo botão do próprio app: o navegador oferece a instalação
/// (`beforeinstallprompt`), o botão aparece, e some quando o app já está
/// instalado. O botão começa escondido, então quem não pode instalar não o vê.
#[test]
fn pagina_oferece_botao_para_instalar_que_some_depois_de_instalado() {
    let html = fs::read_to_string(web().join("index.html")).expect("web/index.html");
    let app = fs::read_to_string(web().join("app.js")).expect("web/app.js");
    assert!(html.contains("<button type=\"button\" id=\"instalar\" hidden>Instalar o Tostão</button>"), "botão instalar escondido");
    assert!(app.contains("\"beforeinstallprompt\""), "app.js não escuta beforeinstallprompt");
    assert!(app.contains("e.preventDefault()"), "a oferta do navegador precisa ser guardada, não mostrada");
    assert!(app.contains("oferta.prompt()"), "o botão precisa chamar prompt() da oferta");
    assert!(app.contains("\"appinstalled\""), "app.js não escuta appinstalled");
}

/// Versão nova da casca: o aviso aparece com o botão "atualizar", e a troca
/// só acontece quando o usuário toca nele. O aviso volta a cada abertura
/// enquanto a versão nova estiver esperando.
#[test]
fn versao_nova_mostra_aviso_e_so_ativa_no_toque() {
    let html = fs::read_to_string(web().join("index.html")).expect("web/index.html");
    let app = fs::read_to_string(web().join("app.js")).expect("web/app.js");
    assert!(html.contains("id=\"atualizacao\" class=\"aviso-atualizacao\" hidden"), "aviso de atualização escondido");
    assert!(html.contains("id=\"atualizar\""), "botão atualizar");
    assert!(app.contains("\"updatefound\""), "app.js não acompanha a versão nova");
    assert!(app.contains("registro.waiting"), "aviso precisa olhar a versão esperando");
    assert!(app.contains("\"SKIP_WAITING\""), "o toque precisa pedir a ativação");
    assert!(app.contains("\"controllerchange\""), "depois de trocar, a página recarrega");
}

/// Ao abrir o app instalado, pede ao navegador que não apague o orçamento
/// guardado quando faltar espaço. Numa aba comum, não pede.
#[test]
fn app_instalado_pede_armazenamento_persistente() {
    let app = fs::read_to_string(web().join("app.js")).expect("web/app.js");
    assert!(app.contains("(display-mode: standalone)"), "sem checar se o app está instalado");
    assert!(app.contains("navigator.storage.persist()"), "sem pedido de armazenamento persistente");
}

/// Lighthouse (SEO) pede uma descrição na página; a faixa zerada precisa de
/// contraste de texto (Lighthouse, acessibilidade): o texto usa a cor principal.
#[test]
fn pagina_tem_descricao_e_faixa_zerada_com_contraste() {
    let html = fs::read_to_string(web().join("index.html")).expect("web/index.html");
    assert!(html.contains("<meta name=\"description\" content=\""), "sem meta description");
    assert!(html.contains(".faixa.zero { background: var(--linha); color: var(--texto); }"), "faixa zerada com texto cinza");
}
