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
