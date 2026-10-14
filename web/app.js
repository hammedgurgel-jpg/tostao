import init, { Tostao, formatarValor } from "./pkg/tostao.js";

const $ = (id) => document.getElementById(id);
let t;

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

function render() {
  const plano = JSON.parse(t.plano($("mes").value));
  const contas = JSON.parse(t.contas());

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

/** Depois de cada mudança: a tela mostra o que o núcleo calcula. */
function aposMudanca() {
  render();
}

/** Roda uma ação do usuário; mostra o erro do núcleo, se houver. */
function acao(fn) {
  return (evento) => {
    evento.preventDefault();
    try {
      fn();
      $("erro").textContent = "";
      aposMudanca();
      evento.target.reset();
      $("lancar-data").value = hojeIso();
    } catch (e) {
      $("erro").textContent = String(e);
    }
  };
}

async function principal() {
  await init();
  t = Tostao.novoOrcamento("Família");

  $("mes").value = mesAtual();
  $("lancar-data").value = hojeIso();
  $("mes").addEventListener("change", () => {
    try {
      render();
      $("erro").textContent = "";
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
