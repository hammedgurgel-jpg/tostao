# Pesquisa: o motor Rust roda no navegador via WASM?

Ticket: [Pesquisa: o motor Rust roda no navegador via WASM?](https://github.com/hammedgurgel-jpg/tostao/issues/12)

## Resposta

**Sim, o núcleo do motor compila para `wasm32-unknown-unknown` sem nenhuma alteração.** O que impede o build do crate inteiro são as partes de servidor e de armazenamento em arquivo. Elas precisam ficar fora do alvo WASM, e não é preciso reescrever o motor.

## O que foi verificado

Um protótipo isolado em uma pasta temporária, fora do repositório, incluiu só os módulos do motor (`motor`, `calc`, `goals`, `dates`, `protocol`, `knowledge`) e as dependências `chrono` (sem recursos padrão) e `serde_json`. Resultado:

- `cargo check --target wasm32-unknown-unknown --lib`: compila sem erros.
- `cargo build --release` com `opt-level = "s"`, LTO e `strip`, exportando `motor::calcular` e `motor::classificar`: cerca de **107 KB** de `.wasm` e **37 KB** comprimido (gzip). Medida aproximada: não inclui a ponte com JavaScript, que é o próximo passo.

Esses módulos não usam sistema de arquivos, rede, relógio do sistema nem SQLite. Os imports são só de coleções da biblioteca padrão, `chrono` e `serde`.

## O que impede o crate inteiro

Ao rodar `cargo check --target wasm32-unknown-unknown --lib` no próprio repositório, o build falhou em `sqlite-wasm-rs`, dependência do `rusqlite` com a feature `bundled`. A falha foi `failed to find tool "clang"`: o build do SQLite para WASM precisa de um compilador C com alvo WASM, e a máquina de desenvolvimento não tem `clang`. Esse problema é do ambiente de build, não do motor.

Pelo mapa de código, os pontos que precisam de tratamento para o navegador são:

| Onde | Problema | Ação |
|---|---|---|
| `src/server.rs` | usa `rusqlite` (banco do servidor) | fica fora do alvo WASM |
| `src/http.rs` | usa `tiny_http` e `ureq` (servidor HTTP e cliente) | fica fora do alvo WASM |
| `src/aparelho.rs:324-337` | lê e grava o JSON do aparelho com `std::fs` | trocar por IndexedDB no lado da tela (ver ticket #14) |
| `src/aparelho.rs:698` | `SystemTime::now()` para a data de hoje | pela documentação da biblioteca padrão, `SystemTime::now` não é suportado em `wasm32-unknown-unknown`, e o programa entra em pânico se for chamado. **Não testado aqui.** A data deve vir do JavaScript (`Date`), passada para o motor |

Para o alvo WASM, `src/aparelho.rs` e o servidor não precisam compilar, só o motor. Pelo código, o motor só é consumido pelo aparelho e pelo servidor, então isolá-lo em outra unidade de build não quebra nada.

## Ferramenta de ponte

Recomendação: `wasm-bindgen`, com um crate fino que exporta duas funções. Uma recebe o orçamento como JSON e devolve os cálculos como JSON, e a outra recebe as transações e as atribuições para classificar. Não exponha structs Rust diretamente: o motor já trabalha com `serde_json::Value`, e JSON na fronteira mantém a ponte simples e testável.

Uma alternativa seria `wasm-pack`, que empacota o `wasm-bindgen` com o npm. Como o app é uma página estática no GitHub Pages, o `wasm-bindgen` puro com um `.js` gerado basta.

## Próximos passos (fora deste ticket)

1. Separar as dependências de servidor atrás de uma feature (`servidor = ["rusqlite", "tiny_http", "ureq"]`) ou em outro crate. Assim `cargo build --target wasm32-unknown-unknown` compila só o motor, sem precisar de `clang`.
2. Criar o crate da ponte com `wasm-bindgen` e confirmar a data via `Date` no JavaScript, sem `SystemTime`.
3. Medir o tamanho do build final com a ponte e as funções reais.

## Não verificado

- O comportamento de `SystemTime::now()` no navegador (só pela documentação).
- Compilação do crate da ponte com `wasm-bindgen`. O protótipo só compilou o motor.
- Testes do motor rodando no navegador. Os testes atuais rodam nativamente.
