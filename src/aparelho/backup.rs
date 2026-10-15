//! Backup do orçamento em arquivo JSON, no formato da ADR 0002
//! (`docs/adr/0002-armazenamento-e-backup.md`).
//!
//! O arquivo traz só entidades, em listas com id, com valores em inteiros de
//! milésimos. Não tem cálculos nem campos de sincronização: o plano e os
//! saldos são refeitos ao abrir. Importar valida o arquivo inteiro antes de
//! montar o orçamento novo; se algo não confere, nada é alterado.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{ler_data, ler_mes, Aparelho, CopiaLocal, Erro, TipoDeConta, CATEGORIA_A_ATRIBUIR, GRUPO_INTERNO, NOME_A_ATRIBUIR};
use crate::protocol::TOSTAO_SCHEMA_VERSION;

/// Valor do campo `formato` de todo backup.
pub const FORMATO: &str = "tostao-backup";
/// Os valores são milésimos, e o real tem duas casas.
pub const CASAS_DECIMAIS: i64 = 2;

/// Um backup, como vai para o arquivo e volta dele.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Backup {
    pub formato: String,
    pub versao_de_esquema: i64,
    /// Instante da exportação, em ISO 8601.
    pub criado_em: String,
    /// Mês que a pessoa estava vendo na exportação (`AAAA-MM`).
    pub ultimo_mes: String,
    pub casas_decimais: i64,
    pub contas: Vec<ContaNoBackup>,
    pub grupos: Vec<GrupoNoBackup>,
    pub categorias: Vec<CategoriaNoBackup>,
    pub atribuicoes: Vec<AtribuicaoNoBackup>,
    pub transacoes: Vec<TransacaoNoBackup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContaNoBackup {
    pub id: String,
    pub nome: String,
    /// `corrente`, `poupanca`, `dinheiro`, `cartao` ou `acompanhamento`.
    pub tipo: String,
    pub ordem: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrupoNoBackup {
    pub id: String,
    pub nome: String,
    pub ordem: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoriaNoBackup {
    pub id: String,
    pub nome: String,
    pub grupo_id: String,
    pub ordem: i64,
    /// Só a categoria de pagamento de um cartão tem conta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conta_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtribuicaoNoBackup {
    pub id: String,
    pub categoria_id: String,
    /// `AAAA-MM`.
    pub mes: String,
    pub valor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransacaoNoBackup {
    pub id: String,
    pub conta_id: String,
    /// `AAAA-MM-DD`.
    pub data: String,
    pub valor: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub categoria_id: Option<String>,
    /// Id da ponta espelho, quando a transação é uma transferência.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transferencia_id: Option<String>,
    /// `nao_compensada`, `compensada` ou `reconciliada`.
    pub compensacao: String,
}

/// O que a tela mostra antes de importar: a data do backup e as contagens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resumo {
    pub criado_em: String,
    pub ultimo_mes: String,
    pub contas: usize,
    pub grupos: usize,
    pub categorias: usize,
    pub atribuicoes: usize,
    pub transacoes: usize,
}

impl Backup {
    fn resumo(&self) -> Resumo {
        Resumo {
            criado_em: self.criado_em.clone(),
            ultimo_mes: self.ultimo_mes.clone(),
            contas: self.contas.len(),
            grupos: self.grupos.len(),
            categorias: self.categorias.len(),
            atribuicoes: self.atribuicoes.len(),
            transacoes: self.transacoes.len(),
        }
    }
}

/// Lê e valida um backup. Erro quando o formato, a versão ou alguma
/// referência não confere; quem chama não muda nada.
pub fn ler_backup(texto: &str) -> Result<Backup, Erro> {
    let valor: Value = serde_json::from_str(texto).map_err(|e| arquivo(format!("não é um JSON válido: {e}")))?;
    if valor.get("formato").and_then(Value::as_str) != Some(FORMATO) {
        return Err(arquivo("não é um backup do Tostão (o campo \"formato\" não confere)"));
    }
    match valor.get("versao_de_esquema").and_then(Value::as_i64) {
        Some(TOSTAO_SCHEMA_VERSION) => {}
        Some(versao) => {
            return Err(arquivo(format!(
                "backup de outra versão do esquema ({versao}); este app lê a versão {TOSTAO_SCHEMA_VERSION}"
            )))
        }
        None => return Err(arquivo("falta a versão do esquema (\"versao_de_esquema\")")),
    }
    let backup: Backup = serde_json::from_value(valor).map_err(|e| arquivo(format!("backup incompleto ou com campos inválidos: {e}")))?;
    validar(&backup)?;
    Ok(backup)
}

/// Contagens e data de um backup, para mostrar antes de importar.
pub fn resumo_do_backup(texto: &str) -> Result<Resumo, Erro> {
    ler_backup(texto).map(|b| b.resumo())
}

fn arquivo(mensagem: impl Into<String>) -> Erro {
    Erro::Arquivo(mensagem.into())
}

fn validar(b: &Backup) -> Result<(), Erro> {
    if b.casas_decimais != CASAS_DECIMAIS {
        return Err(arquivo(format!("\"casas_decimais\" é {} e este app usa {CASAS_DECIMAIS}", b.casas_decimais)));
    }
    if b.criado_em.trim().is_empty() {
        return Err(arquivo("falta a data da exportação (\"criado_em\")"));
    }
    if !mes_valido(&b.ultimo_mes) {
        return Err(arquivo(format!("\"ultimo_mes\" inválido: \"{}\" (use AAAA-MM)", b.ultimo_mes)));
    }

    let contas = ids_unicos(b.contas.iter().map(|c| c.id.as_str()), "contas")?;
    nomes_unicos(b.contas.iter().map(|c| c.nome.as_str()), "contas", &[])?;
    if let Some(c) = b.contas.iter().find(|c| TipoDeConta::ler(&c.tipo).is_none()) {
        return Err(arquivo(format!("tipo de conta desconhecido em \"{}\": \"{}\"", c.nome, c.tipo)));
    }

    let grupos = ids_unicos(b.grupos.iter().map(|g| g.id.as_str()), "grupos")?;
    if grupos.contains(GRUPO_INTERNO) {
        return Err(arquivo("um grupo usa o id reservado do grupo interno"));
    }
    nomes_unicos(b.grupos.iter().map(|g| g.nome.as_str()), "grupos", &["Interno"])?;

    let categorias = ids_unicos(b.categorias.iter().map(|c| c.id.as_str()), "categorias")?;
    if categorias.contains(CATEGORIA_A_ATRIBUIR) {
        return Err(arquivo("uma categoria usa o id reservado de \"A atribuir\""));
    }
    nomes_unicos(b.categorias.iter().map(|c| c.nome.as_str()), "categorias", &[NOME_A_ATRIBUIR])?;
    for c in &b.categorias {
        if !grupos.contains(c.grupo_id.as_str()) {
            return Err(arquivo(format!("a categoria \"{}\" está num grupo que não existe no backup", c.nome)));
        }
        if let Some(conta) = &c.conta_id {
            if !contas.contains(conta.as_str()) {
                return Err(arquivo(format!("a categoria \"{}\" aponta para uma conta que não existe no backup", c.nome)));
            }
        }
    }

    ids_unicos(b.atribuicoes.iter().map(|a| a.id.as_str()), "atribuições")?;
    let mut meses_com_atribuicao = BTreeSet::new();
    for a in &b.atribuicoes {
        if !categorias.contains(a.categoria_id.as_str()) {
            return Err(arquivo(format!("atribuição para uma categoria que não existe no backup (\"{}\")", a.categoria_id)));
        }
        if !mes_valido(&a.mes) {
            return Err(arquivo(format!("mês de atribuição inválido: \"{}\" (use AAAA-MM)", a.mes)));
        }
        if !meses_com_atribuicao.insert((a.categoria_id.as_str(), a.mes.as_str())) {
            return Err(arquivo(format!("a categoria \"{}\" tem mais de uma atribuição em {}", a.categoria_id, a.mes)));
        }
    }

    ids_unicos(b.transacoes.iter().map(|t| t.id.as_str()), "transações")?;
    let ligacoes: BTreeMap<&str, (&str, Option<&str>)> =
        b.transacoes.iter().map(|t| (t.id.as_str(), (t.conta_id.as_str(), t.transferencia_id.as_deref()))).collect();
    for t in &b.transacoes {
        if !contas.contains(t.conta_id.as_str()) {
            return Err(arquivo(format!("transação {} numa conta que não existe no backup", t.id)));
        }
        if ler_data(&t.data).is_err() {
            return Err(arquivo(format!("data inválida na transação {}: \"{}\" (use AAAA-MM-DD)", t.id, t.data)));
        }
        if let Some(c) = &t.categoria_id {
            if c != CATEGORIA_A_ATRIBUIR && !categorias.contains(c.as_str()) {
                return Err(arquivo(format!("transação {} numa categoria que não existe no backup", t.id)));
            }
        }
        if !matches!(t.compensacao.as_str(), "nao_compensada" | "compensada" | "reconciliada") {
            return Err(arquivo(format!("situação de compensação desconhecida na transação {}", t.id)));
        }
        if let Some(espelho) = &t.transferencia_id {
            // A ponta espelho existe, é de outra conta e aponta de volta.
            let ligada = matches!(
                ligacoes.get(espelho.as_str()),
                Some((conta_espelho, Some(volta))) if *volta == t.id.as_str() && *conta_espelho != t.conta_id.as_str()
            );
            if !ligada {
                return Err(arquivo(format!("transferência {} sem a ponta espelho certa", t.id)));
            }
        }
    }
    Ok(())
}

/// Os ids de uma lista, que não podem se repetir.
fn ids_unicos<'a>(ids: impl Iterator<Item = &'a str>, lista: &str) -> Result<BTreeSet<&'a str>, Erro> {
    let mut vistos = BTreeSet::new();
    for id in ids {
        if !vistos.insert(id) {
            return Err(arquivo(format!("id repetido em {lista}: \"{id}\"")));
        }
    }
    Ok(vistos)
}

/// Nomes que não podem ficar vazios, repetir (sem diferença de maiúsculas)
/// nem usar um nome reservado do app.
fn nomes_unicos<'a>(nomes: impl Iterator<Item = &'a str>, lista: &str, reservados: &[&str]) -> Result<(), Erro> {
    let mut vistos: BTreeSet<String> = reservados.iter().map(|n| n.to_lowercase()).collect();
    for nome in nomes {
        if nome.trim().is_empty() {
            return Err(arquivo(format!("há um nome vazio em {lista}")));
        }
        if !vistos.insert(nome.to_lowercase()) {
            return Err(arquivo(format!("nome repetido ou reservado em {lista}: \"{nome}\"")));
        }
    }
    Ok(())
}

fn mes_valido(texto: &str) -> bool {
    ler_mes(texto).map(|m| m.format("%Y-%m").to_string() == texto).unwrap_or(false)
}

/// Tipo de conta do app para o nome do backup.
fn tipo_no_backup(account_type: &str) -> Option<&'static str> {
    Some(match account_type {
        "Checking" => "corrente",
        "Savings" => "poupanca",
        "Cash" => "dinheiro",
        "CreditCard" => "cartao",
        "OtherAsset" => "acompanhamento",
        _ => return None,
    })
}

fn compensacao_no_backup(cleared: &str) -> &'static str {
    match cleared {
        "Cleared" => "compensada",
        "Reconciled" => "reconciliada",
        _ => "nao_compensada",
    }
}

fn compensacao_do_backup(compensacao: &str) -> &'static str {
    match compensacao {
        "compensada" => "Cleared",
        "reconciliada" => "Reconciled",
        _ => "Uncleared",
    }
}

fn texto(v: &Value, campo: &str) -> String {
    v[campo].as_str().unwrap_or_default().to_owned()
}

fn texto_opcional(v: &Value, campo: &str) -> Option<String> {
    v[campo].as_str().map(str::to_owned)
}

impl Aparelho {
    /// O orçamento como backup (JSON). `criado_em` é o instante da exportação
    /// (quem chama lê o relógio) e `ultimo_mes` o mês que a pessoa estava vendo.
    pub fn exportar_backup(&self, criado_em: &str, ultimo_mes: &str) -> Result<String, Erro> {
        let backup = Backup {
            formato: FORMATO.to_owned(),
            versao_de_esquema: TOSTAO_SCHEMA_VERSION,
            criado_em: criado_em.to_owned(),
            ultimo_mes: ler_mes(ultimo_mes)?.format("%Y-%m").to_string(),
            casas_decimais: CASAS_DECIMAIS,
            contas: self.contas_no_backup(),
            grupos: self.grupos_no_backup(),
            categorias: self.categorias_no_backup(),
            atribuicoes: self.atribuicoes_no_backup(),
            transacoes: self.transacoes_no_backup(),
        };
        serde_json::to_string_pretty(&backup).map_err(|e| arquivo(e.to_string()))
    }

    /// Troca o orçamento pelo do backup. Valida tudo antes: quando devolve
    /// erro, `self` continua como estava. O orçamento e o id do aparelho
    /// ficam, e o nome do orçamento também.
    pub fn importar_backup(&self, texto: &str) -> Result<Aparelho, Erro> {
        let backup = ler_backup(texto)?;
        let nome = self.vivas("be_budget").find_map(|o| o["budget_name"].as_str()).unwrap_or("Família").to_owned();
        let mut novo = Aparelho::de_copia(CopiaLocal {
            orcamento: self.copia.orcamento.clone(),
            aparelho: self.copia.aparelho.clone(),
            conhecimento: self.copia.conhecimento,
            ..Default::default()
        });
        novo.semear_base(&nome);

        for c in &backup.contas {
            let tipo = TipoDeConta::ler(&c.tipo).ok_or_else(|| arquivo(format!("tipo de conta desconhecido: \"{}\"", c.tipo)))?;
            novo.mudar(
                "be_accounts",
                json!({
                    "id": c.id, "account_name": c.nome, "account_type": tipo.account_type(),
                    "on_budget": tipo != TipoDeConta::Acompanhamento, "sortable_index": c.ordem, "is_tombstone": false
                }),
            );
        }
        for g in &backup.grupos {
            novo.mudar("be_master_categories", json!({ "id": g.id, "name": g.nome, "sortable_index": g.ordem, "is_tombstone": false }));
        }
        for c in &backup.categorias {
            let mut entidade = json!({
                "id": c.id, "entities_master_category_id": c.grupo_id, "name": c.nome, "sortable_index": c.ordem, "is_tombstone": false
            });
            if let Some(conta) = &c.conta_id {
                entidade["entities_account_id"] = json!(conta);
            }
            novo.mudar("be_subcategories", entidade);
        }
        for a in &backup.atribuicoes {
            // O id da atribuição é o que o motor procura: o do arquivo não entra.
            novo.gravar_atribuicao(&a.categoria_id, ler_mes(&a.mes)?, a.valor);
        }
        let conta_de: BTreeMap<&str, &str> = backup.transacoes.iter().map(|t| (t.id.as_str(), t.conta_id.as_str())).collect();
        for t in &backup.transacoes {
            let mut entidade = json!({
                "id": t.id, "entities_account_id": t.conta_id, "date": ler_data(&t.data)?.to_string(), "amount": t.valor,
                "cleared": compensacao_do_backup(&t.compensacao), "accepted": true, "is_tombstone": false
            });
            if let Some(categoria) = &t.categoria_id {
                entidade["entities_subcategory_id"] = json!(categoria);
            }
            if let Some(espelho) = &t.transferencia_id {
                entidade["transfer_account_id"] = json!(conta_de.get(espelho.as_str()).copied().unwrap_or_default());
                entidade["transfer_transaction_id"] = json!(espelho);
            }
            novo.mudar("be_transactions", entidade);
        }
        Ok(novo)
    }

    fn contas_no_backup(&self) -> Vec<ContaNoBackup> {
        let mut contas: Vec<ContaNoBackup> = self
            .vivas("be_accounts")
            .filter_map(|c| {
                Some(ContaNoBackup {
                    id: texto(c, "id"),
                    nome: texto(c, "account_name"),
                    tipo: tipo_no_backup(c["account_type"].as_str()?)?.to_owned(),
                    ordem: c["sortable_index"].as_i64().unwrap_or(0),
                })
            })
            .collect();
        contas.sort_by_key(|c| c.ordem);
        contas
    }

    fn grupos_no_backup(&self) -> Vec<GrupoNoBackup> {
        let mut grupos: Vec<GrupoNoBackup> = self
            .vivas("be_master_categories")
            .filter(|g| g["id"] != GRUPO_INTERNO)
            .map(|g| GrupoNoBackup { id: texto(g, "id"), nome: texto(g, "name"), ordem: g["sortable_index"].as_i64().unwrap_or(0) })
            .collect();
        grupos.sort_by_key(|g| g.ordem);
        grupos
    }

    fn categorias_no_backup(&self) -> Vec<CategoriaNoBackup> {
        let mut categorias: Vec<CategoriaNoBackup> = self
            .vivas("be_subcategories")
            .filter(|c| c["id"] != CATEGORIA_A_ATRIBUIR)
            .map(|c| CategoriaNoBackup {
                id: texto(c, "id"),
                nome: texto(c, "name"),
                grupo_id: texto(c, "entities_master_category_id"),
                ordem: c["sortable_index"].as_i64().unwrap_or(0),
                conta_id: texto_opcional(c, "entities_account_id"),
            })
            .collect();
        categorias.sort_by_key(|c| c.ordem);
        categorias
    }

    fn atribuicoes_no_backup(&self) -> Vec<AtribuicaoNoBackup> {
        let mut atribuicoes: Vec<AtribuicaoNoBackup> = self
            .vivas("be_monthly_subcategory_budgets")
            .map(|a| AtribuicaoNoBackup {
                id: texto(a, "id"),
                categoria_id: texto(a, "entities_subcategory_id"),
                mes: ler_mes(&texto(a, "month")).map(|m| m.format("%Y-%m").to_string()).unwrap_or_default(),
                valor: a["budgeted"].as_i64().unwrap_or(0),
            })
            .collect();
        atribuicoes.sort_by(|x, y| (&x.mes, &x.categoria_id).cmp(&(&y.mes, &y.categoria_id)));
        atribuicoes
    }

    fn transacoes_no_backup(&self) -> Vec<TransacaoNoBackup> {
        let mut transacoes: Vec<TransacaoNoBackup> = self
            .vivas("be_transactions")
            .map(|t| TransacaoNoBackup {
                id: texto(t, "id"),
                conta_id: texto(t, "entities_account_id"),
                data: texto(t, "date"),
                valor: t["amount"].as_i64().unwrap_or(0),
                categoria_id: texto_opcional(t, "entities_subcategory_id"),
                transferencia_id: texto_opcional(t, "transfer_transaction_id"),
                compensacao: compensacao_no_backup(t["cleared"].as_str().unwrap_or_default()).to_owned(),
            })
            .collect();
        transacoes.sort_by(|x, y| (&x.data, &x.id).cmp(&(&y.data, &y.id)));
        transacoes
    }
}
