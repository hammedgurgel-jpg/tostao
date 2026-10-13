//! Tostão: núcleo do app de orçamento da família (sincronização entre aparelhos
//! e servidor próprio, e cálculos do orçamento).
//!
//! O protocolo e os cálculos foram reconstruídos a partir do cliente web do YNAB.
//!
//! Fonte: análise do bundle `chunk.7bd67ec4d1cff4ed86e3.js` (SHA-256
//! c7e8b170…), feita com o REA em 2026-10-13, mais uma captura de rede
//! passiva das respostas de `/api/v1/catalog`.
//!
//! Não observado, portanto não reproduzido:
//! - a codificação do corpo da requisição no fio (fica no [`api::Transport`]);
//! - o nome do cabeçalho que leva `apiVersionHeader = "2026-01-01"`;
//! - a ordem dos campos nas respostas do servidor;
//! - como o servidor do YNAB soma transações em `be_*_calculations`; o
//!   [`motor`] faz essa soma com as regras conferidas na tela
//!   (`docs/analise/01-estouro-cartao-metas.md` e `03-transferencias.md`).
//!
//! `roundMilliDollars` está lido: `Math.round` por padrão, passo de 10 milésimos
//! para o real (ver [`calc::round_to_currency`]).

pub mod api;
pub mod calc;
pub mod dates;
pub mod goals;
pub mod knowledge;
pub mod motor;
pub mod protocol;
pub mod server;
pub mod sync;
