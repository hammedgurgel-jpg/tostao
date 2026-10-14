import init, { Tostao, formatarValor } from "./pkg/tostao.js";

const $ = (id) => document.getElementById(id);
let t;
// Falso quando o navegador não deixou ler a cópia guardada: nesse caso a
// página não grava, para não sobrescrever o que já está lá.
let guardando = true;

function esc(texto) {
  return String(texto).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
}

function celula(valor) {
  return `<td class="${valor < 0 ? "negativo" : ""}">${formatarValor(valor)}</td>`;
}

function linhaDoPlano(l, recuo) {
  return `<tr><td>${recuo}${esc(l.nome)}</td>${celula(l.atribuido)}${celula(l.atividade)}${celula(l.disponivel)}</tr>`;
}

function mesAtual() {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
}

function hojeIso() {
  const d = new Date();
  return new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 10);
}

/** Preenche um <select> mantendo a escolha atual, se ainda existir. */
function preencher(select, opcoes) {
  const atual = select.value;
  select.innerHTML = opcoes.map(([valor, rotulo]) => `<option value="${esc(valor)}">${esc(rotulo)}</option>`).join("");
  if (opcoes.some(([valor]) => valor === atual)) select.value = atual;
}

// Armazenamento: uma cópia (texto JSON) no IndexedDB do navegador.
const BANCO = "tostao";
const LOJA = "aparelho";
const CHAVE = "copia";

function abrirBanco() {
  return new Promise((ok, falha) => {
    const pedido = indexedDB.open(BANCO, 1);
    pedido.onupgradeneeded = () => pedido.result.createObjectStore(LOJA);
    pedido.onsuccess = () => ok(pedido.result);
    pedido.onerror = () => falha(pedido.error);
    pedido.onblocked = () => falha(new Error("o banco está aberto em outra aba"));
  });
}

async function operar(modo, fazer) {
  const db = await abrirBanco();
  try {
    return await new Promise((ok, falha) => {
      const tx = db.transaction(LOJA, modo);
      const pedido = fazer(tx.objectStore(LOJA));
      tx.oncomplete = () => ok(pedido ? pedido.result ?? null : null);
      tx.onerror = () => falha(tx.error);
      tx.onabort = () => falha(tx.error ?? new Error("gravação cancelada"));
    });
  } finally {
    db.close();
  }
}

const lerCopia = () => operar("readonly", (loja) => loja.get(CHAVE));
const gravarCopia = (texto) => operar("readwrite", (loja) => loja.put(texto, CHAVE));

/** Aviso que fica até recarregar: sem armazenamento, as mudanças só valem nesta aba. */
function semArmazenamento(motivo) {
  guardando = false;
  $("armazenamento").textContent = `${motivo} As mudanças só ficam nesta aba.`;
}

/** Carrega a cópia guardada, ou começa um orçamento novo se não houver. */
async function carregarAparelho() {
  let texto;
  try {
    texto = await lerCopia();
  } catch (e) {
    semArmazenamento(`Não deu para ler o que está guardado no navegador (${e}).`);
    return Tostao.novoOrcamento("Família");
  }
  if (texto) {
    try {
      return Tostao.deJson(texto);
    } catch (e) {
      semArmazenamento(`A cópia guardada no navegador não pôde ser lida (${e}).`);
      return Tostao.novoOrcamento("Família");
    }
  }
  return Tostao.novoOrcamento("Família");
}

function render() {
  const plano = JSON.parse(t.plano($("mes").value));
  const contas = JSON.parse(t.contas());

  const pendentes = t.mudancasPendentes();
  $("pendentes").textContent = `${pendentes} mudança(s) ainda não enviada(s) ao servidor.`;

  $("a-atribuir").textContent =
    plano.a_atribuir === null
      ? "A atribuir: — (nada lançado nem atribuído até este mês)"
      : `A atribuir: ${formatarValor(plano.a_atribuir)}`;

  const linhas = [];
  for (const g of plano.grupos) {
    linhas.push(`<tr class="grupo"><td colspan="4">${esc(g.nome)}</td></tr>`);
    for (const c of g.categorias) linhas.push(linhaDoPlano(c, "&nbsp;&nbsp;"));
  }
  if (plano.sem_categoria) linhas.push(linhaDoPlano(plano.sem_categoria, ""));
  $("plano").innerHTML = linhas.join("");

  $("contas").innerHTML = contas
    .map((c) => {
      const nome = c.orcamentaria ? c.nome : `${c.nome} (acompanhamento)`;
      return `<tr><td>${esc(nome)}</td>${celula(c.saldo)}${celula(c.saldo_compensado)}</tr>`;
    })
    .join("");

  const categorias = plano.grupos.flatMap((g) => g.categorias.map((c) => [c.nome, c.nome]));
  preencher($("lancar-conta"), contas.map((c) => [c.nome, c.nome]));
  preencher($("lancar-categoria"), [["", "Sem categoria"], ["A atribuir", "A atribuir"], ...categorias]);
  preencher($("atribuir-categoria"), categorias);
  preencher($("categoria-grupo"), plano.grupos.map((g) => [g.nome, g.nome]));
}

/** Depois de cada mudança: grava a cópia (se der) e mostra o que o núcleo calcula. */
async function aposMudanca() {
  if (guardando) {
    try {
      await gravarCopia(t.paraJson());
    } catch (e) {
      $("erro").textContent = `Não deu para guardar no navegador (${e}). A mudança só existe nesta aba.`;
    }
  }
  render();
}

/** Roda uma ação do usuário; mostra o erro do núcleo, se houver. */
function acao(fn) {
  return async (evento) => {
    evento.preventDefault();
    try {
      fn();
    } catch (e) {
      $("erro").textContent = String(e);
      return;
    }
    $("erro").textContent = "";
    await aposMudanca();
    evento.target.reset();
    $("lancar-data").value = hojeIso();
  };
}

function mostrarIds() {
  $("orcamento-id").textContent = t.orcamento();
  $("aparelho-id").textContent = t.id();
}

/**
 * Sincroniza com o servidor. O token vem do campo e vale só para esta chamada:
 * não é gravado no aparelho nem na cópia. Depois, a cópia nova é guardada.
 */
async function sincronizarAgora(evento) {
  evento.preventDefault();
  const servidor = $("servidor").value.trim();
  const token = $("token").value;
  if (!servidor || !token) {
    $("sincronizacao").textContent = "Informe o servidor e o token.";
    return;
  }
  if (!navigator.onLine) {
    $("sincronizacao").textContent = "Sem rede: o orçamento continua guardado neste aparelho. Sincronize quando a conexão voltar.";
    return;
  }
  $("sincronizar").disabled = true;
  $("sincronizacao").textContent = "Sincronizando…";
  let sucesso = false;
  try {
    await t.sincronizar(servidor, token);
    sucesso = true;
    $("sincronizacao").textContent = `Sincronizado com ${servidor}.`;
  } catch (e) {
    // Falha de rede (servidor fora do ar, endereço errado, rede sem internet).
    const texto = String(e);
    $("sincronizacao").textContent = texto.includes("não deu para falar com o servidor")
      ? `Não deu para falar com ${servidor}. Confira a conexão e o endereço; o orçamento continua guardado neste aparelho.`
      : `Não deu para sincronizar: ${texto}`;
  } finally {
    $("token").value = "";
    $("sincronizar").disabled = false;
  }
  $("erro").textContent = "";
  // Como o CLI: a cópia só é gravada depois de uma sincronização que deu certo.
  if (sucesso && guardando) {
    try {
      await gravarCopia(t.paraJson());
    } catch (e) {
      $("erro").textContent = `Não deu para guardar no navegador (${e}). A sincronização só existe nesta aba.`;
    }
  }
  render();
}

/** Troca a cópia deste aparelho pela de outro orçamento que já está no servidor. */
async function entrarNoOrcamento(evento) {
  evento.preventDefault();
  const id = $("entrar-id").value.trim();
  if (!id) return;
  try {
    const pendentes = t.mudancasPendentes();
    if (pendentes > 0 && !confirm(`Há ${pendentes} mudança(s) não enviada(s) neste aparelho. Entrar em outro orçamento as descarta. Continuar?`)) {
      return;
    }
    const nova = Tostao.paraOrcamento(id);
    // O handle antigo é liberado antes de trocar; o wasm não o libera sozinho.
    t.free();
    t = nova;
    if (guardando) await gravarCopia(t.paraJson());
  } catch (e) {
    $("erro").textContent = `Não deu para entrar nesse orçamento (${e}).`;
    return;
  }
  $("erro").textContent = "";
  $("sincronizacao").textContent = "Entrou no orçamento. Sincronize para trazer os dados.";
  $("entrar-id").value = "";
  mostrarIds();
  render();
}

/** Casca offline: o service worker guarda a página para abrir sem rede. */
async function registrarCasca() {
  if (!("serviceWorker" in navigator)) return;
  try {
    await navigator.serviceWorker.register("sw.js");
  } catch (e) {
    // O app segue funcionando online; só não abre sem rede.
    console.warn("Sem casca offline:", e);
  }
}

async function principal() {
  registrarCasca();
  await init();
  t = await carregarAparelho();
  if (guardando) {
    try {
      await gravarCopia(t.paraJson());
    } catch (e) {
      semArmazenamento(`Não deu para guardar no navegador (${e}).`);
    }
  }

  $("mes").value = mesAtual();
  $("lancar-data").value = hojeIso();
  mostrarIds();
  $("form-sincronizar").addEventListener("submit", sincronizarAgora);
  $("form-entrar").addEventListener("submit", entrarNoOrcamento);
  $("mes").addEventListener("change", () => {
    try {
      render();
    } catch (e) {
      $("erro").textContent = String(e);
    }
  });

  $("form-conta").addEventListener(
    "submit",
    acao(() => t.criarConta($("conta-nome").value.trim(), $("conta-tipo").value))
  );
  $("form-grupo").addEventListener("submit", acao(() => t.criarGrupo($("grupo-nome").value.trim())));
  $("form-categoria").addEventListener(
    "submit",
    acao(() => t.criarCategoria($("categoria-nome").value.trim(), $("categoria-grupo").value))
  );
  $("form-atribuir").addEventListener(
    "submit",
    acao(() => t.atribuir($("atribuir-categoria").value, $("atribuir-valor").value, $("mes").value))
  );
  $("form-lancar").addEventListener(
    "submit",
    acao(() => {
      const categoria = $("lancar-categoria").value;
      t.lancar(
        $("lancar-conta").value,
        $("lancar-valor").value,
        $("lancar-data").value,
        categoria === "" ? null : categoria,
        null
      );
    })
  );

  render();
}

principal().catch((e) => {
  $("erro").textContent = `Não deu para carregar o núcleo: ${e}`;
});
