//! Aparelho de linha de comando: a cópia local do orçamento num arquivo
//! JSON, as mudanças que a família faz nela e a sincronização com o
//! servidor pelo [`BudgetSync`].
//!
//! Cada mudança local sobe o conhecimento do aparelho e marca a entidade com
//! esse valor; na sincronização vão as entidades marcadas depois do que o
//! servidor já sabe do aparelho. O que vem do servidor substitui a cópia
//! local.
//!
//! O aparelho calcula sozinho: [`Aparelho::plano`] e [`Aparelho::contas`]
//! montam o [`Orcamento`](motor::Orcamento) com as entidades vivas da cópia
//! local, inclusive as mudanças ainda não enviadas, e chamam
//! [`motor::calcular`]. Assim tudo, menos o `sincronizar`, funciona sem
//! servidor. Os cálculos que o servidor manda (`be_*_calculations`) continuam
//! sendo recebidos e guardados na cópia local, mas não são usados para
//! mostrar nada.
//!
//! Escolhas daqui, **não conferidas** com o YNAB: os tipos de conta
//! (`Checking`, `Savings`, `Cash`, `CreditCard`, `OtherAsset`), os ids fixos
//! do grupo interno, da categoria de receita e do grupo dos pagamentos de
//! cartão, e o campo `month` direto na atribuição.

use std::collections::BTreeMap;
#[cfg(feature = "arquivo")]
use std::path::Path;
#[cfg(feature = "arquivo")]
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::api::{ApiError, ApiV1, ServerError, Transport};
use crate::dates::{add_months, start_of_month, Date};
use crate::knowledge::BudgetKnowledge;
use crate::calc::SubCategoryMonthCalc;
use crate::motor::protocolo::{orcamento_das_entidades, CATEGORIA_RECEITA};
use crate::motor::{self, Calculos};
use crate::protocol::{
    IncomingChangedEntities, OutgoingChangedEntities, SyncBudgetDataRequest, SyncBudgetDataResponse, SyncType, TOSTAO_SCHEMA_VERSION,
};
use crate::sync::{BudgetHost, BudgetSync, HandledResponse, SyncError, SyncResult};

/// Grupo das categorias internas (a de receita), que não aparece no plano.
pub const GRUPO_INTERNO: &str = "grupo-interno";
/// Categoria de receita, que a tela chama de "A atribuir".
pub const CATEGORIA_A_ATRIBUIR: &str = "categoria-a-atribuir";
pub const NOME_A_ATRIBUIR: &str = "A atribuir";
/// Grupo das categorias de pagamento do cartão.
pub const GRUPO_PAGAMENTOS: &str = "grupo-pagamentos-cartao";

pub mod backup;

#[derive(Debug)]
pub enum Erro {
    Arquivo(String),
    /// Nome que não existe, ou que existe mais de uma vez.
    Nome(String),
    Valor(String),
    Sincronizacao(SyncError),
    PermissaoNegada(ServerError),
}

impl std::fmt::Display for Erro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Erro::Arquivo(m) | Erro::Nome(m) | Erro::Valor(m) => f.write_str(m),
            Erro::Sincronizacao(SyncError::Api(ApiError::Server(e))) if e.status_code == 401 => {
                f.write_str("o servidor recusou o token (401)")
            }
            Erro::Sincronizacao(SyncError::Api(ApiError::Server(e))) => {
                write!(f, "o servidor respondeu {} {}", e.status_code, e.id)?;
                match &e.data {
                    Some(data) => write!(f, ": {data}"),
                    None => Ok(()),
                }
            }
            Erro::Sincronizacao(SyncError::Api(ApiError::Transport(m))) => {
                write!(f, "não deu para falar com o servidor: {m}")
            }
            Erro::Sincronizacao(SyncError::Api(ApiError::Protocolo(m))) => {
                write!(f, "o servidor respondeu algo que não é o protocolo esperado: {m}")
            }
            Erro::Sincronizacao(SyncError::UnexpectedSchemaVersion { expected, received }) => write!(
                f,
                "o servidor usa a versão de esquema {received} e este aparelho a {expected}; atualize o que estiver atrás"
            ),
            Erro::Sincronizacao(e) => write!(f, "falha ao sincronizar: {e:?}"),
            Erro::PermissaoNegada(e) => write!(f, "o servidor negou acesso ao orçamento: {}", e.id),
        }
    }
}

impl Erro {
    /// `true` quando a sincronização não alcançou o servidor (sem rede,
    /// endereço errado, servidor fora do ar). Quem chama usa isto, e não o
    /// texto da mensagem, para decidir o aviso da tela.
    pub fn e_falha_de_rede(&self) -> bool {
        matches!(self, Erro::Sincronizacao(SyncError::Api(ApiError::Transport(_))))
    }
}

impl std::error::Error for Erro {}

/// Tipo de conta, como na conversa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoDeConta {
    Corrente,
    Poupanca,
    Dinheiro,
    Cartao,
    /// Conta de acompanhamento: fora do orçamento.
    Acompanhamento,
}

impl TipoDeConta {
    pub fn ler(texto: &str) -> Option<Self> {
        Some(match texto {
            "corrente" => Self::Corrente,
            "poupanca" | "poupança" => Self::Poupanca,
            "dinheiro" => Self::Dinheiro,
            "cartao" | "cartão" => Self::Cartao,
            "acompanhamento" => Self::Acompanhamento,
            _ => return None,
        })
    }

    fn account_type(self) -> &'static str {
        match self {
            Self::Corrente => "Checking",
            Self::Poupanca => "Savings",
            Self::Dinheiro => "Cash",
            Self::Cartao => "CreditCard",
            Self::Acompanhamento => "OtherAsset",
        }
    }
}

/// Para onde vai uma transação.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destino {
    /// Pelo nome; "A atribuir" é receita do mês.
    Categoria(String),
    SemCategoria,
    /// Transferência para a conta com este nome: cria as duas pontas.
    Transferencia(String),
}

/// Uma entidade na cópia local.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Guardada {
    valor: Value,
    /// Conhecimento do aparelho em que mudou aqui; 0 = veio do servidor.
    conhecimento_local: i64,
}

/// Espelho serializável de [`BudgetKnowledge`].
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct Conhecimento {
    do_aparelho: i64,
    servidor_sabe_do_aparelho: i64,
    aparelho_sabe_do_servidor: i64,
    versao_de_esquema: i64,
    refazer_calculos: i64,
}

impl From<Conhecimento> for BudgetKnowledge {
    fn from(c: Conhecimento) -> Self {
        BudgetKnowledge {
            current_device_knowledge: c.do_aparelho,
            server_knowledge_of_device: c.servidor_sabe_do_aparelho,
            device_knowledge_of_server: c.aparelho_sabe_do_servidor,
            schema_version_of_knowledge: c.versao_de_esquema,
            queue_calculations_for_server_entities: c.refazer_calculos,
        }
    }
}

impl From<BudgetKnowledge> for Conhecimento {
    fn from(k: BudgetKnowledge) -> Self {
        Conhecimento {
            do_aparelho: k.current_device_knowledge,
            servidor_sabe_do_aparelho: k.server_knowledge_of_device,
            aparelho_sabe_do_servidor: k.device_knowledge_of_server,
            versao_de_esquema: k.schema_version_of_knowledge,
            refazer_calculos: k.queue_calculations_for_server_entities,
        }
    }
}

/// O que fica no arquivo do aparelho.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CopiaLocal {
    orcamento: String,
    aparelho: String,
    conhecimento: Conhecimento,
    /// tipo -> id -> entidade
    entidades: BTreeMap<String, BTreeMap<String, Guardada>>,
    #[serde(skip)]
    permissao_negada: Option<ServerError>,
}

impl BudgetHost for CopiaLocal {
    fn device_id(&self) -> Option<&str> {
        Some(&self.aparelho)
    }
    fn is_logged_in(&self) -> bool {
        true
    }
    fn persist_budget_knowledge_values(&mut self, _: &str, k: &BudgetKnowledge) {
        self.conhecimento = (*k).into();
    }
    fn get_changed_budget_entities_to_sync(&mut self, _: &str, k: &BudgetKnowledge) -> OutgoingChangedEntities {
        let mut out = Map::new();
        for (tipo, por_id) in &self.entidades {
            let pendentes = por_id.values().filter(|g| g.conhecimento_local > k.server_knowledge_of_device);
            let lista: Vec<Value> = if tipo == "be_transactions" {
                // Transações vão em grupos, com as subtransações (nenhuma aqui).
                pendentes
                    .map(|g| json!({ "id": g.valor["id"], "be_transaction": g.valor, "be_subtransactions": [] }))
                    .collect()
            } else {
                pendentes.map(|g| g.valor.clone()).collect()
            };
            if lista.is_empty() {
                continue;
            }
            let tipo = if tipo == "be_transactions" { "be_transaction_groups" } else { tipo.as_str() };
            out.insert(tipo.to_owned(), if tipo == "be_budget" { lista[0].clone() } else { Value::Array(lista) });
        }
        serde_json::from_value(Value::Object(out)).unwrap_or_default()
    }
    fn handle_sync_budget_data_with_server_response(
        &mut self,
        _: &Arc<Mutex<BudgetKnowledge>>,
        _: &SyncBudgetDataRequest,
        response: &SyncBudgetDataResponse,
    ) -> HandledResponse {
        let mut mudou = false;
        if let Ok(Value::Object(recebidas)) = serde_json::to_value(&response.changed_entities) {
            for (tipo, valor) in recebidas {
                let lista = match valor {
                    Value::Array(lista) => lista,
                    Value::Null => continue,
                    uma => vec![uma],
                };
                for entidade in lista {
                    let Some(id) = entidade.get("id").and_then(Value::as_str).map(str::to_owned) else { continue };
                    let guardada = Guardada { valor: entidade, conhecimento_local: 0 };
                    self.entidades.entry(tipo.clone()).or_default().insert(id, guardada);
                    mudou = true;
                }
            }
        }
        HandledResponse { server_had_changed_entities: mudou, should_persist_server_knowledge: true }
    }
    fn possibly_import_transactions_after_budget_sync(&mut self, _: &SyncBudgetDataResponse, _: bool) {}
    fn handle_permission_denied_sync_error(&mut self, error: &ServerError) {
        self.permissao_negada = Some(error.clone());
    }
}

/// Como a categoria estourou no mês (só quando o disponível é negativo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estouro {
    /// Parte paga com dinheiro (vermelho na tela).
    Dinheiro,
    /// Parte paga no cartão, sem dinheiro reservado (amarelo na tela).
    Credito,
}

/// Uma linha do plano do mês.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDoPlano {
    pub nome: String,
    pub atribuido: i64,
    /// Saídas do mês; na categoria de pagamento, também o gasto coberto que
    /// passou para ela.
    pub atividade: i64,
    pub disponivel: i64,
    /// Tipo do estouro, quando o disponível é negativo.
    pub estouro: Option<Estouro>,
    /// Categoria de pagamento do cartão: quanto da fatura não tem dinheiro
    /// reservado (0 quando a fatura está coberta).
    pub falta: i64,
    /// `true` na categoria de pagamento do cartão.
    pub pagamento: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrupoNoPlano {
    pub nome: String,
    pub categorias: Vec<LinhaDoPlano>,
}

/// O plano de um mês, calculado no aparelho.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plano {
    pub mes: Date,
    /// `None` antes do primeiro mês com transação ou atribuição.
    pub a_atribuir: Option<i64>,
    pub grupos: Vec<GrupoNoPlano>,
    /// Só quando há saídas ou saldo sem categoria.
    pub sem_categoria: Option<LinhaDoPlano>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeConta {
    pub nome: String,
    pub orcamentaria: bool,
    /// Saldo de trabalho.
    pub saldo: i64,
    /// Compensadas e reconciliadas.
    pub saldo_compensado: i64,
}

/// Um aparelho: a cópia local do orçamento, sem I/O. Quem guarda a cópia
/// (um arquivo, o armazenamento do navegador) usa [`Aparelho::de_json`] e
/// [`Aparelho::para_json`]; com a feature `arquivo`, a camada de arquivo
/// (`abrir`, `salvar`…) faz isso num arquivo.
#[derive(Debug)]
pub struct Aparelho {
    copia: CopiaLocal,
    /// Arquivo da cópia, quando veio de [`Aparelho::abrir`] ou foi criada num.
    #[cfg(feature = "arquivo")]
    caminho: Option<PathBuf>,
}

impl Aparelho {
    /// Cria um orçamento novo, com a categoria de receita ("A atribuir").
    pub fn novo_orcamento(nome: &str) -> Self {
        let mut a = Self::para_orcamento(&novo_id());
        a.semear_base(nome);
        a
    }

    /// Põe no orçamento o que todo orçamento tem: o nome, o grupo interno e
    /// a categoria "A atribuir".
    fn semear_base(&mut self, nome: &str) {
        let id = self.copia.orcamento.clone();
        self.mudar("be_budget", json!({ "id": id, "budget_name": nome, "is_tombstone": false }));
        self.mudar(
            "be_master_categories",
            json!({ "id": GRUPO_INTERNO, "name": "Interno", "internal_name": "MasterCategory/__Internal__", "sortable_index": 0, "is_tombstone": false }),
        );
        self.mudar(
            "be_subcategories",
            json!({
                "id": CATEGORIA_A_ATRIBUIR, "entities_master_category_id": GRUPO_INTERNO, "name": NOME_A_ATRIBUIR,
                "internal_name": CATEGORIA_RECEITA, "sortable_index": 0, "is_tombstone": false
            }),
        );
    }

    /// Prepara um aparelho novo para um orçamento que já está no servidor;
    /// a primeira sincronização traz tudo.
    pub fn para_orcamento(orcamento: &str) -> Self {
        Self::de_copia(CopiaLocal {
            orcamento: orcamento.to_owned(),
            aparelho: novo_id(),
            conhecimento: Conhecimento { versao_de_esquema: TOSTAO_SCHEMA_VERSION, ..Default::default() },
            ..Default::default()
        })
    }

    fn de_copia(copia: CopiaLocal) -> Self {
        Self {
            copia,
            #[cfg(feature = "arquivo")]
            caminho: None,
        }
    }

    /// Lê a cópia local de um texto JSON (o que [`Aparelho::para_json`] escreve).
    pub fn de_json(texto: &str) -> Result<Self, Erro> {
        let copia = serde_json::from_str(texto).map_err(|e| Erro::Arquivo(format!("não é uma cópia do Tostão: {e}")))?;
        Ok(Self::de_copia(copia))
    }

    /// A cópia local como JSON, para quem guarda o aparelho gravar.
    pub fn para_json(&self) -> Result<String, Erro> {
        serde_json::to_string_pretty(&self.copia).map_err(|e| Erro::Arquivo(e.to_string()))
    }

    pub fn orcamento(&self) -> &str {
        &self.copia.orcamento
    }

    pub fn id(&self) -> &str {
        &self.copia.aparelho
    }

    /// Entidades mudadas aqui que o servidor ainda não recebeu.
    pub fn mudancas_pendentes(&self) -> usize {
        let enviado = self.copia.conhecimento.servidor_sabe_do_aparelho;
        self.copia.entidades.values().flat_map(|m| m.values()).filter(|g| g.conhecimento_local > enviado).count()
    }

    /// Mudança local: sobe o conhecimento do aparelho e marca a entidade.
    fn mudar(&mut self, tipo: &str, entidade: Value) {
        self.copia.conhecimento.do_aparelho += 1;
        let id = entidade["id"].as_str().expect("entidade com id").to_owned();
        let guardada = Guardada { valor: entidade, conhecimento_local: self.copia.conhecimento.do_aparelho };
        self.copia.entidades.entry(tipo.to_owned()).or_default().insert(id, guardada);
    }

    /// Entidades não removidas de um tipo.
    fn vivas<'a>(&'a self, tipo: &str) -> impl Iterator<Item = &'a Value> + 'a {
        self.copia
            .entidades
            .get(tipo)
            .into_iter()
            .flat_map(|m| m.values())
            .map(|g| &g.valor)
            .filter(|v| v.get("is_tombstone") != Some(&Value::Bool(true)))
    }

    /// Id da entidade viva com este nome (sem diferença de maiúsculas).
    fn por_nome(&self, tipo: &str, campo: &str, nome: &str, o_que: &str) -> Result<String, Erro> {
        let achadas: Vec<&Value> = self.com_nome(tipo, campo, nome).collect();
        match achadas.as_slice() {
            [uma] => Ok(uma["id"].as_str().unwrap_or_default().to_owned()),
            [] => Err(Erro::Nome(format!("não existe {o_que} com o nome \"{nome}\""))),
            _ => Err(Erro::Nome(format!("há mais de um(a) {o_que} com o nome \"{nome}\""))),
        }
    }

    fn proximo_indice(&self, tipo: &str) -> i64 {
        self.vivas(tipo).filter_map(|v| v.get("sortable_index")?.as_i64()).max().unwrap_or(0) + 1
    }

    fn com_nome<'a>(&'a self, tipo: &str, campo: &'a str, nome: &'a str) -> impl Iterator<Item = &'a Value> + 'a {
        let nome = nome.to_lowercase();
        self.vivas(tipo).filter(move |v| v.get(campo).and_then(Value::as_str).is_some_and(|n| n.to_lowercase() == nome))
    }

    fn nome_livre(&self, tipo: &str, campo: &str, nome: &str, o_que: &str) -> Result<(), Erro> {
        if nome.trim().is_empty() {
            return Err(Erro::Nome(format!("dê um nome à {o_que}")));
        }
        match self.com_nome(tipo, campo, nome).next() {
            Some(_) => Err(Erro::Nome(format!("já existe {o_que} com o nome \"{nome}\""))),
            None => Ok(()),
        }
    }

    /// Cria a conta; o cartão ganha a sua categoria de pagamento.
    pub fn criar_conta(&mut self, nome: &str, tipo: TipoDeConta) -> Result<String, Erro> {
        self.nome_livre("be_accounts", "account_name", nome, "conta")?;
        let id = novo_id();
        let indice = self.proximo_indice("be_accounts");
        self.mudar(
            "be_accounts",
            json!({
                "id": id, "account_name": nome, "account_type": tipo.account_type(),
                "on_budget": tipo != TipoDeConta::Acompanhamento, "sortable_index": indice, "is_tombstone": false
            }),
        );
        if tipo == TipoDeConta::Cartao {
            if self.vivas("be_master_categories").all(|v| v["id"] != GRUPO_PAGAMENTOS) {
                let indice = self.proximo_indice("be_master_categories");
                self.mudar(
                    "be_master_categories",
                    json!({ "id": GRUPO_PAGAMENTOS, "name": "Pagamentos de cartão", "sortable_index": indice, "is_tombstone": false }),
                );
            }
            let indice = self.proximo_indice("be_subcategories");
            self.mudar(
                "be_subcategories",
                json!({
                    "id": novo_id(), "entities_master_category_id": GRUPO_PAGAMENTOS, "entities_account_id": id,
                    "name": nome, "sortable_index": indice, "is_tombstone": false
                }),
            );
        }
        Ok(id)
    }

    pub fn criar_grupo(&mut self, nome: &str) -> Result<String, Erro> {
        self.nome_livre("be_master_categories", "name", nome, "grupo")?;
        let id = novo_id();
        let indice = self.proximo_indice("be_master_categories");
        self.mudar("be_master_categories", json!({ "id": id, "name": nome, "sortable_index": indice, "is_tombstone": false }));
        Ok(id)
    }

    pub fn criar_categoria(&mut self, nome: &str, grupo: &str) -> Result<String, Erro> {
        let grupo = self.por_nome("be_master_categories", "name", grupo, "grupo")?;
        self.nome_livre("be_subcategories", "name", nome, "categoria")?;
        let id = novo_id();
        let indice = self.proximo_indice("be_subcategories");
        self.mudar(
            "be_subcategories",
            json!({ "id": id, "entities_master_category_id": grupo, "name": nome, "sortable_index": indice, "is_tombstone": false }),
        );
        Ok(id)
    }

    /// Atribui `valor` à categoria no mês (substitui a atribuição anterior).
    /// O id é o mesmo em todos os aparelhos, então a última mudança vale.
    pub fn atribuir(&mut self, categoria: &str, mes: Date, valor: i64) -> Result<(), Erro> {
        let categoria = self.por_nome("be_subcategories", "name", categoria, "categoria")?;
        if categoria == CATEGORIA_A_ATRIBUIR {
            return Err(Erro::Nome(format!("não se atribui a \"{NOME_A_ATRIBUIR}\"")));
        }
        self.gravar_atribuicao(&categoria, start_of_month(mes), valor);
        Ok(())
    }

    /// Tira `valor` da categoria de origem e põe no destino, no mês. Com
    /// "A atribuir" na origem, é só atribuir ao destino. Usada por "mover de
    /// outra" e por "cobrir estouro" (que move exatamente o estouro).
    pub fn movimentar(&mut self, origem: &str, destino: &str, mes: Date, valor: i64) -> Result<(), Erro> {
        let origem = self.por_nome("be_subcategories", "name", origem, "categoria")?;
        let destino = self.por_nome("be_subcategories", "name", destino, "categoria")?;
        if destino == CATEGORIA_A_ATRIBUIR {
            return Err(Erro::Nome(format!("não se atribui a \"{NOME_A_ATRIBUIR}\"")));
        }
        if origem == destino {
            return Err(Erro::Nome("a origem e o destino são a mesma categoria".into()));
        }
        let mes = start_of_month(mes);
        let do_destino = self.atribuido_em(&destino, mes);
        if origem != CATEGORIA_A_ATRIBUIR {
            let da_origem = self.atribuido_em(&origem, mes);
            self.gravar_atribuicao(&origem, mes, da_origem - valor);
        }
        self.gravar_atribuicao(&destino, mes, do_destino + valor);
        Ok(())
    }

    fn gravar_atribuicao(&mut self, categoria: &str, mes: Date, valor: i64) {
        self.mudar(
            "be_monthly_subcategory_budgets",
            json!({
                "id": id_da_atribuicao(categoria, mes), "entities_subcategory_id": categoria,
                "month": mes.to_string(), "budgeted": valor, "is_tombstone": false
            }),
        );
    }

    /// O atribuído da categoria no mês (0 quando não há atribuição).
    fn atribuido_em(&self, categoria: &str, mes: Date) -> i64 {
        let id = id_da_atribuicao(categoria, mes);
        self.vivas("be_monthly_subcategory_budgets")
            .find(|v| v["id"].as_str() == Some(id.as_str()))
            .and_then(|v| v["budgeted"].as_i64())
            .unwrap_or(0)
    }

    /// Lança uma transação na conta. Numa transferência, `valor` é o da
    /// ponta desta conta, e o espelho leva o oposto na outra.
    pub fn lancar(&mut self, conta: &str, data: Date, valor: i64, destino: &Destino) -> Result<String, Erro> {
        let conta = self.por_nome("be_accounts", "account_name", conta, "conta")?;
        let transacao = |id: &str, conta: &str, valor: i64| {
            json!({
                "id": id, "entities_account_id": conta, "date": data.to_string(), "amount": valor,
                "cleared": "Uncleared", "accepted": true, "is_tombstone": false
            })
        };
        let id = novo_id();
        let mut t = transacao(&id, &conta, valor);
        match destino {
            Destino::Categoria(nome) => {
                t["entities_subcategory_id"] = json!(self.por_nome("be_subcategories", "name", nome, "categoria")?);
            }
            Destino::SemCategoria => {}
            Destino::Transferencia(outra) => {
                let outra = self.por_nome("be_accounts", "account_name", outra, "conta")?;
                if outra == conta {
                    return Err(Erro::Nome("a transferência precisa de duas contas diferentes".into()));
                }
                let espelho = novo_id();
                t["transfer_account_id"] = json!(outra);
                t["transfer_transaction_id"] = json!(espelho);
                let mut e = transacao(&espelho, &outra, -valor);
                e["transfer_account_id"] = json!(conta);
                e["transfer_transaction_id"] = json!(id);
                self.mudar("be_transactions", e);
            }
        }
        self.mudar("be_transactions", t);
        Ok(id)
    }

    /// Sincronização incremental: envia as mudanças pendentes e recebe as
    /// do servidor. Não grava nada: quem guarda a cópia grava depois, com
    /// [`Aparelho::para_json`] (ou `salvar`, com a feature `arquivo`).
    pub async fn sincronizar<T: Transport>(&mut self, transport: T, token: &str) -> Result<SyncResult, Erro> {
        let mut api = ApiV1::new(transport);
        api.session_token = Some(token.to_owned());
        let knowledge = Arc::new(Mutex::new(BudgetKnowledge::from(self.copia.conhecimento)));
        let orcamento = self.copia.orcamento.clone();
        // A sincronização trabalha sobre um clone: se esta future for descartada
        // no `await`, `self.copia` continua inteira (o `mem::take` a deixava vazia).
        let mut sync = BudgetSync::new(api, self.copia.clone(), TOSTAO_SCHEMA_VERSION);
        let resultado =
            sync.internal_sync_budget_data_with_server(Some(&orcamento), Some(&knowledge), SyncType::Delta).await;
        self.copia = sync.host;
        let resultado = resultado.map_err(Erro::Sincronizacao)?;
        if let Some(e) = self.copia.permissao_negada.take() {
            return Err(Erro::PermissaoNegada(e));
        }
        self.copia.conhecimento = (*knowledge.lock().unwrap_or_else(|p| p.into_inner())).into();
        Ok(resultado)
    }

    /// Calcula o orçamento com as entidades da cópia local, ao menos até
    /// `ate` e até um mês depois do último com movimento.
    fn calcular(&self, ate: Option<Date>) -> Result<Calculos, Erro> {
        let todas = |tipo: &str| {
            self.copia.entidades.get(tipo).map(|m| m.values().map(|g| g.valor.clone()).collect::<Vec<_>>())
        };
        let entidades = IncomingChangedEntities {
            be_accounts: todas("be_accounts"),
            be_subcategories: todas("be_subcategories"),
            be_monthly_budgets: todas("be_monthly_budgets"),
            be_monthly_subcategory_budgets: todas("be_monthly_subcategory_budgets"),
            be_transactions: todas("be_transactions"),
            be_subtransactions: todas("be_subtransactions"),
            ..Default::default()
        };
        let orcamento = orcamento_das_entidades(&entidades).map_err(|e| {
            Erro::Valor(format!("a cópia local tem uma entidade que não dá para ler: {} {} (campo {})", e.tipo, e.id, e.campo))
        })?;
        let ultimo = motor::calcular(&orcamento).meses.last().map(|m| add_months(m.mes, 1));
        let ultimo_mes = ultimo.max(ate.map(start_of_month));
        Ok(motor::calcular(&motor::Orcamento { ultimo_mes, ..orcamento }))
    }

    /// O plano do mês, calculado aqui com a cópia local.
    pub fn plano(&self, mes: Date) -> Result<Plano, Erro> {
        let mes = start_of_month(mes);
        let n = |v: &Value, campo: &str| v.get(campo).and_then(Value::as_i64).unwrap_or(0);
        let calculos = self.calcular(Some(mes))?;
        let do_mes = calculos.mes(mes);

        let a_atribuir = do_mes.map(|m| m.a_atribuir);
        let sem_categoria = do_mes
            .map(|m| {
                let s = &m.sem_categoria;
                let atividade = s.cash_outflows.unwrap_or(0) + s.credit_outflows.unwrap_or(0);
                LinhaDoPlano {
                    nome: "Sem categoria".into(),
                    atribuido: 0,
                    atividade,
                    disponivel: s.balance.unwrap_or(0),
                    estouro: None,
                    falta: 0,
                    pagamento: false,
                }
            })
            .filter(|l| l.atividade != 0 || l.disponivel != 0);

        let mut grupos: Vec<&Value> = self.vivas("be_master_categories").filter(|g| g["id"] != GRUPO_INTERNO).collect();
        grupos.sort_by_key(|g| (n(g, "sortable_index"), g["name"].as_str().unwrap_or_default().to_owned()));
        let grupos = grupos
            .into_iter()
            .map(|g| {
                let mut categorias: Vec<&Value> =
                    self.vivas("be_subcategories").filter(|c| c["entities_master_category_id"] == g["id"]).collect();
                categorias.sort_by_key(|c| (n(c, "sortable_index"), c["name"].as_str().unwrap_or_default().to_owned()));
                let categorias = categorias
                    .into_iter()
                    .map(|c| {
                        let nome = c["name"].as_str().unwrap_or_default().to_owned();
                        let conta_do_cartao = c.get("entities_account_id").and_then(Value::as_str);
                        let pagamento = conta_do_cartao.is_some();
                        match do_mes.and_then(|m| m.categoria(c["id"].as_str().unwrap_or_default())) {
                            Some(calc) => {
                                let disponivel = calc.balance.unwrap_or(0);
                                LinhaDoPlano {
                                    nome,
                                    atribuido: calc.budgeted,
                                    atividade: calc.cash_outflows.unwrap_or(0) + calc.credit_outflows.unwrap_or(0)
                                        - calc.budgeted_spending,
                                    disponivel,
                                    estouro: if pagamento { None } else { estouro_de(calc, disponivel) },
                                    falta: conta_do_cartao.map_or(0, |conta| falta_da_fatura(&calculos, conta, disponivel)),
                                    pagamento,
                                }
                            }
                            None => LinhaDoPlano {
                                nome,
                                atribuido: 0,
                                atividade: 0,
                                disponivel: 0,
                                estouro: None,
                                falta: conta_do_cartao.map_or(0, |conta| falta_da_fatura(&calculos, conta, 0)),
                                pagamento,
                            },
                        }
                    })
                    .collect();
                GrupoNoPlano { nome: g["name"].as_str().unwrap_or_default().to_owned(), categorias }
            })
            .collect();

        Ok(Plano { mes, a_atribuir, grupos, sem_categoria })
    }

    /// As contas, com os saldos calculados aqui com a cópia local.
    pub fn contas(&self) -> Result<Vec<LinhaDeConta>, Erro> {
        let calculos = self.calcular(None)?;
        let mut contas: Vec<&Value> = self.vivas("be_accounts").collect();
        contas.sort_by_key(|c| c.get("sortable_index").and_then(Value::as_i64).unwrap_or(0));
        Ok(contas
            .into_iter()
            .map(|c| {
                let saldo = calculos.saldo(c["id"].as_str().unwrap_or_default());
                LinhaDeConta {
                    nome: c["account_name"].as_str().unwrap_or_default().to_owned(),
                    orcamentaria: c.get("on_budget").and_then(Value::as_bool).unwrap_or(true),
                    saldo: saldo.map_or(0, |s| s.saldo),
                    saldo_compensado: saldo.map_or(0, |s| s.saldo_compensado),
                }
            })
            .collect())
    }
}

/// Leitura e gravação da cópia em arquivo: uma camada fina sobre
/// [`Aparelho::de_json`] e [`Aparelho::para_json`].
#[cfg(feature = "arquivo")]
impl Aparelho {
    /// Cria um orçamento novo num arquivo que ainda não existe.
    pub fn criar_orcamento(caminho: impl AsRef<Path>, nome: &str) -> Result<Self, Erro> {
        let caminho = caminho_livre(caminho.as_ref())?;
        let mut a = Self::novo_orcamento(nome);
        a.caminho = Some(caminho);
        Ok(a)
    }

    /// Prepara um aparelho novo, num arquivo que ainda não existe, para um
    /// orçamento que já está no servidor.
    pub fn entrar(caminho: impl AsRef<Path>, orcamento: &str) -> Result<Self, Erro> {
        let caminho = caminho_livre(caminho.as_ref())?;
        let mut a = Self::para_orcamento(orcamento);
        a.caminho = Some(caminho);
        Ok(a)
    }

    pub fn abrir(caminho: impl AsRef<Path>) -> Result<Self, Erro> {
        let caminho = caminho.as_ref();
        let texto = std::fs::read_to_string(caminho).map_err(|e| {
            Erro::Arquivo(format!("não deu para ler {}: {e} (crie o orçamento ou entre num antes)", caminho.display()))
        })?;
        let mut a = Self::de_json(&texto).map_err(|e| Erro::Arquivo(format!("{}: {e}", caminho.display())))?;
        a.caminho = Some(caminho.to_owned());
        Ok(a)
    }

    /// Grava a cópia no arquivo de onde veio (num arquivo ao lado, depois troca).
    pub fn salvar(&self) -> Result<(), Erro> {
        let caminho = self.caminho().ok_or_else(|| Erro::Arquivo("este aparelho não tem arquivo para gravar".into()))?;
        let texto = self.para_json()?;
        let temporario = caminho.with_extension("tmp");
        std::fs::write(&temporario, texto)
            .and_then(|()| std::fs::rename(&temporario, caminho))
            .map_err(|e| Erro::Arquivo(format!("não deu para gravar {}: {e}", caminho.display())))
    }

    /// O arquivo da cópia, se ela tiver um.
    pub fn caminho(&self) -> Option<&Path> {
        self.caminho.as_deref()
    }
}

#[cfg(feature = "arquivo")]
fn caminho_livre(caminho: &Path) -> Result<PathBuf, Erro> {
    if caminho.exists() {
        return Err(Erro::Arquivo(format!("o arquivo {} já existe", caminho.display())));
    }
    Ok(caminho.to_owned())
}

/// Id da atribuição de uma categoria num mês (o mesmo em todos os aparelhos).
fn id_da_atribuicao(categoria: &str, mes: Date) -> String {
    format!("msb/{}/{categoria}", mes.format("%Y-%m"))
}

/// Tipo do estouro de uma categoria: dinheiro antes de crédito, como a tela
/// mostra. Só quando o disponível é negativo.
fn estouro_de(calc: &SubCategoryMonthCalc, disponivel: i64) -> Option<Estouro> {
    if disponivel >= 0 {
        None
    } else if calc.unbudgeted_cash_outflows < 0 {
        Some(Estouro::Dinheiro)
    } else if calc.unbudgeted_credit_outflows < 0 {
        Some(Estouro::Credito)
    } else {
        None
    }
}

/// Falta da fatura: a dívida do cartão (saldo negativo da conta) menos o
/// dinheiro reservado na categoria de pagamento. Disponível negativo não
/// reserva nada, então não aumenta a falta.
fn falta_da_fatura(calculos: &Calculos, conta: &str, disponivel: i64) -> i64 {
    let divida = (-calculos.saldo(conta).map_or(0, |s| s.saldo)).max(0);
    (divida - disponivel.max(0)).max(0)
}

/// Id novo, aleatório, no formato de um UUID (sem as garantias de versão).
/// A fonte é a do sistema (ou `crypto.getRandomValues`, no navegador): dois
/// aparelhos não podem gerar o mesmo id.
pub fn novo_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("a fonte de aleatoriedade do sistema não respondeu");
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

/// Lê um valor em reais: `80`, `-80`, `80,50`, `1.234,56`, `80.50`.
/// Devolve milésimos.
pub fn ler_valor(texto: &str) -> Result<i64, Erro> {
    let invalido = || Erro::Valor(format!("valor inválido: \"{texto}\" (use, por exemplo, -80 ou 1.234,56)"));
    let t = texto.trim();
    let (negativo, t) = match t.strip_prefix('-') {
        Some(resto) => (true, resto),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let (inteiro, fracao) = if t.contains(',') {
        let (i, f) = t.split_once(',').ok_or_else(invalido)?;
        (i.replace('.', ""), f.to_owned())
    } else if let Some((i, f)) = t.split_once('.') {
        // "1.000" e "1.000.000" são milhares; "80.5" é decimal.
        let grupos: Vec<&str> = t.split('.').collect();
        if grupos.len() > 2 || f.len() == 3 {
            if grupos[1..].iter().any(|g| g.len() != 3) {
                return Err(invalido());
            }
            (t.replace('.', ""), String::new())
        } else {
            (i.to_owned(), f.to_owned())
        }
    } else {
        (t.to_owned(), String::new())
    };
    if inteiro.is_empty() || !inteiro.chars().all(|c| c.is_ascii_digit()) || fracao.len() > 3
        || !fracao.chars().all(|c| c.is_ascii_digit())
    {
        return Err(invalido());
    }
    let inteiro: i64 = inteiro.parse().map_err(|_| invalido())?;
    let fracao: i64 = format!("{fracao:0<3}").parse().map_err(|_| invalido())?;
    let valor = inteiro.checked_mul(1000).and_then(|v| v.checked_add(fracao)).ok_or_else(invalido)?;
    Ok(if negativo { -valor } else { valor })
}

/// Milésimos em reais, como a tela: `-1.234,56`.
pub fn formatar_valor(milesimos: i64) -> String {
    let centavos = (milesimos.abs() + 5) / 10;
    let (inteiro, fracao) = (centavos / 100, centavos % 100);
    let digitos = inteiro.to_string();
    let mut com_pontos = String::new();
    for (i, c) in digitos.chars().enumerate() {
        if i > 0 && (digitos.len() - i) % 3 == 0 {
            com_pontos.push('.');
        }
        com_pontos.push(c);
    }
    let sinal = if milesimos < 0 && centavos > 0 { "-" } else { "" };
    format!("{sinal}{com_pontos},{fracao:02}")
}

/// Data `AAAA-MM-DD`.
pub fn ler_data(texto: &str) -> Result<Date, Erro> {
    Date::parse_from_str(texto.trim(), "%Y-%m-%d").map_err(|_| Erro::Valor(format!("data inválida: \"{texto}\" (use AAAA-MM-DD)")))
}

/// Mês `AAAA-MM` (ou uma data, de que vale o mês).
pub fn ler_mes(texto: &str) -> Result<Date, Erro> {
    let t = texto.trim();
    Date::parse_from_str(&format!("{t}-01"), "%Y-%m-%d")
        .or_else(|_| Date::parse_from_str(t, "%Y-%m-%d"))
        .map(start_of_month)
        .map_err(|_| Erro::Valor(format!("mês inválido: \"{texto}\" (use AAAA-MM)")))
}

#[cfg(test)]
mod testes_de_id {
    use super::novo_id;

    #[test]
    fn ids_novos_sao_distintos_e_tem_formato_de_uuid() {
        let (a, b) = (novo_id(), novo_id());
        assert_ne!(a, b);
        let partes: Vec<usize> = a.split('-').map(str::len).collect();
        assert_eq!(partes, [8, 4, 4, 4, 12]);
        assert!(a.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
    }
}
