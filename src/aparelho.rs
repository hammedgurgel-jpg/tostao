//! Aparelho de linha de comando: a cópia local do orçamento num arquivo
//! JSON, as mudanças que a família faz nela e a sincronização com o
//! servidor pelo [`BudgetSync`].
//!
//! Cada mudança local sobe o conhecimento do aparelho e marca a entidade com
//! esse valor; na sincronização vão as entidades marcadas depois do que o
//! servidor já sabe do aparelho. O que vem do servidor substitui a cópia
//! local, inclusive os cálculos (`be_*_calculations`), que só o servidor faz.
//!
//! Escolhas daqui, **não conferidas** com o YNAB: os tipos de conta
//! (`Checking`, `Savings`, `Cash`, `CreditCard`, `OtherAsset`), os ids fixos
//! do grupo interno, da categoria de receita e do grupo dos pagamentos de
//! cartão, e o campo `month` direto na atribuição.

use std::collections::hash_map::RandomState;
use std::collections::BTreeMap;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::api::{ApiError, ApiV1, ServerError, Transport};
use crate::calc::{calculate_left_to_budget_for_monthly_budget, MonthCalc, MonthlyBudget};
use crate::dates::{start_of_month, Date};
use crate::knowledge::BudgetKnowledge;
use crate::motor::protocolo::{CALCULOS_DE_CATEGORIA, CALCULOS_DE_CONTA, CALCULOS_DO_MES, CATEGORIA_RECEITA};
use crate::protocol::{
    OutgoingChangedEntities, SyncBudgetDataRequest, SyncBudgetDataResponse, SyncType, TOSTAO_SCHEMA_VERSION,
};
use crate::sync::{BudgetHost, BudgetSync, HandledResponse, SyncError, SyncResult};

/// Grupo das categorias internas (a de receita), que não aparece no plano.
pub const GRUPO_INTERNO: &str = "grupo-interno";
/// Categoria de receita, que a tela chama de "A atribuir".
pub const CATEGORIA_A_ATRIBUIR: &str = "categoria-a-atribuir";
pub const NOME_A_ATRIBUIR: &str = "A atribuir";
/// Grupo das categorias de pagamento do cartão.
pub const GRUPO_PAGAMENTOS: &str = "grupo-pagamentos-cartao";

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
            Erro::Sincronizacao(SyncError::UnexpectedSchemaVersion { expected, received }) => write!(
                f,
                "o servidor usa a versão de esquema {received} e este aparelho a {expected}; atualize o que estiver atrás"
            ),
            Erro::Sincronizacao(e) => write!(f, "falha ao sincronizar: {e:?}"),
            Erro::PermissaoNegada(e) => write!(f, "o servidor negou acesso ao orçamento: {}", e.id),
        }
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

/// Uma linha do plano do mês.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDoPlano {
    pub nome: String,
    pub atribuido: i64,
    /// Saídas do mês; na categoria de pagamento, também o gasto coberto que
    /// passou para ela.
    pub atividade: i64,
    pub disponivel: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrupoNoPlano {
    pub nome: String,
    pub categorias: Vec<LinhaDoPlano>,
}

/// O plano de um mês, com os cálculos que vieram do servidor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plano {
    pub mes: Date,
    /// `None` se o servidor ainda não mandou cálculo para este mês.
    pub a_atribuir: Option<i64>,
    pub grupos: Vec<GrupoNoPlano>,
    /// Só quando há saídas ou saldo sem categoria.
    pub sem_categoria: Option<LinhaDoPlano>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeConta {
    pub nome: String,
    pub orcamentaria: bool,
    /// Saldo de trabalho e saldo compensado, se o servidor já os mandou.
    pub saldo: Option<(i64, i64)>,
}

/// Um aparelho com a cópia local no arquivo `caminho`.
#[derive(Debug)]
pub struct Aparelho {
    pub caminho: PathBuf,
    copia: CopiaLocal,
}

impl Aparelho {
    /// Cria um orçamento novo, com a categoria de receita ("A atribuir"),
    /// num arquivo que ainda não existe.
    pub fn criar_orcamento(caminho: impl AsRef<Path>, nome: &str) -> Result<Self, Erro> {
        let mut a = Self::novo(caminho.as_ref(), &novo_id())?;
        let id = a.copia.orcamento.clone();
        a.mudar("be_budget", json!({ "id": id, "budget_name": nome, "is_tombstone": false }));
        a.mudar(
            "be_master_categories",
            json!({ "id": GRUPO_INTERNO, "name": "Interno", "internal_name": "MasterCategory/__Internal__", "sortable_index": 0, "is_tombstone": false }),
        );
        a.mudar(
            "be_subcategories",
            json!({
                "id": CATEGORIA_A_ATRIBUIR, "entities_master_category_id": GRUPO_INTERNO, "name": NOME_A_ATRIBUIR,
                "internal_name": CATEGORIA_RECEITA, "sortable_index": 0, "is_tombstone": false
            }),
        );
        Ok(a)
    }

    /// Prepara um aparelho novo para um orçamento que já está no servidor;
    /// a primeira sincronização traz tudo.
    pub fn entrar(caminho: impl AsRef<Path>, orcamento: &str) -> Result<Self, Erro> {
        Self::novo(caminho.as_ref(), orcamento)
    }

    fn novo(caminho: &Path, orcamento: &str) -> Result<Self, Erro> {
        if caminho.exists() {
            return Err(Erro::Arquivo(format!("o arquivo {} já existe", caminho.display())));
        }
        let copia = CopiaLocal {
            orcamento: orcamento.to_owned(),
            aparelho: novo_id(),
            conhecimento: Conhecimento { versao_de_esquema: TOSTAO_SCHEMA_VERSION, ..Default::default() },
            ..Default::default()
        };
        Ok(Self { caminho: caminho.to_owned(), copia })
    }

    pub fn abrir(caminho: impl AsRef<Path>) -> Result<Self, Erro> {
        let caminho = caminho.as_ref();
        let texto = std::fs::read_to_string(caminho).map_err(|e| {
            Erro::Arquivo(format!("não deu para ler {}: {e} (crie o orçamento ou entre num antes)", caminho.display()))
        })?;
        let copia = serde_json::from_str(&texto)
            .map_err(|e| Erro::Arquivo(format!("{} não é um arquivo do Tostão: {e}", caminho.display())))?;
        Ok(Self { caminho: caminho.to_owned(), copia })
    }

    /// Grava a cópia local (num arquivo ao lado, depois troca).
    pub fn salvar(&self) -> Result<(), Erro> {
        let texto = serde_json::to_string_pretty(&self.copia).map_err(|e| Erro::Arquivo(e.to_string()))?;
        let temporario = self.caminho.with_extension("tmp");
        std::fs::write(&temporario, texto)
            .and_then(|()| std::fs::rename(&temporario, &self.caminho))
            .map_err(|e| Erro::Arquivo(format!("não deu para gravar {}: {e}", self.caminho.display())))
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
        let mes = start_of_month(mes);
        self.mudar(
            "be_monthly_subcategory_budgets",
            json!({
                "id": format!("msb/{}/{categoria}", mes.format("%Y-%m")), "entities_subcategory_id": categoria,
                "month": mes.to_string(), "budgeted": valor, "is_tombstone": false
            }),
        );
        Ok(())
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
    /// do servidor (com os cálculos). Grava o arquivo se der certo.
    pub fn sincronizar<T: Transport>(&mut self, transport: T, token: &str) -> Result<SyncResult, Erro> {
        let mut api = ApiV1::new(transport);
        api.session_token = Some(token.to_owned());
        let knowledge = Arc::new(Mutex::new(BudgetKnowledge::from(self.copia.conhecimento)));
        let orcamento = self.copia.orcamento.clone();
        let mut sync = BudgetSync::new(api, std::mem::take(&mut self.copia), TOSTAO_SCHEMA_VERSION);
        let resultado = sync.internal_sync_budget_data_with_server(Some(&orcamento), Some(&knowledge), SyncType::Delta);
        self.copia = sync.host;
        let resultado = resultado.map_err(Erro::Sincronizacao)?;
        if let Some(e) = self.copia.permissao_negada.take() {
            return Err(Erro::PermissaoNegada(e));
        }
        self.copia.conhecimento = (*knowledge.lock().unwrap_or_else(|p| p.into_inner())).into();
        self.salvar()?;
        Ok(resultado)
    }

    /// O plano do mês, com os cálculos que vieram do servidor.
    pub fn plano(&self, mes: Date) -> Plano {
        let mes = start_of_month(mes);
        let mes_de = |v: &Value| {
            v.get("month").and_then(Value::as_str).and_then(|m| Date::parse_from_str(m.get(..10)?, "%Y-%m-%d").ok())
        };
        let n = |v: &Value, campo: &str| v.get(campo).and_then(Value::as_i64).unwrap_or(0);

        let meses: Vec<(Date, MonthCalc)> = self
            .vivas(CALCULOS_DO_MES)
            .filter_map(|v| {
                let calc = MonthCalc {
                    immediate_income: n(v, "immediate_income"),
                    additional_to_be_budgeted: n(v, "additional_to_be_budgeted"),
                    budgeted: n(v, "budgeted"),
                    over_spent: n(v, "over_spent"),
                    available_to_budget: n(v, "available_to_budget"),
                };
                Some((mes_de(v)?, calc))
            })
            .collect();
        let todos: Vec<MonthlyBudget<'_, Date>> =
            meses.iter().map(|(m, c)| MonthlyBudget { month: *m, calculation: Some(c) }).collect();
        let a_atribuir =
            todos.iter().find(|m| m.month == mes).map(|m| calculate_left_to_budget_for_monthly_budget(m, &todos));

        let do_mes = self.vivas(CALCULOS_DO_MES).find(|v| mes_de(v) == Some(mes));
        let sem_categoria = do_mes
            .map(|v| {
                let atividade = n(v, "uncategorized_cash_outflows") + n(v, "uncategorized_credit_outflows");
                LinhaDoPlano { nome: "Sem categoria".into(), atribuido: 0, atividade, disponivel: n(v, "uncategorized_balance") }
            })
            .filter(|l| l.atividade != 0 || l.disponivel != 0);

        let calculo_da = |categoria: &str| {
            self.vivas(CALCULOS_DE_CATEGORIA)
                .find(|v| mes_de(v) == Some(mes) && v.get("entities_subcategory_id").and_then(Value::as_str) == Some(categoria))
        };
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
                        match calculo_da(c["id"].as_str().unwrap_or_default()) {
                            Some(v) => LinhaDoPlano {
                                nome,
                                atribuido: n(v, "budgeted"),
                                atividade: n(v, "cash_outflows") + n(v, "credit_outflows") - n(v, "budgeted_spending"),
                                disponivel: n(v, "balance"),
                            },
                            None => LinhaDoPlano { nome, atribuido: 0, atividade: 0, disponivel: 0 },
                        }
                    })
                    .collect();
                GrupoNoPlano { nome: g["name"].as_str().unwrap_or_default().to_owned(), categorias }
            })
            .collect();

        Plano { mes, a_atribuir, grupos, sem_categoria }
    }

    /// As contas, com os saldos que vieram do servidor.
    pub fn contas(&self) -> Vec<LinhaDeConta> {
        let mut contas: Vec<&Value> = self.vivas("be_accounts").collect();
        contas.sort_by_key(|c| c.get("sortable_index").and_then(Value::as_i64).unwrap_or(0));
        contas
            .into_iter()
            .map(|c| {
                let saldo = self.vivas(CALCULOS_DE_CONTA).find(|s| s["entities_account_id"] == c["id"]).map(|s| {
                    (s["balance"].as_i64().unwrap_or(0), s["cleared_balance"].as_i64().unwrap_or(0))
                });
                LinhaDeConta {
                    nome: c["account_name"].as_str().unwrap_or_default().to_owned(),
                    orcamentaria: c.get("on_budget").and_then(Value::as_bool).unwrap_or(true),
                    saldo,
                }
            })
            .collect()
    }
}

/// Id novo, aleatório, no formato de um UUID (sem as garantias de versão).
pub fn novo_id() -> String {
    let parte = || RandomState::new().build_hasher().finish();
    let (a, b) = (parte(), parte());
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        a >> 32,
        (a >> 16) & 0xffff,
        a & 0xffff,
        b >> 48,
        b & 0xffff_ffff_ffff
    )
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

/// A data de hoje, em UTC.
pub fn hoje() -> Date {
    let segundos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    chrono::DateTime::from_timestamp(segundos, 0).map_or(Date::default(), |d| d.date_naive())
}
