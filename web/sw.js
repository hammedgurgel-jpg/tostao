// Casca do Tostão: guarda a página e o núcleo em WebAssembly para abrir sem
// rede. O orçamento em si fica no IndexedDB (ver app.js), não aqui.
//
// A casca se atualiza sozinha: cada pedido responde com a cópia guardada (se
// houver) e busca a versão nova na rede para a próxima abertura. Por isso o
// nome do cache é fixo e não há versão para subir a cada mudança.
// Os testes em tests/casca.rs conferem esta lista com os arquivos de web/.
const CACHE = "tostao-casca";
const PRECACHE = [
  "index.html",
  "app.js",
  "manifest.webmanifest",
  "icons/icon-192.png",
  "icons/icon-512.png",
  "pkg/tostao.js",
  "pkg/tostao_bg.wasm",
];

// Endereços absolutos da casca: só eles entram no cache.
const CASCA = new Set(PRECACHE.map((caminho) => new URL(caminho, self.registration.scope).href));
const INICIO = new URL("index.html", self.registration.scope).href;

self.addEventListener("install", (evento) => {
  // Um arquivo de cada vez: um que falta (o pkg/ sem build, por exemplo) não
  // impede de guardar o resto. A versão em lote desiste de tudo se um faltar.
  evento.waitUntil(
    caches.open(CACHE).then((cache) =>
      Promise.all(PRECACHE.map((caminho) => cache.add(caminho).catch((e) => console.warn("Não guardou", caminho, e))))
    )
  );
  self.skipWaiting();
});

self.addEventListener("activate", (evento) => {
  evento.waitUntil(
    caches
      .keys()
      .then((nomes) => Promise.all(nomes.filter((nome) => nome !== CACHE).map((nome) => caches.delete(nome))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener("fetch", (evento) => {
  const pedido = evento.request;
  // Só GET do próprio site. A sincronização (POST para o servidor, outra
  // origem) passa direto: sem rede ela falha e a página mostra o erro.
  if (pedido.method !== "GET" || new URL(pedido.url).origin !== self.location.origin) return;
  evento.respondWith(responder(pedido));
});

async function responder(pedido) {
  const cache = await caches.open(CACHE);
  // Navegar para qualquer endereço abre a página; o app não tem outras telas.
  const alvo = pedido.mode === "navigate" ? INICIO : pedido;
  const url = typeof alvo === "string" ? alvo : alvo.url;
  const guardado = await cache.match(alvo);
  // A rede é consultada sempre: a resposta nova entra no cache para a próxima
  // abertura. Sem cópia guardada, ela é a própria resposta.
  const rede = fetch(alvo).then((resposta) => {
    if (resposta.ok && CASCA.has(url)) cache.put(alvo, resposta.clone());
    return resposta;
  });
  if (guardado) {
    rede.catch(() => {});
    return guardado;
  }
  return rede;
}
