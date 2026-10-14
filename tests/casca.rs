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

/// `cache.addAll` é atômico: um arquivo que falta (o `pkg/`, por exemplo) não
/// deixa guardar nem a página. A instalação guarda um arquivo de cada vez.
#[test]
fn instalacao_do_service_worker_nao_desiste_por_um_arquivo() {
    let codigo = fs::read_to_string(web().join("sw.js")).expect("web/sw.js");
    assert!(!codigo.contains("addAll"), "addAll é atômico: um arquivo faltando derruba a casca inteira");
}
