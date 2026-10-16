// Casca do Tostão: guarda a página e o núcleo em WebAssembly para abrir sem
// rede. O orçamento em si fica no IndexedDB (ver app.js), não aqui.
//
// A casca é uma versão inteira: todos os arquivos de PRECACHE são guardados
// juntos, num cache com o nome da versão, e só então a versão nova passa a
// responder. Assim uma abertura nunca mistura arquivos de builds diferentes.
// `construir.ps1` troca a linha VERSAO a cada build, pelo hash dos arquivos da
// casca: não edite à mão. Os testes em tests/casca.rs conferem esta lista com
// os arquivos de web/.
const VERSAO = "dev";
const CACHE = `tostao-casca-${VERSAO}`;
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
  evento.waitUntil(instalar());
});

async function instalar() {
  // Busca tudo antes de gravar, ignorando o cache HTTP do navegador
  // (`no-cache`): senão a busca pode devolver a versão antiga que o
  // GitHub Pages deixou guardada por max-age=600. Um arquivo que falta
  // derruba a instalação inteira; a versão anterior continua valendo.
  try {
    const buscados = await Promise.all(
      [...CASCA].map(async (url) => {
        const resposta = await fetch(url, { cache: "no-cache" });
        if (!resposta.ok) throw new Error(`${url}: ${resposta.status}`);
        return [url, resposta];
      })
    );
    const cache = await caches.open(CACHE);
    await Promise.all(buscados.map(([url, resposta]) => cache.put(url, resposta)));
  } catch (e) {
    // Sem a casca inteira, não sobra cache parcial com este nome.
    await caches.delete(CACHE);
    throw e;
  }
  // Só chega aqui com a versão inteira guardada.
  await self.skipWaiting();
}

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
  // Navegar para qualquer endereço abre a página; o app não tem outras telas.
  const alvo = pedido.mode === "navigate" ? INICIO : pedido.url;
  const cache = await caches.open(CACHE);
  // Tudo que é da casca sai deste cache, todos da mesma versão. O que não é
  // da casca segue direto para a rede.
  const guardado = CASCA.has(alvo) ? await cache.match(alvo) : undefined;
  return guardado ?? fetch(pedido);
}
