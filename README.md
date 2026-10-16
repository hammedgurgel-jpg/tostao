# Tostão

App de orçamento por envelopes para a família, compatível com o protocolo de
sincronização do YNAB, com servidor próprio. O vocabulário está em
[CONTEXT.md](CONTEXT.md); as conferências feitas na tela do YNAB, em
[docs/analise/](docs/analise/).

Por enquanto há dois programas de linha de comando, sem interface gráfica:

- **`tostao-servidor`**: o servidor. Guarda os orçamentos num arquivo SQLite,
  recebe as mudanças de cada aparelho, recalcula o orçamento com o motor e
  devolve aos aparelhos as mudanças dos outros e os cálculos.
- **`tostao`**: um aparelho. Guarda a cópia local do orçamento num arquivo
  JSON, faz mudanças nela, calcula o plano e os saldos com o mesmo motor e,
  quando houver servidor, sincroniza com ele.

## Compilar

Precisa do Rust (`rustup`, com o `cargo`). Na pasta do repositório:

```powershell
cargo build --release
```

Os programas ficam em `target\release\tostao-servidor.exe` e
`target\release\tostao.exe`. Para rodar os testes (inclusive o de ponta a
ponta, que sobe o servidor e roda o roteiro 1 por dois aparelhos):

```powershell
cargo test
```

## Features e build para wasm

O crate tem três features, todas ligadas por padrão: `servidor` (o
`tostao-servidor`, com SQLite e `tiny_http`; liga `http`), `http` (o transporte
HTTP do `tostao`, com `ureq`) e `arquivo` (ler e gravar a cópia do aparelho em
arquivo). O núcleo (o motor e o aparelho em memória, com
`Aparelho::de_json` e `para_json`) não depende de nenhuma delas e compila para
WebAssembly:

```powershell
rustup target add wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown --lib --no-default-features
```

## Página no navegador (em teste)

Uma página em `web/` roda o núcleo em WebAssembly, sem servidor: cria
orçamento, contas, grupos e categorias, lança transações e mostra o plano e os
saldos. A cópia do aparelho fica no IndexedDB do navegador, então recarregar a
página não perde nada. Se o navegador não deixar guardar (janela privada, por
exemplo), a página avisa e as mudanças valem só nessa aba.

```powershell
cargo install wasm-bindgen-cli --version 0.2.129 --locked
powershell -ExecutionPolicy Bypass -File web\construir.ps1
cd web
python -m http.server 8000
```

Abra `http://127.0.0.1:8000/index.html`. O `construir.ps1` gera `web/pkg/`
(ignorado pelo git) e grava a versão da casca em `web/sw.js` (a linha
`const VERSAO`), um hash dos arquivos guardados. Por isso um build local deixa
`web/sw.js` alterado no git: não o commite com a versão de um build.

Para sincronizar pela página, suba o servidor com a origem dela
(`--origem http://127.0.0.1:8000`, ver [Subir o servidor](#subir-o-servidor)),
digite o endereço do servidor e o token no campo "Sincronizar com o
servidor" e clique em Sincronizar. O token não é guardado no navegador: a
cópia do aparelho no IndexedDB não tem o token, e o campo é limpo depois do
envio. Para o segundo aparelho entrar no mesmo orçamento, copie o "Id do
orçamento" de um deles e use "Entrar num orçamento" (o segundo aparelho
precisa estar em outra origem, por exemplo `http://127.0.0.1:8001`, porque
o IndexedDB é separado por origem).

## Usar só num aparelho, sem servidor

O aparelho calcula sozinho: o `plano` e o `contas` saem do motor rodando
sobre a cópia local, inclusive as mudanças ainda não enviadas. Todos os
comandos, menos o `sincronizar`, funcionam sem servidor e sem token:

```powershell
Set-Alias tostao .	argetelease	ostao.exe
tostao criar-orcamento "Família"
tostao criar-conta Corrente --tipo corrente
tostao lancar Corrente 1000 --categoria "A atribuir"
tostao plano
```

O servidor só é preciso para dividir o orçamento entre aparelhos.

## Subir o servidor

O servidor ouve só nesta máquina (`127.0.0.1`), na porta 8080, e exige um
token, que os aparelhos enviam a cada sincronização. Num terminal:

```powershell
$env:TOSTAO_TOKEN = "troque-por-um-segredo"
.\target\release\tostao-servidor.exe --banco "$env:USERPROFILE\tostao\tostao.db"
```

A pasta do banco precisa existir (`mkdir "$env:USERPROFILE\tostao"`). Deixe
esse terminal aberto; `Ctrl+C` para.

| Opção | Variável | Padrão | O que é |
|---|---|---|---|
| `--token` | `TOSTAO_TOKEN` | (obrigatório) | segredo que os aparelhos enviam |
| `--banco` | `TOSTAO_BANCO` | `tostao.db` na pasta atual | arquivo SQLite |
| `--porta` | `TOSTAO_PORTA` | `8080` | porta; `0` escolhe uma livre |
| `--endereco` | `TOSTAO_ENDERECO` | `127.0.0.1` | endereço; outro valor expõe o servidor na rede |
| `--usuario` | `TOSTAO_USUARIO` | `familia` | usuário dono dos orçamentos |
| `--origem` | `TOSTAO_ORIGEM` | (nenhuma) | origem de uma página no navegador, ex.: `http://127.0.0.1:8000`; pode repetir (no ambiente, separe por vírgula). Sem ela, a página não consegue sincronizar |

Não há TLS: para usar de outra máquina (por exemplo, pelo Tailscale), passe
`--endereco` com o IP da interface certa e confie na rede, não no token.

## Usar o cliente

Em outro terminal, diga onde está o servidor e qual é o token (ou passe
`--servidor` e `--token` em cada comando; só o `sincronizar` os usa):

```powershell
$env:TOSTAO_SERVIDOR = "http://127.0.0.1:8080"
$env:TOSTAO_TOKEN = "troque-por-um-segredo"
Set-Alias tostao .\target\release\tostao.exe
tostao ajuda
```

Cada aparelho é um arquivo: `--arquivo <caminho>` (ou `TOSTAO_ARQUIVO`);
o padrão é `tostao-aparelho.json` na pasta atual.

| Comando | O que faz |
|---|---|
| `criar-orcamento <nome>` | cria um orçamento novo neste aparelho, com a categoria "A atribuir" (receita) |
| `entrar <id-do-orcamento>` | prepara um aparelho novo para um orçamento que já está no servidor |
| `criar-conta <nome> [--tipo corrente\|poupanca\|dinheiro\|cartao\|acompanhamento]` | cria a conta; o cartão ganha a sua categoria de pagamento |
| `criar-grupo <nome>` | cria um grupo de categorias |
| `criar-categoria <nome> --grupo <grupo>` | cria uma categoria |
| `atribuir <categoria> <valor> [--mes AAAA-MM]` | atribui o valor à categoria no mês (substitui o anterior) |
| `lancar <conta> <valor> [--categoria <nome> \| --transferir-para <conta>] [--data AAAA-MM-DD]` | lança uma transação; saída é negativa; sem `--categoria` fica sem categoria; `--categoria "A atribuir"` é receita do mês; `--transferir-para` cria as duas pontas |
| `sincronizar` | envia as mudanças deste aparelho e recebe as do servidor (precisa do servidor e do token) |
| `plano [--mes AAAA-MM]` | mostra o "A atribuir" e as categorias com atribuído, atividade e disponível |
| `contas` | mostra o saldo de trabalho e o saldo compensado de cada conta |
| `info` | mostra o id do orçamento, o do aparelho e quantas mudanças faltam enviar |

Valores em reais: `-80`, `1000`, `80,50`, `1.234,56`. Nomes de conta, grupo e
categoria não diferenciam maiúsculas; use aspas se tiverem espaço. Sem
`--mes` ou `--data`, vale o dia de hoje (UTC).

As mudanças ficam só no aparelho até o `sincronizar`, mas o `plano` e o
`contas` já as levam em conta: são calculados no aparelho, com a cópia
local. O que os outros aparelhos mudaram só aparece depois de sincronizar.

## Exemplo completo: o roteiro 1 por dois aparelhos

O roteiro 1 da [análise 01](docs/analise/01-estouro-cartao-metas.md) (seção 2),
conferido na tela do YNAB, com dois aparelhos, `a.json` e `b.json`. Com o
servidor no ar e as variáveis `TOSTAO_SERVIDOR` e `TOSTAO_TOKEN` definidas:

```powershell
# Aparelho A cria o orçamento: corrente com 1.000 de receita, cartão com 0.
tostao --arquivo a.json criar-orcamento "Família"
#   Orçamento criado: <id-do-orcamento>   (anote o id)
tostao --arquivo a.json criar-conta Corrente --tipo corrente
tostao --arquivo a.json criar-conta "Cartão" --tipo cartao
tostao --arquivo a.json criar-grupo "Dia a dia"
tostao --arquivo a.json criar-categoria Mercado --grupo "Dia a dia"
tostao --arquivo a.json criar-categoria Lazer --grupo "Dia a dia"
tostao --arquivo a.json lancar Corrente 1000 --categoria "A atribuir" --data 2026-10-13
tostao --arquivo a.json plano --mes 2026-10
#   A atribuir: 1.000,00
tostao --arquivo a.json sincronizar

# Aparelho B entra no mesmo orçamento.
tostao --arquivo b.json entrar <id-do-orcamento>
tostao --arquivo b.json sincronizar

# 1A, em B: atribui 50 a Mercado e gasta 80 na corrente.
tostao --arquivo b.json atribuir Mercado 50 --mes 2026-10
tostao --arquivo b.json lancar Corrente -80 --categoria Mercado --data 2026-10-13
tostao --arquivo b.json plano --mes 2026-10
#   A atribuir: 950,00; Mercado 50,00 / -80,00 / -30,00
tostao --arquivo b.json plano --mes 2026-11
#   A atribuir: 920,00
tostao --arquivo b.json sincronizar

# 1B, 1C e 1D, em A: 40 na corrente sem categoria, 60 no cartão em Lazer,
# 25 no cartão sem categoria.
tostao --arquivo a.json sincronizar
tostao --arquivo a.json lancar Corrente -40 --data 2026-10-13
tostao --arquivo a.json lancar "Cartão" -60 --categoria Lazer --data 2026-10-13
tostao --arquivo a.json lancar "Cartão" -25 --data 2026-10-13
tostao --arquivo a.json sincronizar

# B vê o resultado.
tostao --arquivo b.json sincronizar
tostao --arquivo b.json plano --mes 2026-10
#   A atribuir: 950,00; Lazer 0,00 / -60,00 / -60,00; Sem categoria -65,00
tostao --arquivo b.json plano --mes 2026-11
#   A atribuir: 880,00; Mercado e Lazer começam em 0
tostao --arquivo b.json contas
#   Corrente 880,00; Cartão -85,00
```

É o que o teste [tests/ponta_a_ponta.rs](tests/ponta_a_ponta.rs) faz. O
mesmo roteiro num aparelho só, sem servidor, está em
[tests/aparelho.rs](tests/aparelho.rs).

## Onde ficam os arquivos

| Arquivo | Onde | O que guarda |
|---|---|---|
| banco do servidor | `--banco` / `TOSTAO_BANCO` (padrão `tostao.db` na pasta onde o servidor foi aberto) | orçamentos, entidades, cálculos e o conhecimento de cada aparelho (SQLite) |
| cópia de um aparelho | `--arquivo` / `TOSTAO_ARQUIVO` (padrão `tostao-aparelho.json` na pasta atual) | id do orçamento e do aparelho, conhecimentos e as entidades, inclusive as mudanças ainda não enviadas (JSON) |

Apagar a cópia de um aparelho perde só as mudanças não enviadas; para
recuperar o resto, `entrar` de novo com o id do orçamento e `sincronizar`.
O `.gitignore` ignora `*.db` e `tostao-aparelho*.json`.

## Escolhas não conferidas com o YNAB

O protocolo foi reconstruído do cliente web do YNAB (ver [src/lib.rs](src/lib.rs)).
O que não foi observado e foi escolhido aqui:

- **Formato no fio** ([src/http.rs](src/http.rs)): o envelope do catálogo vai
  como JSON; a resposta é o `data` em JSON, com a versão do servidor em
  `X-Tostao-Server-Version`; erros vêm como `{"error": {"id", "data"}}` com o
  status do erro; o token vai em `Authorization: Bearer`; o id do aparelho,
  em `X-Tostao-Device-Id`.
- **Criação do orçamento**: o servidor cria o orçamento na primeira
  sincronização do usuário, porque a operação do YNAB não foi lida.
- **Cálculos** ([src/motor/protocolo.rs](src/motor/protocolo.rs),
  `entidades_dos_calculos`): os nomes `be_monthly_budget_calculations`,
  `be_monthly_subcategory_budget_calculations` e `be_account_calculations`
  vêm da captura de rede, mas os campos e os ids não foram vistos. O servidor
  recalcula o orçamento inteiro a cada lote aceito, do primeiro mês com
  transação ou atribuição até um mês depois do último. O aparelho guarda os
  cálculos que recebe, mas não os usa: calcula o plano e os saldos sozinho.
- **Entidades criadas pelo cliente** ([src/aparelho.rs](src/aparelho.rs)): os
  tipos de conta (`Checking`, `Savings`, `Cash`, `CreditCard`, `OtherAsset`),
  os ids fixos do grupo interno, da categoria de receita e do grupo de
  pagamentos de cartão, e o campo `month` direto na atribuição.

Fora do escopo por enquanto: interface gráfica, validação do par de
transferência no servidor, transações agendadas e hospedagem.
