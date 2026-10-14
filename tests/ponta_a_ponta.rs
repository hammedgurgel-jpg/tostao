//! De ponta a ponta: sobe o `tostao-servidor` numa porta livre e roda o
//! roteiro 1 da análise 01 por dois aparelhos (`tostao`), via HTTP.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const TOKEN: &str = "segredo-de-teste";

/// O servidor rodando; é parado no fim do teste.
struct Servidor {
    processo: Child,
    url: String,
}

impl Servidor {
    fn subir(banco: &Path) -> Self {
        let mut processo = Command::new(env!("CARGO_BIN_EXE_tostao-servidor"))
            .args(["--porta", "0", "--token", TOKEN, "--banco"])
            .arg(banco)
            .stdout(Stdio::piped())
            .spawn()
            .expect("sobe o servidor");
        let mut primeira = String::new();
        BufReader::new(processo.stdout.take().unwrap()).read_line(&mut primeira).unwrap();
        // "Tostão: servidor em http://127.0.0.1:PORTA (banco: …)"
        let url = primeira.split_whitespace().find(|p| p.starts_with("http://")).expect("endereço na saída").to_owned();
        Servidor { processo, url }
    }
}

impl Drop for Servidor {
    fn drop(&mut self) {
        let _ = self.processo.kill();
        let _ = self.processo.wait();
    }
}

/// Um aparelho: o cliente de linha de comando com um arquivo próprio.
struct Aparelho<'a> {
    arquivo: PathBuf,
    servidor: &'a Servidor,
}

impl Aparelho<'_> {
    fn rodar(&self, args: &[&str]) -> String {
        let saida = Command::new(env!("CARGO_BIN_EXE_tostao"))
            .arg("--arquivo")
            .arg(&self.arquivo)
            .args(["--servidor", &self.servidor.url, "--token", TOKEN])
            .args(args)
            .output()
            .expect("roda o cliente");
        let texto = String::from_utf8_lossy(&saida.stdout).into_owned();
        assert!(
            saida.status.success(),
            "tostao {args:?} falhou:\n{texto}\n{}",
            String::from_utf8_lossy(&saida.stderr)
        );
        texto
    }

    fn a_atribuir(&self, mes: &str) -> String {
        let plano = self.rodar(&["plano", "--mes", mes]);
        plano
            .lines()
            .find_map(|l| l.strip_prefix("A atribuir: "))
            .unwrap_or_else(|| panic!("sem \"A atribuir\" no plano:\n{plano}"))
            .to_owned()
    }

    /// Colunas da linha do plano que começa com `nome` (atribuído, atividade, disponível).
    fn linha(&self, mes: &str, nome: &str) -> Vec<String> {
        let plano = self.rodar(&["plano", "--mes", mes]);
        let linha = plano
            .lines()
            .find(|l| l.trim_start().starts_with(nome))
            .unwrap_or_else(|| panic!("sem \"{nome}\" no plano:\n{plano}"));
        linha.split_whitespace().rev().take(3).map(str::to_owned).collect::<Vec<_>>().into_iter().rev().collect()
    }
}

#[test]
fn roteiro_1_por_dois_aparelhos_via_http() {
    let pasta = std::env::temp_dir().join(format!("tostao-ponta-a-ponta-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let servidor = Servidor::subir(&pasta.join("servidor.db"));
    let a = Aparelho { arquivo: pasta.join("a.json"), servidor: &servidor };
    let b = Aparelho { arquivo: pasta.join("b.json"), servidor: &servidor };

    // A cria o orçamento: corrente com 1.000 de receita, cartão com 0.
    let criado = a.rodar(&["criar-orcamento", "Família"]);
    let orcamento = criado.lines().find_map(|l| l.strip_prefix("Orçamento criado: ")).unwrap().trim().to_owned();
    a.rodar(&["criar-conta", "Corrente", "--tipo", "corrente"]);
    a.rodar(&["criar-conta", "Cartão", "--tipo", "cartao"]);
    a.rodar(&["criar-grupo", "Dia a dia"]);
    a.rodar(&["criar-categoria", "Mercado", "--grupo", "Dia a dia"]);
    a.rodar(&["criar-categoria", "Lazer", "--grupo", "Dia a dia"]);
    a.rodar(&["lancar", "Corrente", "1000", "--categoria", "A atribuir", "--data", "2026-10-13"]);
    a.rodar(&["sincronizar"]);
    assert_eq!(a.a_atribuir("2026-10"), "1.000,00");

    // B entra no orçamento e recebe tudo.
    b.rodar(&["entrar", &orcamento]);
    b.rodar(&["sincronizar"]);
    assert_eq!(b.a_atribuir("2026-10"), "1.000,00");

    // 1A (em B): atribui 50 a Mercado e gasta 80 na corrente.
    b.rodar(&["atribuir", "Mercado", "50", "--mes", "2026-10"]);
    b.rodar(&["lancar", "Corrente", "-80", "--categoria", "Mercado", "--data", "2026-10-13"]);
    b.rodar(&["sincronizar"]);
    assert_eq!(b.linha("2026-10", "Mercado"), ["50,00", "-80,00", "-30,00"]);
    assert_eq!((b.a_atribuir("2026-10"), b.a_atribuir("2026-11")), ("950,00".into(), "920,00".into()));

    // 1B, 1C e 1D (em A): 40 na corrente sem categoria; 60 no cartão em
    // Lazer; 25 no cartão sem categoria.
    a.rodar(&["sincronizar"]);
    a.rodar(&["lancar", "Corrente", "-40", "--data", "2026-10-13"]);
    a.rodar(&["lancar", "Cartão", "-60", "--categoria", "Lazer", "--data", "2026-10-13"]);
    a.rodar(&["lancar", "Cartão", "-25", "--data", "2026-10-13"]);
    a.rodar(&["sincronizar"]);
    assert_eq!((a.a_atribuir("2026-10"), a.a_atribuir("2026-11")), ("950,00".into(), "880,00".into()));

    // B vê o mesmo depois de sincronizar.
    b.rodar(&["sincronizar"]);
    assert_eq!(b.a_atribuir("2026-10"), "950,00", "A atribuir de outubro na tela");
    assert_eq!(b.a_atribuir("2026-11"), "880,00", "A atribuir de novembro na tela");
    assert_eq!(b.linha("2026-10", "Lazer"), ["0,00", "-60,00", "-60,00"]);
    assert_eq!(b.linha("2026-10", "Sem categoria"), ["0,00", "-65,00", "-65,00"]);
    assert_eq!(b.linha("2026-11", "Mercado"), ["0,00", "0,00", "0,00"], "o negativo não passa adiante");
    let contas = b.rodar(&["contas"]);
    assert!(contas.lines().any(|l| l.starts_with("Corrente") && l.contains("880,00")), "{contas}");
    assert!(contas.lines().any(|l| l.starts_with("Cartão") && l.contains("-85,00")), "{contas}");

    drop(servidor);
    let _ = std::fs::remove_dir_all(&pasta);
}
