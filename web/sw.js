// Casca do Tostão: guarda a página e o núcleo em WebAssembly para abrir sem
// rede. O orçamento em si fica no IndexedDB (ver app.js), não aqui.
//
// Ao mudar algum arquivo da casca, suba a versão de CACHE: é ela que faz o
// navegador trocar os arquivos guardados (a estratégia é "primeiro o cache").
// Os testes em tests/casca.rs conferem esta lista com os arquivos de web/.
const CACHE = "tostao-casca-v1";
const PRECACHE = [
  "index.html",
  "app.js",
  "manifest.webmanifest",
  "icons/icon-192.png",
  "icons/icon-512.png",
  "pkg/tostao.js",
  "pkg/tostao_bg.wasm",
];

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
  const guardado = await cache.match(pedido.mode === "navigate" ? "index.html" : pedido);
  if (guardado) return guardado;
  const resposta = await fetch(pedido);
  if (resposta.ok) cache.put(pedido, resposta.clone());
  return resposta;
}
