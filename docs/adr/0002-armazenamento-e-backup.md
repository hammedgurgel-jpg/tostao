# 0002 — Armazenamento e backup do orçamento

Status: aceita (issue #18, decisões da conversa do #14)

## Contexto

O app roda só no celular, sem servidor próprio, e guarda o orçamento no
IndexedDB do navegador. Falta uma cópia que a família possa guardar fora do
aparelho e restaurar. A conversa do #14 fechou o modelo de dados e o formato
do backup; a ADR que ela cita não chegou ao repositório, e o número 0001 já é
do ícone de iOS (`0001-icone-ios-mantido.md`). Esta é a próxima livre.

## Decisão

**Armazenamento.** Continua como está: o aparelho guarda a cópia inteira num
registro só do IndexedDB (texto JSON). A loja por tipo de entidade do #14 fica
para depois. O formato do backup não depende dela.

**Formato do backup.** Um arquivo JSON com cabeçalho e listas de entidades
com id:

- `formato`: `"tostao-backup"`.
- `versao_de_esquema`: inteiro; é `TOSTAO_SCHEMA_VERSION` (hoje `1`).
- `criado_em`: instante da exportação, ISO 8601.
- `ultimo_mes`: mês que a tela mostrava na exportação (`AAAA-MM`).
- `casas_decimais`: `2`.
- `contas`: `id`, `nome`, `tipo` (`corrente`, `poupanca`, `dinheiro`, `cartao`,
  `acompanhamento`), `ordem`.
- `grupos`: `id`, `nome`, `ordem`.
- `categorias`: `id`, `nome`, `grupo_id`, `ordem` e, só na categoria de
  pagamento de um cartão, `conta_id`.
- `atribuicoes`: `id`, `categoria_id`, `mes` (`AAAA-MM`), `valor`.
- `transacoes`: `id`, `conta_id`, `data` (`AAAA-MM-DD`), `valor`,
  `categoria_id` (opcional; `categoria-a-atribuir` na entrada), `transferencia_id`
  (opcional, a ponta espelho), `compensacao` (`nao_compensada`, `compensada`
  ou `reconciliada`).

Valores são inteiros em milésimos. O arquivo não tem cálculos (saldos, plano,
estouros) nem campos de sincronização (conhecimento, `is_tombstone`); o plano
e os saldos são refeitos ao abrir. O grupo interno e a categoria "A atribuir"
não entram: o app os recria.

**Exportar** baixa o arquivo. **Importar** valida antes de mexer:

1. o `formato` e a `versao_de_esquema` são conferidos primeiro, com mensagem
   própria;
2. a estrutura, os ids, os nomes (sem repetir nem usar nome reservado), as
   referências, as datas e os meses são conferidos;
3. a tela mostra a data do backup e as contagens, e só "Substituir" troca o
   orçamento inteiro.

Um arquivo recusado não toca no orçamento guardado. O núcleo devolve o erro
antes de montar o orçamento novo (`Aparelho::importar_backup`), e a ponte só
troca o aparelho quando isso dá certo.

## Consequências

- A `versao_de_esquema` é a versão do formato. Um backup de outra versão é
  recusado, porque ainda não há migrações: quando houver, cada versão nova
  ganha uma função numerada.
- O backup não leva o nome do orçamento: a importação mantém o nome do
  aparelho (hoje sempre "Família").
- Os ids das atribuições são recalculados ao importar (`msb/<mês>/<categoria>`),
  porque o motor os procura por essa regra. O id do arquivo é só informativo.
- A sincronização com o servidor não está na página (#17); a importação
  mantém o id do aparelho e o conhecimento dele, mas não há o que sincronizar
  daqui.
- O núcleo e a ponte têm testes de ida e volta e das recusas
  (`tests/backup.rs`, `src/web.rs`).
