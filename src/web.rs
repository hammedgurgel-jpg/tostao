//! Ponte para o navegador (feature `web`): o [`Aparelho`] exposto ao
//! JavaScript com wasm-bindgen. Só repassa os comandos e converte os
//! resultados; as regras estão em [`crate::aparelho`].
//!
//! Convenções: valores de entrada são texto como na linha de comando
//! (`"80,50"`, `"-1.234,56"`); valores que voltam são inteiros em milésimos
//! (`formatarValor` devolve o texto para a tela); datas são `AAAA-MM-DD` e
//! meses são `AAAA-MM`. Plano e contas voltam como texto JSON.

use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use crate::aparelho::{
    formatar_valor, ler_data, ler_mes, ler_valor, Aparelho, Destino, Erro, LinhaDeConta, LinhaDoPlano, Plano, TipoDeConta,
};

fn erro_js(e: Erro) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// Um aparelho com o orçamento na memória do navegador.
#[wasm_bindgen]
pub struct Tostao {
    aparelho: Aparelho,
}

#[wasm_bindgen]
impl Tostao {
    /// Cria um orçamento novo, com a categoria "A atribuir".
    #[wasm_bindgen(js_name = novoOrcamento)]
    pub fn novo_orcamento(nome: &str) -> Tostao {
        Tostao { aparelho: Aparelho::novo_orcamento(nome) }
    }

    /// Prepara um aparelho para um orçamento que já está no servidor; a
    /// primeira sincronização traz tudo.
    #[wasm_bindgen(js_name = paraOrcamento)]
    pub fn para_orcamento(orcamento: &str) -> Tostao {
        Tostao { aparelho: Aparelho::para_orcamento(orcamento) }
    }

    /// Lê a cópia local gravada por [`Tostao::para_json`].
    #[wasm_bindgen(js_name = deJson)]
    pub fn de_json(texto: &str) -> Result<Tostao, JsValue> {
        Aparelho::de_json(texto).map(|aparelho| Tostao { aparelho }).map_err(erro_js)
    }

    /// A cópia local como texto JSON, para guardar no navegador.
    #[wasm_bindgen(js_name = paraJson)]
    pub fn para_json(&self) -> Result<String, JsValue> {
        self.aparelho.para_json().map_err(erro_js)
    }

    pub fn orcamento(&self) -> String {
        self.aparelho.orcamento().to_owned()
    }

    pub fn id(&self) -> String {
        self.aparelho.id().to_owned()
    }

    /// Mudanças feitas aqui que o servidor ainda não recebeu.
    #[wasm_bindgen(js_name = mudancasPendentes)]
    pub fn mudancas_pendentes(&self) -> usize {
        self.aparelho.mudancas_pendentes()
    }

    /// `tipo`: corrente, poupanca, dinheiro, cartao ou acompanhamento.
    #[wasm_bindgen(js_name = criarConta)]
    pub fn criar_conta(&mut self, nome: &str, tipo: &str) -> Result<(), JsValue> {
        let tipo = TipoDeConta::ler(tipo).ok_or_else(|| JsValue::from_str(&format!("tipo de conta desconhecido: {tipo}")))?;
        self.aparelho.criar_conta(nome, tipo).map(|_| ()).map_err(erro_js)
    }

    #[wasm_bindgen(js_name = criarGrupo)]
    pub fn criar_grupo(&mut self, nome: &str) -> Result<(), JsValue> {
        self.aparelho.criar_grupo(nome).map(|_| ()).map_err(erro_js)
    }

    #[wasm_bindgen(js_name = criarCategoria)]
    pub fn criar_categoria(&mut self, nome: &str, grupo: &str) -> Result<(), JsValue> {
        self.aparelho.criar_categoria(nome, grupo).map(|_| ()).map_err(erro_js)
    }

    /// `valor` em reais, como digitado (`"50"`, `"80,50"`); `mes` é `AAAA-MM`.
    pub fn atribuir(&mut self, categoria: &str, valor: &str, mes: &str) -> Result<(), JsValue> {
        let valor = ler_valor(valor).map_err(erro_js)?;
        let mes = ler_mes(mes).map_err(erro_js)?;
        self.aparelho.atribuir(categoria, mes, valor).map_err(erro_js)
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
        let valor = ler_valor(valor).map_err(erro_js)?;
        let data = ler_data(data).map_err(erro_js)?;
        let destino = match (categoria, transferir_para) {
            (Some(_), Some(_)) => return Err(JsValue::from_str("use categoria ou transferência, não as duas")),
            (Some(c), None) => Destino::Categoria(c),
            (None, Some(c)) => Destino::Transferencia(c),
            (None, None) => Destino::SemCategoria,
        };
        self.aparelho.lancar(conta, data, valor, &destino).map(|_| ()).map_err(erro_js)
    }

    /// O plano do mês (`AAAA-MM`), calculado aqui, como texto JSON.
    pub fn plano(&self, mes: &str) -> Result<String, JsValue> {
        let mes = ler_mes(mes).map_err(erro_js)?;
        self.aparelho.plano(mes).map(|p| plano_json(&p).to_string()).map_err(erro_js)
    }

    /// As contas e os saldos, como texto JSON.
    pub fn contas(&self) -> Result<String, JsValue> {
        self.aparelho.contas().map(|c| contas_json(&c).to_string()).map_err(erro_js)
    }
}

/// Milésimos em texto, como a tela: `1.234,56`. Recebe `f64` (e não `i64`,
/// que vira `BigInt` no JavaScript); milésimos cabem exatos nele.
#[wasm_bindgen(js_name = formatarValor)]
pub fn formatar(milesimos: f64) -> String {
    formatar_valor(milesimos as i64)
}

fn linha_json(l: &LinhaDoPlano) -> Value {
    json!({ "nome": l.nome, "atribuido": l.atribuido, "atividade": l.atividade, "disponivel": l.disponivel })
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

        let contas = contas_json(&a.contas().unwrap());
        assert_eq!(contas[0]["saldo"], json!(920_000));
    }
}
