//! Aparelho de linha de comando do Tostão. Ver `README.md`.

use std::process::ExitCode;

use chrono::Datelike;
use tostao::aparelho::{formatar_valor, ler_data, ler_mes, ler_valor, Aparelho, Destino, Erro, LinhaDoPlano, TipoDeConta};
use tostao::dates::Date;
use tostao::http::HttpTransport;

const AJUDA: &str = "\
tostao: aparelho de linha de comando do Tostão

Uso: tostao [opções] <comando> [argumentos]

Comandos:
  criar-orcamento <nome>                  cria um orçamento novo neste aparelho
  entrar <id-do-orcamento>                prepara este aparelho para um orçamento do servidor
  criar-conta <nome> [--tipo <tipo>]      tipo: corrente (padrão), poupanca, dinheiro,
                                          cartao ou acompanhamento
  criar-grupo <nome>                      cria um grupo de categorias
  criar-categoria <nome> --grupo <grupo>  cria uma categoria no grupo
  atribuir <categoria> <valor> [--mes AAAA-MM]
                                          atribui o valor à categoria no mês (padrão: este)
  lancar <conta> <valor> [--categoria <nome> | --transferir-para <conta>] [--data AAAA-MM-DD]
                                          lança uma transação; saída é negativa; sem
                                          --categoria fica sem categoria; a categoria
                                          \"A atribuir\" é receita do mês
  sincronizar                             envia as mudanças e recebe as do servidor
  plano [--mes AAAA-MM]                   mostra o plano do mês (padrão: este)
  contas                                  mostra os saldos das contas
  info                                    mostra o orçamento, o aparelho e o que falta enviar

Opções (ou variáveis de ambiente):
  --arquivo <caminho>   TOSTAO_ARQUIVO   cópia local do aparelho (padrão: tostao-aparelho.json)
  --servidor <url>      TOSTAO_SERVIDOR  servidor (padrão: http://127.0.0.1:8080)
  --token <texto>       TOSTAO_TOKEN     token do servidor (para sincronizar)

O plano e as contas são calculados neste aparelho, com as mudanças ainda não
enviadas; só o `sincronizar` precisa do servidor e do token.

Valores em reais: -80, 1000, 80,50 ou 1.234,56. Nomes de conta, grupo e
categoria não diferenciam maiúsculas; use aspas se tiverem espaço.
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

/// Argumentos já separados: posicionais e `--opção valor`.
struct Args {
    posicionais: Vec<String>,
    opcoes: Vec<(String, String)>,
}

impl Args {
    fn ler() -> Result<Self, String> {
        let mut args = std::env::args().skip(1);
        let mut a = Args { posicionais: Vec::new(), opcoes: Vec::new() };
        while let Some(arg) = args.next() {
            if arg == "--ajuda" || arg == "-h" || arg == "--help" {
                a.posicionais.insert(0, "ajuda".into());
            } else if let Some(nome) = arg.strip_prefix("--") {
                let valor = args.next().ok_or_else(|| format!("falta o valor de {arg}"))?;
                a.opcoes.push((nome.to_owned(), valor));
            } else {
                a.posicionais.push(arg);
            }
        }
        Ok(a)
    }

    fn opcao(&self, nome: &str) -> Option<&str> {
        self.opcoes.iter().rev().find(|(n, _)| n == nome).map(|(_, v)| v.as_str())
    }

    fn opcao_ou_ambiente(&self, nome: &str, variavel: &str) -> Option<String> {
        self.opcao(nome).map(str::to_owned).or_else(|| std::env::var(variavel).ok().filter(|v| !v.is_empty()))
    }

    /// O n-ésimo argumento depois do comando.
    fn arg(&self, n: usize, o_que: &str) -> Result<&str, String> {
        self.posicionais.get(n + 1).map(String::as_str).ok_or_else(|| format!("falta {o_que}\n\n{AJUDA}"))
    }

    /// Recusa opções que o comando não conhece (evita erro de digitação
    /// passar em silêncio).
    fn so_opcoes(&self, aceitas: &[&str]) -> Result<(), String> {
        let globais = ["arquivo", "servidor", "token"];
        match self.opcoes.iter().find(|(n, _)| !aceitas.contains(&n.as_str()) && !globais.contains(&n.as_str())) {
            Some((n, _)) => Err(format!("opção desconhecida para este comando: --{n}")),
            None => Ok(()),
        }
    }
}

fn executar() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::ler()?;
    let arquivo = args.opcao_ou_ambiente("arquivo", "TOSTAO_ARQUIVO").unwrap_or_else(|| "tostao-aparelho.json".into());
    let servidor = args.opcao_ou_ambiente("servidor", "TOSTAO_SERVIDOR").unwrap_or_else(|| "http://127.0.0.1:8080".into());
    let token = || args.opcao_ou_ambiente("token", "TOSTAO_TOKEN").ok_or("defina o token com --token ou TOSTAO_TOKEN");
    let comando = args.posicionais.first().map(String::as_str).unwrap_or("ajuda");

    match comando {
        "ajuda" => print!("{AJUDA}"),
        "criar-orcamento" => {
            args.so_opcoes(&[])?;
            let a = Aparelho::criar_orcamento(&arquivo, args.arg(0, "o nome do orçamento")?)?;
            a.salvar()?;
            println!("Orçamento criado: {}", a.orcamento());
            println!("Cópia local em {arquivo}. Para dividi-lo com outros aparelhos, rode `tostao sincronizar`.");
        }
        "entrar" => {
            args.so_opcoes(&[])?;
            let a = Aparelho::entrar(&arquivo, args.arg(0, "o id do orçamento")?)?;
            a.salvar()?;
            println!("Aparelho {} pronto para o orçamento {}.", a.id(), a.orcamento());
            println!("Rode `tostao sincronizar` para trazer o orçamento do servidor.");
        }
        "criar-conta" => {
            args.so_opcoes(&["tipo"])?;
            let tipo = args.opcao("tipo").unwrap_or("corrente");
            let tipo = TipoDeConta::ler(tipo).ok_or_else(|| format!("tipo de conta desconhecido: {tipo}"))?;
            let nome = args.arg(0, "o nome da conta")?;
            mudar(&arquivo, |a| a.criar_conta(nome, tipo).map(|_| format!("Conta \"{nome}\" criada.")))?;
        }
        "criar-grupo" => {
            args.so_opcoes(&[])?;
            let nome = args.arg(0, "o nome do grupo")?;
            mudar(&arquivo, |a| a.criar_grupo(nome).map(|_| format!("Grupo \"{nome}\" criado.")))?;
        }
        "criar-categoria" => {
            args.so_opcoes(&["grupo"])?;
            let nome = args.arg(0, "o nome da categoria")?;
            let grupo = args.opcao("grupo").ok_or("diga o grupo com --grupo")?;
            mudar(&arquivo, |a| a.criar_categoria(nome, grupo).map(|_| format!("Categoria \"{nome}\" criada em \"{grupo}\".")))?;
        }
        "atribuir" => {
            args.so_opcoes(&["mes"])?;
            let categoria = args.arg(0, "a categoria")?;
            let valor = ler_valor(args.arg(1, "o valor")?)?;
            let mes = args.opcao("mes").map_or(Ok(hoje()), ler_mes)?;
            mudar(&arquivo, |a| {
                a.atribuir(categoria, mes, valor)
                    .map(|()| format!("{} atribuído a \"{categoria}\" em {}.", formatar_valor(valor), mes.format("%Y-%m")))
            })?;
        }
        "lancar" | "lançar" => {
            args.so_opcoes(&["categoria", "transferir-para", "data"])?;
            let conta = args.arg(0, "a conta")?;
            let valor = ler_valor(args.arg(1, "o valor")?)?;
            let data = args.opcao("data").map_or(Ok(hoje()), ler_data)?;
            let destino = match (args.opcao("categoria"), args.opcao("transferir-para")) {
                (Some(_), Some(_)) => return Err("use --categoria ou --transferir-para, não os dois".into()),
                (Some(c), None) => Destino::Categoria(c.to_owned()),
                (None, Some(c)) => Destino::Transferencia(c.to_owned()),
                (None, None) => Destino::SemCategoria,
            };
            mudar(&arquivo, |a| {
                a.lancar(conta, data, valor, &destino)
                    .map(|_| format!("Transação de {} lançada em \"{conta}\" ({data}).", formatar_valor(valor)))
            })?;
        }
        "sincronizar" => {
            args.so_opcoes(&[])?;
            let mut a = Aparelho::abrir(&arquivo)?;
            let enviando = a.mudancas_pendentes();
            let transporte = HttpTransport::new(&servidor, &token()?, a.id());
            let r = a.sincronizar(transporte, &token()?)?;
            a.salvar()?;
            let conhecimento = r.server_response.map_or(0, |r| r.current_server_knowledge);
            println!("Sincronizado com {servidor}: {enviando} mudança(s) enviada(s); conhecimento do servidor {conhecimento}.");
        }
        "plano" => {
            args.so_opcoes(&["mes"])?;
            let a = Aparelho::abrir(&arquivo)?;
            let mes = args.opcao("mes").map_or(Ok(hoje()), ler_mes)?;
            mostrar_plano(&a, mes)?;
        }
        "contas" => {
            args.so_opcoes(&[])?;
            let a = Aparelho::abrir(&arquivo)?;
            println!("{:<28} {:>14} {:>14}", "Conta", "Saldo", "Compensado");
            for c in a.contas()? {
                let nome = if c.orcamentaria { c.nome } else { format!("{} (acompanhamento)", c.nome) };
                println!("{nome:<28} {:>14} {:>14}", formatar_valor(c.saldo), formatar_valor(c.saldo_compensado));
            }
            avisar_pendentes(&a);
        }
        "info" => {
            args.so_opcoes(&[])?;
            let a = Aparelho::abrir(&arquivo)?;
            println!("Orçamento: {}", a.orcamento());
            println!("Aparelho:  {}", a.id());
            println!("Arquivo:   {arquivo}");
            println!("Servidor:  {servidor}");
            println!("Mudanças a enviar: {}", a.mudancas_pendentes());
        }
        outro => return Err(format!("comando desconhecido: {outro}\n\n{AJUDA}").into()),
    }
    Ok(())
}

/// Abre o aparelho, faz a mudança, grava e avisa que falta sincronizar.
fn mudar(arquivo: &str, f: impl FnOnce(&mut Aparelho) -> Result<String, Erro>) -> Result<(), Erro> {
    let mut a = Aparelho::abrir(arquivo)?;
    let mensagem = f(&mut a)?;
    a.salvar()?;
    println!("{mensagem}");
    avisar_pendentes(&a);
    Ok(())
}

fn avisar_pendentes(a: &Aparelho) {
    let n = a.mudancas_pendentes();
    if n > 0 {
        println!("({n} mudança(s) ainda não enviada(s) ao servidor.)");
    }
}

const MESES: [&str; 12] = [
    "janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro",
    "dezembro",
];

fn mostrar_plano(a: &Aparelho, mes: Date) -> Result<(), Erro> {
    let plano = a.plano(mes)?;
    println!("Plano de {} de {}", MESES[plano.mes.month0() as usize], plano.mes.year());
    match plano.a_atribuir {
        Some(v) => println!("A atribuir: {}", formatar_valor(v)),
        None => println!("A atribuir: — (nada lançado nem atribuído até este mês)"),
    }
    println!();
    println!("{:<30} {:>12} {:>12} {:>12}", "Categoria", "Atribuído", "Atividade", "Disponível");
    let linha = |l: &LinhaDoPlano, recuo: &str| {
        let nome = format!("{recuo}{}", l.nome);
        println!(
            "{nome:<30} {:>12} {:>12} {:>12}",
            formatar_valor(l.atribuido),
            formatar_valor(l.atividade),
            formatar_valor(l.disponivel)
        );
    };
    for g in &plano.grupos {
        println!("{}", g.nome);
        for c in &g.categorias {
            linha(c, "  ");
        }
    }
    if let Some(s) = &plano.sem_categoria {
        linha(s, "");
    }
    avisar_pendentes(a);
    Ok(())
}

/// A data de hoje, em UTC. O relógio fica no aparelho de linha de comando,
/// fora do núcleo, que também roda onde não há relógio do sistema.
fn hoje() -> Date {
    let segundos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    chrono::DateTime::from_timestamp(segundos, 0).map_or(Date::default(), |d| d.date_naive())
}
