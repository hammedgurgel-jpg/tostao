//! Ponte para o navegador (feature `web`): o [`Aparelho`] exposto ao
//! JavaScript com wasm-bindgen. Só repassa os comandos e converte os
//! resultados; as regras estão em [`crate::aparelho`].
//!
//! Convenções: valores de entrada são texto como na linha de comando
//! (`"80,50"`, `"-1.234,56"`); valores que voltam são inteiros em milésimos
//! (`formatarValor` devolve o texto para a tela); datas são `AAAA-MM-DD` e
//! meses são `AAAA-MM`. Plano e contas voltam como texto JSON.
//!
//! A sincronização ([`Tostao::sincronizar`]) fala com o servidor pelo `fetch`
//! do navegador ([`FetchTransport`]). O token vem a cada chamada e não é
//! guardado: nem no aparelho, nem na cópia que a página grava.

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::{Error, Promise};
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{future_to_promise, JsFuture};
use web_sys::{Headers, Request, RequestInit, Response};

use crate::api::{erro_do_servidor, ApiError, HttpResponse, Transport};
use crate::aparelho::backup::resumo_do_backup;
use crate::aparelho::{
    formatar_valor, ler_data, ler_mes, ler_valor, Aparelho, Destino, Erro, Estouro, LinhaDeConta, LinhaDoPlano, Plano,
    TipoDeConta,
};
use crate::protocol::{CatalogRequest, DEVICE_ID_HEADER, SERVER_VERSION_HEADER};

fn erro_js(e: Erro) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// Erro de quem chama enquanto uma sincronização tem o aparelho emprestado.
const OCUPADO: &str = "o aparelho está sincronizando; espere terminar";

/// Um aparelho com o orçamento na memória do navegador.
#[wasm_bindgen]
pub struct Tostao {
    /// Vazio só enquanto uma sincronização tem o aparelho (ver
    /// [`Tostao::sincronizar`]). O `Rc` deixa a sincronização devolvê-lo
    /// depois do `await`, sem segurar o `RefCell` durante a espera.
    aparelho: Rc<RefCell<Option<Aparelho>>>,
}

impl Tostao {
    fn novo(aparelho: Aparelho) -> Tostao {
        Tostao { aparelho: Rc::new(RefCell::new(Some(aparelho))) }
    }

    fn ler<R>(&self, fazer: impl FnOnce(&Aparelho) -> Result<R, Erro>) -> Result<R, JsValue> {
        let guarda = self.aparelho.borrow();
        let aparelho = guarda.as_ref().ok_or_else(|| JsValue::from_str(OCUPADO))?;
        fazer(aparelho).map_err(erro_js)
    }

    fn mudar<R>(&mut self, fazer: impl FnOnce(&mut Aparelho) -> Result<R, Erro>) -> Result<R, JsValue> {
        let mut guarda = self.aparelho.borrow_mut();
        let aparelho = guarda.as_mut().ok_or_else(|| JsValue::from_str(OCUPADO))?;
        fazer(aparelho).map_err(erro_js)
    }

    /// [`Tostao::importar`] com o erro do núcleo, para os testes (criar um
    /// `JsValue` fora do navegador não funciona).
    fn trocar_por_backup(&mut self, texto: &str) -> Result<(), Erro> {
        let mut guarda = self.aparelho.borrow_mut();
        let aparelho = guarda.as_mut().ok_or_else(|| Erro::Valor(OCUPADO.into()))?;
        let novo = aparelho.importar_backup(texto)?;
        *aparelho = novo;
        Ok(())
    }
}

/// Data e contagens de um backup, como texto JSON, para confirmar antes de
/// importar: `{ criado_em, ultimo_mes, contagens: { contas, grupos, … } }`.
#[wasm_bindgen(js_name = resumoDoBackup)]
pub fn resumo_do_backup_js(texto: &str) -> Result<String, JsValue> {
    let r = resumo_do_backup(texto).map_err(erro_js)?;
    Ok(json!({
        "criado_em": r.criado_em,
        "ultimo_mes": r.ultimo_mes,
        "contagens": {
            "contas": r.contas, "grupos": r.grupos, "categorias": r.categorias,
            "atribuicoes": r.atribuicoes, "transacoes": r.transacoes,
        },
    })
    .to_string())
}

#[wasm_bindgen]
impl Tostao {
    /// Cria um orçamento novo, com a categoria "A atribuir".
    #[wasm_bindgen(js_name = novoOrcamento)]
    pub fn novo_orcamento(nome: &str) -> Tostao {
        Tostao::novo(Aparelho::novo_orcamento(nome))
    }

    /// Prepara um aparelho para um orçamento que já está no servidor; a
    /// primeira sincronização traz tudo.
    #[wasm_bindgen(js_name = paraOrcamento)]
    pub fn para_orcamento(orcamento: &str) -> Tostao {
        Tostao::novo(Aparelho::para_orcamento(orcamento))
    }

    /// Lê a cópia local gravada por [`Tostao::para_json`].
    #[wasm_bindgen(js_name = deJson)]
    pub fn de_json(texto: &str) -> Result<Tostao, JsValue> {
        Aparelho::de_json(texto).map(Tostao::novo).map_err(erro_js)
    }

    /// A cópia local como texto JSON, para guardar no navegador.
    #[wasm_bindgen(js_name = paraJson)]
    pub fn para_json(&self) -> Result<String, JsValue> {
        self.ler(|a| a.para_json())
    }

    pub fn orcamento(&self) -> Result<String, JsValue> {
        self.ler(|a| Ok(a.orcamento().to_owned()))
    }

    pub fn id(&self) -> Result<String, JsValue> {
        self.ler(|a| Ok(a.id().to_owned()))
    }

    /// Mudanças feitas aqui que o servidor ainda não recebeu.
    #[wasm_bindgen(js_name = mudancasPendentes)]
    pub fn mudancas_pendentes(&self) -> Result<usize, JsValue> {
        self.ler(|a| Ok(a.mudancas_pendentes()))
    }

    /// `tipo`: corrente, poupanca, dinheiro, cartao ou acompanhamento.
    #[wasm_bindgen(js_name = criarConta)]
    pub fn criar_conta(&mut self, nome: &str, tipo: &str) -> Result<(), JsValue> {
        self.mudar(|a| {
            let tipo = TipoDeConta::ler(tipo).ok_or_else(|| Erro::Valor(format!("tipo de conta desconhecido: {tipo}")))?;
            a.criar_conta(nome, tipo).map(|_| ())
        })
    }

    #[wasm_bindgen(js_name = criarGrupo)]
    pub fn criar_grupo(&mut self, nome: &str) -> Result<(), JsValue> {
        self.mudar(|a| a.criar_grupo(nome).map(|_| ()))
    }

    #[wasm_bindgen(js_name = criarCategoria)]
    pub fn criar_categoria(&mut self, nome: &str, grupo: &str) -> Result<(), JsValue> {
        self.mudar(|a| a.criar_categoria(nome, grupo).map(|_| ()))
    }

    /// `valor` em reais, como digitado (`"50"`, `"80,50"`); `mes` é `AAAA-MM`.
    pub fn atribuir(&mut self, categoria: &str, valor: &str, mes: &str) -> Result<(), JsValue> {
        self.mudar(|a| {
            let valor = ler_valor(valor)?;
            let mes = ler_mes(mes)?;
            a.atribuir(categoria, mes, valor)
        })
    }

    /// Movimenta dinheiro de uma categoria para outra no mês (`"A atribuir"`
    /// é a origem livre). `valor` em reais, como digitado; pode ser negativo.
    pub fn movimentar(&mut self, origem: &str, destino: &str, mes: &str, valor: &str) -> Result<(), JsValue> {
        self.mudar(|a| {
            let valor = ler_valor(valor)?;
            let mes = ler_mes(mes)?;
            a.movimentar(origem, destino, mes, valor)
        })
    }

    /// Lança uma transação. `valor` em reais, como digitado (saída é
    /// negativa). Sem `categoria` nem `transferir_para`, fica sem categoria.
    pub fn lancar(
        &mut self,
        conta: &str,
        valor: &str,
        data: &str,
        categoria: Option<String>,
        transferir_para: Option<String>,
    ) -> Result<(), JsValue> {
        self.mudar(|a| {
            let valor = ler_valor(valor)?;
            let data = ler_data(data)?;
            let destino = match (categoria, transferir_para) {
                (Some(_), Some(_)) => return Err(Erro::Valor("use categoria ou transferência, não as duas".into())),
                (Some(c), None) => Destino::Categoria(c),
                (None, Some(c)) => Destino::Transferencia(c),
                (None, None) => Destino::SemCategoria,
            };
            a.lancar(conta, data, valor, &destino).map(|_| ())
        })
    }

    /// O orçamento como arquivo de backup (texto JSON). `criado_em` é o
    /// instante da exportação (ISO 8601); `ultimo_mes` é o mês que a tela mostra.
    pub fn exportar(&self, criado_em: &str, ultimo_mes: &str) -> Result<String, JsValue> {
        self.ler(|a| a.exportar_backup(criado_em, ultimo_mes))
    }

    /// Substitui o orçamento inteiro pelo do backup. Um arquivo recusado não
    /// toca no orçamento guardado. Quem chama confirma antes, com o
    /// [`resumoDoBackup`].
    pub fn importar(&mut self, texto: &str) -> Result<(), JsValue> {
        self.trocar_por_backup(texto).map_err(erro_js)
    }

    /// O plano do mês (`AAAA-MM`), calculado aqui, como texto JSON.
    pub fn plano(&self, mes: &str) -> Result<String, JsValue> {
        self.ler(|a| {
            let mes = ler_mes(mes)?;
            a.plano(mes).map(|p| plano_json(&p).to_string())
        })
    }

    /// As contas e os saldos, como texto JSON.
    pub fn contas(&self) -> Result<String, JsValue> {
        self.ler(|a| a.contas().map(|c| contas_json(&c).to_string()))
    }

    /// Sincroniza com o servidor pelo `fetch` do navegador. `servidor` é a
    /// origem (ex.: `http://127.0.0.1:8080`); `token` é o que a pessoa
    /// digitou e vale só para esta chamada. Devolve uma Promise; até ela
    /// resolver, as outras chamadas dão erro de "sincronizando".
    pub fn sincronizar(&mut self, servidor: &str, token: &str) -> Result<Promise, JsValue> {
        let mut aparelho = self.aparelho.borrow_mut().take().ok_or_else(|| JsValue::from_str(OCUPADO))?;
        let transporte = FetchTransport::new(servidor, token, aparelho.id());
        let token = token.to_owned();
        let guarda = Rc::clone(&self.aparelho);
        Ok(future_to_promise(async move {
            let resultado = aparelho.sincronizar(transporte, &token).await;
            // O aparelho volta mesmo quando a sincronização falha.
            *guarda.borrow_mut() = Some(aparelho);
            resultado.map(|_| JsValue::UNDEFINED).map_err(erro_sincronizacao)
        }))
    }
}

/// Erro da sincronização como `{ tipo, mensagem }`: `tipo` é `"rede"` quando o
/// servidor não foi alcançado, e `"outro"` nos demais casos.
fn erro_sincronizacao(e: Erro) -> JsValue {
    let objeto = js_sys::Object::new();
    let tipo = if e.e_falha_de_rede() { "rede" } else { "outro" };
    let _ = js_sys::Reflect::set(&objeto, &"tipo".into(), &JsValue::from_str(tipo));
    let _ = js_sys::Reflect::set(&objeto, &"mensagem".into(), &JsValue::from_str(&e.to_string()));
    objeto.into()
}

/// [`Transport`] pelo `fetch` do navegador. O token e o id do aparelho vão
/// só nos cabeçalhos desta requisição.
struct FetchTransport {
    servidor: String,
    token: String,
    device_id: String,
}

impl FetchTransport {
    fn new(servidor: &str, token: &str, device_id: &str) -> Self {
        Self {
            servidor: servidor.trim_end_matches('/').to_owned(),
            token: token.to_owned(),
            device_id: device_id.to_owned(),
        }
    }

    /// Manda o `POST` e espera a resposta inteira (status e corpo).
    async fn enviar(&self, path: &str, corpo: &str) -> Result<(Response, String), JsValue> {
        let cabecalhos = Headers::new()?;
        cabecalhos.set("Authorization", &format!("Bearer {}", self.token))?;
        cabecalhos.set(DEVICE_ID_HEADER, &self.device_id)?;
        cabecalhos.set("Content-Type", "application/json")?;
        let init = RequestInit::new();
        init.set_method("POST");
        init.set_headers(&cabecalhos);
        init.set_body(&JsValue::from_str(corpo));
        let pedido = Request::new_with_str_and_init(&format!("{}{path}", self.servidor), &init)?;
        let janela = web_sys::window().ok_or_else(|| JsValue::from_str("sem janela no navegador"))?;
        let resposta: Response = JsFuture::from(janela.fetch_with_request(&pedido)).await?.dyn_into()?;
        let texto = JsFuture::from(resposta.text()?).await?.as_string().unwrap_or_default();
        Ok((resposta, texto))
    }
}

impl Transport for FetchTransport {
    async fn post(&mut self, path: &str, body: &CatalogRequest) -> Result<HttpResponse, ApiError> {
        let corpo = serde_json::to_string(body).map_err(|e| ApiError::Protocolo(e.to_string()))?;
        let (resposta, texto) = self.enviar(path, &corpo).await.map_err(|e| ApiError::Transport(mensagem_js(&e)))?;
        if !resposta.ok() {
            return Err(ApiError::Server(erro_do_servidor(resposta.status(), &texto)));
        }
        let versao = resposta.headers().get(SERVER_VERSION_HEADER).ok().flatten();
        let data: Value = serde_json::from_str(&texto).map_err(|e| ApiError::Protocolo(e.to_string()))?;
        Ok(HttpResponse { headers: versao.map(|v| (SERVER_VERSION_HEADER.to_owned(), v)).into_iter().collect(), data })
    }
}

/// O texto de um erro do JavaScript (um `TypeError` do `fetch`, por exemplo).
fn mensagem_js(e: &JsValue) -> String {
    e.dyn_ref::<Error>().map(|erro| String::from(erro.message())).or_else(|| e.as_string()).unwrap_or_else(|| format!("{e:?}"))
}

/// Milésimos em texto, como a tela: `1.234,56`. Recebe `f64` (e não `i64`,
/// que vira `BigInt` no JavaScript); milésimos cabem exatos nele.
#[wasm_bindgen(js_name = formatarValor)]
pub fn formatar(milesimos: f64) -> String {
    formatar_valor(milesimos as i64)
}

/// Uma linha do plano. `estouro` é `"dinheiro"`, `"credito"` ou `null`;
/// `falta` é a falta da fatura (só na categoria de pagamento do cartão).
fn linha_json(l: &LinhaDoPlano) -> Value {
    let estouro = match l.estouro {
        Some(Estouro::Dinheiro) => json!("dinheiro"),
        Some(Estouro::Credito) => json!("credito"),
        None => Value::Null,
    };
    json!({
        "nome": l.nome,
        "atribuido": l.atribuido,
        "atividade": l.atividade,
        "disponivel": l.disponivel,
        "estouro": estouro,
        "falta": l.falta,
        "pagamento": l.pagamento,
    })
}

fn plano_json(p: &Plano) -> Value {
    json!({
        "mes": p.mes.format("%Y-%m").to_string(),
        "a_atribuir": p.a_atribuir,
        "grupos": p.grupos.iter().map(|g| json!({
            "nome": g.nome,
            "categorias": g.categorias.iter().map(linha_json).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "sem_categoria": p.sem_categoria.as_ref().map(linha_json),
    })
}

fn contas_json(contas: &[LinhaDeConta]) -> Value {
    Value::Array(
        contas
            .iter()
            .map(|c| {
                json!({
                    "nome": c.nome,
                    "orcamentaria": c.orcamentaria,
                    "saldo": c.saldo,
                    "saldo_compensado": c.saldo_compensado,
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::dates::Date;

    /// Roteiro 1 da análise 01 (seção 2), pela ponte: os números batem com
    /// `tests/aparelho.rs`.
    #[test]
    fn roteiro_1_pela_ponte() {
        let mut a = Aparelho::novo_orcamento("Família");
        a.criar_conta("Corrente", TipoDeConta::Corrente).unwrap();
        a.criar_grupo("Contas").unwrap();
        a.criar_categoria("Mercado", "Contas").unwrap();
        let dia = Date::from_ymd_opt(2026, 10, 13).unwrap();
        let mes = Date::from_ymd_opt(2026, 10, 1).unwrap();
        a.lancar("Corrente", dia, 1_000_000, &Destino::Categoria("A atribuir".into())).unwrap();
        a.atribuir("Mercado", mes, 50_000).unwrap();
        a.lancar("Corrente", dia, -80_000, &Destino::Categoria("Mercado".into())).unwrap();

        let plano = plano_json(&a.plano(mes).unwrap());
        assert_eq!(plano["a_atribuir"], json!(950_000));
        let mercado = &plano["grupos"][0]["categorias"][0];
        assert_eq!(mercado["nome"], "Mercado");
        assert_eq!(mercado["atribuido"], json!(50_000));
        assert_eq!(mercado["atividade"], json!(-80_000));
        assert_eq!(mercado["disponivel"], json!(-30_000));
        assert_eq!(mercado["estouro"], "dinheiro", "estouro em dinheiro, pronto para a tela");
        assert_eq!(mercado["falta"], json!(0));
        assert_eq!(mercado["pagamento"], json!(false));

        let contas = contas_json(&a.contas().unwrap());
        assert_eq!(contas[0]["saldo"], json!(920_000));
    }

    /// Um orçamento pequeno pela ponte, com um lançamento em cada categoria.
    fn orcamento_pela_ponte() -> Tostao {
        let mut t = Tostao::novo_orcamento("Família");
        t.criar_conta("Corrente", "corrente").unwrap();
        t.criar_grupo("Contas").unwrap();
        t.criar_categoria("Mercado", "Contas").unwrap();
        t.lancar("Corrente", "1000", "2026-10-13", Some("A atribuir".into()), None).unwrap();
        t.atribuir("Mercado", "50", "2026-10").unwrap();
        t.lancar("Corrente", "-80", "2026-10-13", Some("Mercado".into()), None).unwrap();
        t
    }

    /// Exportar, depois importar num orçamento vazio, devolve o mesmo plano e
    /// os mesmos saldos; o resumo traz a data e as contagens.
    #[test]
    fn backup_ida_e_volta_pela_ponte() {
        let original = orcamento_pela_ponte();
        let arquivo = original.exportar("2026-10-15T12:00:00.000Z", "2026-10").unwrap();

        let resumo: Value = serde_json::from_str(&resumo_do_backup_js(&arquivo).unwrap()).unwrap();
        assert_eq!(resumo["criado_em"], "2026-10-15T12:00:00.000Z");
        assert_eq!(resumo["ultimo_mes"], "2026-10");
        assert_eq!(resumo["contagens"]["contas"], json!(1));
        assert_eq!(resumo["contagens"]["transacoes"], json!(2));

        let mut limpo = Tostao::novo_orcamento("Família");
        limpo.importar(&arquivo).unwrap();
        assert_eq!(limpo.plano("2026-10").unwrap(), original.plano("2026-10").unwrap());
        assert_eq!(limpo.plano("2026-11").unwrap(), original.plano("2026-11").unwrap());
        assert_eq!(limpo.contas().unwrap(), original.contas().unwrap());
    }

    /// Recusas pela ponte: nada muda no orçamento guardado.
    #[test]
    fn backup_recusado_pela_ponte_nao_toca_no_orcamento() {
        let arquivo = orcamento_pela_ponte().exportar("2026-10-15T12:00:00.000Z", "2026-10").unwrap();
        let mut t = orcamento_pela_ponte();
        let antes = (t.plano("2026-10").unwrap(), t.contas().unwrap());

        for ruim in [
            "{ quebrado".to_string(),
            arquivo.replace("\"formato\": \"tostao-backup\"", "\"formato\": \"outro\""),
            arquivo.replace("\"versao_de_esquema\": 1", "\"versao_de_esquema\": 2"),
            arquivo.replace("\"casas_decimais\": 2", "\"casas_decimais\": 3"),
        ] {
            assert!(t.trocar_por_backup(&ruim).is_err(), "aceitou: {ruim:.80}");
            assert_eq!((t.plano("2026-10").unwrap(), t.contas().unwrap()), antes);
        }
    }
}
