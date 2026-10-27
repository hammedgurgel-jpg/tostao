import init, { Tostao, formatarValor, resumoDoBackup } from "./pkg/tostao.js";

const $ = (id) => document.getElementById(id);
let t;
// Falso quando o navegador não deixou ler a cópia guardada: nesse caso a
// página não grava, para não sobrescrever o que já está lá.
let guardando = true;
// O último plano e as contas que a tela mostrou: as folhas leem daqui.
let planoAtual = null;
let contasAtuais = [];

function esc(texto) {
  return String(texto).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
}

function mesAtual() {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
}

function hojeIso() {
  const d = new Date();
  return new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 10);
}

/** Mês `AAAA-MM` deslocado em `d` meses. */
function somarMes(mes, d) {
  const [ano, m] = mes.split("-").map(Number);
  const x = new Date(ano, m - 1 + d, 1);
  return `${x.getFullYear()}-${String(x.getMonth() + 1).padStart(2, "0")}`;
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

// ---------- Plano do mês (variante A) ----------

/** Cor da faixa e das pílulas: verde > 0, cinza 0, vermelho < 0. */
const classeDoValor = (v) => (v > 0 ? "positivo" : v < 0 ? "negativo" : "zero");

/** Pílula da disponível: vermelho no estouro em dinheiro, amarelo no crédito e na fatura com falta. */
function pilula(l) {
  if (l.estouro === "dinheiro") return "negativo";
  if (l.estouro === "credito" || l.falta > 0) return "aviso";
  return classeDoValor(l.disponivel);
}

/** Explicação curta embaixo do nome da categoria. */
function notaDaLinha(l) {
  if (l.estouro === "dinheiro") return "Estourou em dinheiro: sai do A atribuir do mês seguinte";
  if (l.estouro === "credito") return "Estourou no crédito: vira dívida no cartão";
  if (l.falta > 0) return `Falta R$ ${formatarValor(l.falta)} para a fatura`;
  return "";
}

function todasCategorias() {
  return planoAtual.grupos.flatMap((g) => g.categorias);
}

function categoriaPorNome(nome) {
  return todasCategorias().find((c) => c.nome === nome);
}

function linhaDaCategoria(l) {
  const nota = notaDaLinha(l);
  return `<button type="button" class="linha" data-categoria="${esc(l.nome)}" aria-label="${esc(l.nome)}: atribuir ou mover">
    <span class="nome">${esc(l.nome)}${nota ? `<span class="nota">${esc(nota)}</span>` : ""}</span>
    <span class="num">${formatarValor(l.atribuido)}</span>
    <span class="num"><span class="pilula ${pilula(l)}">${formatarValor(l.disponivel)}</span></span>
  </button>`;
}

function renderPlano(plano) {
  planoAtual = plano;
  const a = plano.a_atribuir;
  $("faixa").className = `faixa ${a === null ? "zero" : classeDoValor(a)}`;
  $("a-atribuir").textContent = a === null ? "—" : `R$ ${formatarValor(a)}`;
  $("faixa-nota").textContent =
    a === null ? "Nada lançado nem atribuído até este mês" : a < 0 ? "Atribuiu mais do que tem" : a > 0 ? "Dinheiro livre para atribuir" : "Tudo atribuído";

  const grupos = plano.grupos.map((g) => {
    const atribuido = g.categorias.reduce((s, c) => s + c.atribuido, 0);
    const disponivel = g.categorias.reduce((s, c) => s + c.disponivel, 0);
    return `<div class="grupo" style="margin: 0 0 12px">
      <div class="grupo-cabeca"><span>${esc(g.nome)}<button type="button" class="acao-pequena" data-nova-categoria="${esc(g.nome)}">+ categoria</button></span><span class="num">${formatarValor(atribuido)}</span><span class="num ${disponivel < 0 ? "negativo" : ""}">${formatarValor(disponivel)}</span></div>
      ${g.categorias.map(linhaDaCategoria).join("")}
    </div>`;
  });
  if (plano.sem_categoria) {
    const s = plano.sem_categoria;
    grupos.push(`<div class="grupo" style="margin: 0">
      <div class="linha" style="cursor: default">
        <span class="nome">Sem categoria<span class="nota">Gastos que ainda não têm categoria</span></span>
        <span class="num"></span>
        <span class="num ${s.disponivel < 0 ? "negativo" : ""}">${formatarValor(s.disponivel)}</span>
      </div>
    </div>`);
  }
  $("plano").innerHTML = grupos.length ? grupos.join("") : `<p class="suave" style="padding: 0 14px 4px">Nenhum grupo ainda. Toque em + grupo para começar.</p>`;
}

function renderContas(contas) {
  $("contas").innerHTML = contas
    .map((c) => {
      const nome = c.orcamentaria ? c.nome : `${c.nome} (acompanhamento)`;
      return `<tr><td>${esc(nome)}</td><td class="${c.saldo < 0 ? "negativo" : ""}">${formatarValor(c.saldo)}</td><td class="${c.saldo_compensado < 0 ? "negativo" : ""}">${formatarValor(c.saldo_compensado)}</td></tr>`;
    })
    .join("");
}

function render() {
  const plano = JSON.parse(t.plano($("mes").value));
  contasAtuais = JSON.parse(t.contas());
  renderPlano(plano);
  renderContas(contasAtuais);
}

/** Depois de cada mudança: grava a cópia (se der) e redesenha a tela. */
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

// ---------- Folhas: atribuir e lançar ----------

/** Centavos digitados no teclado: o último dígito entra à direita, como numa calculadora. */
function aplicarTecla(centavos, tecla) {
  if (tecla === "⌫") return Math.floor(centavos / 10);
  const novo = centavos * (tecla === "00" ? 100 : 10) + (tecla === "00" ? 0 : Number(tecla));
  return novo < 1e9 ? novo : centavos;
}

/** Centavos em texto para o núcleo: `80,50`. */
const textoDosCentavos = (centavos) => (centavos / 100).toFixed(2).replace(".", ",");

const teclado = () =>
  `<div class="teclado">${[1, 2, 3, 4, 5, 6, 7, 8, 9, "00", 0, "⌫"].map((n) => `<button type="button" data-tecla="${n}">${n}</button>`).join("")}</div>`;

function abrirFolha(html) {
  const raiz = $("folha-raiz");
  raiz.innerHTML = `<div class="scrim" data-fechar></div><div class="folha" role="dialog" aria-modal="true">${html}</div>`;
  raiz.hidden = false;
  return raiz;
}

function fecharFolha() {
  const raiz = $("folha-raiz");
  raiz.innerHTML = "";
  raiz.hidden = true;
}

/** Mostra o erro do núcleo dentro da folha aberta. */
function erroNaFolha(e) {
  const alvo = $("folha-raiz").querySelector("[data-erro]");
  if (alvo) alvo.textContent = String(e);
}

/**
 * Folha de atribuir: "Atribuir" define o total da categoria no mês; "Mover de
 * outra" tira um valor de outra categoria (ou do A atribuir); "Cobrir estouro"
 * move exatamente o que estourou.
 */
function abrirAtribuir(nome, modoInicial = "atribuir") {
  const l = categoriaPorNome(nome);
  if (!l) return;
  const mes = $("mes").value;
  let modo = modoInicial;
  let origem = "A atribuir";
  let centavos = Math.round(Math.max(l.atribuido, 0) / 10);
  if (modo === "cobrir") centavos = Math.round(-l.disponivel / 10);

  const desenhar = () => {
    const aAtribuir = planoAtual.a_atribuir ?? 0;
    const opcoes = [`<option value="A atribuir" ${origem === "A atribuir" ? "selected" : ""}>A atribuir (R$ ${formatarValor(aAtribuir)})</option>`]
      .concat(todasCategorias().filter((c) => c.nome !== nome && !c.pagamento).map((c) => `<option value="${esc(c.nome)}" ${origem === c.nome ? "selected" : ""}>${esc(c.nome)} (${formatarValor(c.disponivel)})</option>`))
      .join("");
    const podeCobrir = l.disponivel < 0;
    const raiz = abrirFolha(`
      <h3>${esc(nome)}</h3>
      <div class="linha-info"><span>Atribuído</span><span class="num">${formatarValor(l.atribuido)}</span></div>
      <div class="linha-info"><span>Atividade</span><span class="num">${formatarValor(l.atividade)}</span></div>
      <div class="linha-info"><span>Disponível</span><span class="pilula ${pilula(l)} num">${formatarValor(l.disponivel)}</span></div>
      ${l.estouro ? `<p class="suave" style="margin: 0">${esc(notaDaLinha(l))}</p>` : ""}
      <div class="segmentos" role="group" aria-label="Como mexer no dinheiro">
        <button type="button" data-modo="atribuir" class="${modo === "atribuir" ? "ligado" : ""}">Atribuir</button>
        <button type="button" data-modo="mover" class="${modo === "mover" ? "ligado" : ""}">Mover de outra</button>
        <button type="button" data-modo="cobrir" class="${modo === "cobrir" ? "ligado" : ""}" ${podeCobrir ? "" : "disabled"}>Cobrir estouro</button>
      </div>
      ${modo === "atribuir" ? "" : `<label>Tirar de <select data-origem>${opcoes}</select></label>`}
      <div class="visor" aria-live="polite">R$ ${formatarValor(centavos * 10)}</div>
      ${teclado()}
      <p class="aviso" data-erro></p>
      <div class="acoes">
        <button type="button" data-cancelar>Cancelar</button>
        <button type="button" class="principal" data-confirmar>${modo === "atribuir" ? "Atribuir" : modo === "mover" ? "Mover" : "Cobrir"}</button>
      </div>`);

    raiz.onclick = async (e) => {
      const b = e.target.closest("button");
      if (e.target.dataset.fechar !== undefined || (b && b.dataset.cancelar !== undefined)) return fecharFolha();
      if (!b) return;
      if (b.dataset.modo) {
        modo = b.dataset.modo;
        centavos = modo === "cobrir" ? Math.round(-l.disponivel / 10) : modo === "atribuir" ? Math.round(Math.max(l.atribuido, 0) / 10) : 0;
        return desenhar();
      }
      if (b.dataset.tecla) {
        centavos = aplicarTecla(centavos, b.dataset.tecla);
        return desenhar();
      }
      if (b.dataset.confirmar !== undefined) {
        const texto = textoDosCentavos(centavos);
        if (modo !== "atribuir" && centavos === 0) return erroNaFolha("Digite o valor.");
        try {
          if (modo === "atribuir") t.atribuir(nome, texto, mes);
          else t.movimentar(origem, nome, mes, texto);
        } catch (err) {
          return erroNaFolha(err);
        }
        fecharFolha();
        $("erro").textContent = "";
        await aposMudanca();
      }
    };
    raiz.onchange = (e) => {
      if (e.target.dataset.origem !== undefined) {
        origem = e.target.value;
        desenhar();
      }
    };
  };
  desenhar();
}

/**
 * Folha de lançar: saída (numa conta, com categoria ou sem), entrada (vai para
 * "A atribuir") e transferência (inclusive pagar a fatura, que é uma transferência
 * para a conta do cartão).
 */
function abrirLancar() {
  if (!contasAtuais.length) {
    $("erro").textContent = "Crie uma conta antes de lançar.";
    return;
  }
  const s = {
    tipo: "saida",
    conta: contasAtuais[0].nome,
    destino: contasAtuais[1]?.nome ?? "",
    categoria: "",
    data: $("mes").value.startsWith(mesAtual()) ? hojeIso() : `${$("mes").value}-01`,
    centavos: 0,
  };
  const ePagamentoDeCartao = (nome) => todasCategorias().some((c) => c.pagamento && c.nome === nome);

  const desenhar = () => {
    const contasOpcoes = contasAtuais.map((c) => `<option value="${esc(c.nome)}" ${c.nome === s.conta ? "selected" : ""}>${esc(c.nome)}</option>`).join("");
    let campo;
    if (s.tipo === "saida") {
      const grupos = planoAtual.grupos
        .map((g) => {
          const cats = g.categorias.filter((c) => !c.pagamento).map((c) => `<option value="${esc(c.nome)}" ${c.nome === s.categoria ? "selected" : ""}>${esc(c.nome)}</option>`).join("");
          return cats ? `<optgroup label="${esc(g.nome)}">${cats}</optgroup>` : "";
        })
        .join("");
      campo = `<label>Categoria <select data-categoria><option value="">Sem categoria</option>${grupos}</select></label>`;
    } else if (s.tipo === "entrada") {
      campo = `<p class="suave" style="margin: 0">A entrada vai para <b>A atribuir</b> deste mês.</p>`;
    } else {
      const destinos = contasAtuais
        .filter((c) => c.nome !== s.conta)
        .map((c) => `<option value="${esc(c.nome)}" ${c.nome === s.destino ? "selected" : ""}>${esc(c.nome)}${ePagamentoDeCartao(c.nome) ? " (pagar fatura)" : ""}</option>`)
        .join("");
      campo = `<label>Para <select data-destino>${destinos}</select></label>`;
    }
    const raiz = abrirFolha(`
      <h3>Lançar</h3>
      <div class="segmentos" role="group" aria-label="Tipo de lançamento">
        ${[["saida", "Saída"], ["entrada", "Entrada"], ["transferencia", "Transferência"]]
          .map(([v, rotulo]) => `<button type="button" data-tipo="${v}" class="${s.tipo === v ? "ligado" : ""}">${rotulo}</button>`)
          .join("")}
      </div>
      <label>${s.tipo === "transferencia" ? "De" : "Conta"} <select data-conta>${contasOpcoes}</select></label>
      ${campo}
      <label>Data <input type="date" data-data value="${esc(s.data)}"></label>
      <div class="visor" aria-live="polite">R$ ${formatarValor(s.centavos * 10)}</div>
      ${teclado()}
      <p class="aviso" data-erro></p>
      <div class="acoes">
        <button type="button" data-cancelar>Cancelar</button>
        <button type="button" class="principal" data-confirmar>Lançar</button>
      </div>`);

    raiz.onclick = async (e) => {
      const b = e.target.closest("button");
      if (e.target.dataset.fechar !== undefined || (b && b.dataset.cancelar !== undefined)) return fecharFolha();
      if (!b) return;
      if (b.dataset.tipo) {
        s.tipo = b.dataset.tipo;
        return desenhar();
      }
      if (b.dataset.tecla) {
        s.centavos = aplicarTecla(s.centavos, b.dataset.tecla);
        return desenhar();
      }
      if (b.dataset.confirmar !== undefined) {
        const texto = textoDosCentavos(s.centavos);
        if (s.centavos === 0) return erroNaFolha("Digite o valor.");
        try {
          if (s.tipo === "saida") t.lancar(s.conta, `-${texto}`, s.data, s.categoria || null, null);
          else if (s.tipo === "entrada") t.lancar(s.conta, texto, s.data, "A atribuir", null);
          else t.lancar(s.conta, `-${texto}`, s.data, null, s.destino);
        } catch (err) {
          return erroNaFolha(err);
        }
        fecharFolha();
        $("erro").textContent = "";
        await aposMudanca();
      }
    };
    raiz.onchange = (e) => {
      const d = e.target.dataset;
      if (d.conta !== undefined) s.conta = e.target.value;
      if (d.categoria !== undefined) s.categoria = e.target.value;
      if (d.destino !== undefined) s.destino = e.target.value;
      if (d.data !== undefined) s.data = e.target.value;
      desenhar();
    };
  };
  desenhar();
}

// ---------- Criar conta, grupo e categoria ----------

const TIPOS_DE_CONTA = [
  ["corrente", "Corrente"],
  ["poupanca", "Poupança"],
  ["dinheiro", "Dinheiro"],
  ["cartao", "Cartão"],
  ["acompanhamento", "Acompanhamento"],
];

/**
 * Folha com um nome (e, se `comTipo`, o tipo da conta). `criar(nome, tipo)`
 * faz a mudança; o erro do núcleo fica na folha.
 */
function abrirCriar(titulo, { comTipo = false, dica = "", placeholder = "" }, criar) {
  let tipo = "corrente";
  abrirFolha(`
    <h3>${esc(titulo)}</h3>
    ${dica ? `<p class="suave" style="margin: 0">${esc(dica)}</p>` : ""}
    <label>Nome <input data-nome autocomplete="off" placeholder="${esc(placeholder)}"></label>
    ${comTipo ? `<div class="tipos" role="group" aria-label="Tipo da conta">${TIPOS_DE_CONTA.map(([v, r]) => `<button type="button" data-tipo="${v}" class="${v === tipo ? "ligado" : ""}">${r}</button>`).join("")}</div>` : ""}
    <p class="aviso" data-erro></p>
    <div class="acoes">
      <button type="button" data-cancelar>Cancelar</button>
      <button type="button" class="principal" data-confirmar>Criar</button>
    </div>`);
  const raiz = $("folha-raiz");
  raiz.querySelector("[data-nome]").focus();
  raiz.onclick = async (e) => {
    const b = e.target.closest("button");
    if (e.target.dataset.fechar !== undefined || (b && b.dataset.cancelar !== undefined)) return fecharFolha();
    if (!b) return;
    if (b.dataset.tipo) {
      tipo = b.dataset.tipo;
      raiz.querySelectorAll("[data-tipo]").forEach((x) => x.classList.toggle("ligado", x === b));
      return;
    }
    if (b.dataset.confirmar !== undefined) {
      const nome = raiz.querySelector("[data-nome]").value.trim();
      try {
        criar(nome, tipo);
      } catch (err) {
        return erroNaFolha(err);
      }
      fecharFolha();
      $("erro").textContent = "";
      await aposMudanca();
    }
  };
}

// ---------- Backup em arquivo JSON ----------

/** Baixa o orçamento como arquivo; o mês da tela vai como `ultimo_mes`. */
function exportarBackup() {
  const aviso = $("backup-aviso");
  try {
    const texto = t.exportar(new Date().toISOString(), $("mes").value);
    const url = URL.createObjectURL(new Blob([texto], { type: "application/json" }));
    const link = document.createElement("a");
    link.href = url;
    link.download = `tostao-${hojeIso()}.json`;
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    aviso.textContent = "";
  } catch (e) {
    aviso.textContent = `Não deu para exportar: ${e}`;
  }
}

function dataDoBackup(iso) {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString("pt-BR", { dateStyle: "short", timeStyle: "short" });
}

/**
 * Lê o arquivo escolhido e, se o formato e a versão conferem, mostra a data e
 * as contagens. Só "Substituir" mexe no orçamento; cancelar não muda nada.
 */
function confirmarImportacao(texto) {
  const aviso = $("backup-aviso");
  let resumo;
  try {
    resumo = JSON.parse(resumoDoBackup(texto));
  } catch (e) {
    aviso.textContent = `Arquivo recusado: ${e}. O orçamento não mudou.`;
    return;
  }
  aviso.textContent = "";
  const c = resumo.contagens;
  abrirFolha(`
    <h3>Importar backup</h3>
    <p style="margin: 0">Backup de <b>${esc(dataDoBackup(resumo.criado_em))}</b>, com o mês ${esc(resumo.ultimo_mes)} aberto.</p>
    <p class="contagens">Contas: ${c.contas} · Grupos: ${c.grupos} · Categorias: ${c.categorias}<br>Atribuições: ${c.atribuicoes} · Transações: ${c.transacoes}</p>
    <p class="aviso" style="color: var(--negativo)"><b>Isto substitui todo o orçamento deste aparelho.</b></p>
    <p class="aviso" data-erro></p>
    <div class="acoes">
      <button type="button" data-cancelar>Cancelar</button>
      <button type="button" class="principal" data-confirmar>Substituir</button>
    </div>`);
  const raiz = $("folha-raiz");
  raiz.onclick = async (e) => {
    const b = e.target.closest("button");
    if (e.target.dataset.fechar !== undefined || (b && b.dataset.cancelar !== undefined)) return fecharFolha();
    if (!b || b.dataset.confirmar === undefined) return;
    try {
      t.importar(texto);
    } catch (err) {
      return erroNaFolha(err);
    }
    fecharFolha();
    $("erro").textContent = "";
    $("mes").value = resumo.ultimo_mes;
    aviso.textContent = "Backup importado.";
    await aposMudanca();
  };
}

/** Casca offline: o service worker guarda a página para abrir sem rede. */
async function registrarCasca() {
  if (!("serviceWorker" in navigator)) return;
  try {
    // `updateViaCache: "none"`: a checagem do próprio sw.js ignora o cache HTTP,
    // senão uma versão nova do sw.js demora até o max-age do Pages acabar.
    await navigator.serviceWorker.register("sw.js", { updateViaCache: "none" });
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
  $("mes-antes").addEventListener("click", () => {
    $("mes").value = somarMes($("mes").value, -1);
    render();
  });
  $("mes-depois").addEventListener("click", () => {
    $("mes").value = somarMes($("mes").value, 1);
    render();
  });
  $("mes").addEventListener("change", () => {
    try {
      render();
    } catch (e) {
      $("erro").textContent = String(e);
    }
  });

  $("plano").addEventListener("click", (e) => {
    const nova = e.target.closest("[data-nova-categoria]");
    if (nova) return abrirCriar(`Nova categoria em ${nova.dataset.novaCategoria}`, { placeholder: "Mercado" }, (nome) => t.criarCategoria(nome, nova.dataset.novaCategoria));
    const botao = e.target.closest("[data-categoria]");
    if (botao) abrirAtribuir(botao.dataset.categoria);
  });
  $("novo-grupo").addEventListener("click", () => abrirCriar("Novo grupo", { placeholder: "Contas fixas" }, (nome) => t.criarGrupo(nome)));
  $("nova-conta").addEventListener("click", () =>
    abrirCriar("Nova conta", { comTipo: true, dica: "Escolha o tipo e dê um nome." }, (nome, tipo) => t.criarConta(nome, tipo))
  );
  $("lancar-abrir").addEventListener("click", abrirLancar);

  $("exportar").addEventListener("click", exportarBackup);
  $("importar").addEventListener("click", () => $("arquivo").click());
  $("arquivo").addEventListener("change", async (e) => {
    const arquivo = e.target.files[0];
    e.target.value = "";
    if (!arquivo) return;
    try {
      confirmarImportacao(await arquivo.text());
    } catch (err) {
      $("backup-aviso").textContent = `Não deu para ler o arquivo: ${err}`;
    }
  });

  render();
}

principal().catch((e) => {
  $("erro").textContent = `Não deu para carregar o núcleo: ${e}`;
});
