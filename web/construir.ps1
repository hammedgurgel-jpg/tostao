# Gera web/pkg: o núcleo do Tostão em WebAssembly, para abrir web/index.html.
# Precisa do target wasm32-unknown-unknown e do CLI wasm-bindgen 0.2.129
# (cargo install wasm-bindgen-cli --version 0.2.129 --locked).
#
# Depois, grava a versão da casca em web/sw.js: um hash dos arquivos que o
# service worker guarda (PRECACHE). Muda só quando algum deles muda, e é isso
# que faz o navegador instalar a casca nova inteira. Não edite a linha VERSAO.
$ErrorActionPreference = 'Stop'
$raiz = Split-Path -Parent $PSScriptRoot
Set-Location $raiz
# cdylib só para este build: o `.wasm` que o wasm-bindgen converte.
cargo rustc --target wasm32-unknown-unknown --lib --no-default-features --features web --release --crate-type cdylib
wasm-bindgen --target web --out-dir web/pkg target/wasm32-unknown-unknown/release/tostao.wasm

$sw = Join-Path $raiz 'web/sw.js'
$codigo = [IO.File]::ReadAllText($sw, [Text.Encoding]::UTF8)
$inicio = $codigo.IndexOf('const PRECACHE = [')
$fim = $codigo.IndexOf('];', $inicio)
if ($inicio -lt 0 -or $fim -lt 0) { throw "PRECACHE não encontrado em web/sw.js" }
$caminhos = [regex]::Matches($codigo.Substring($inicio, $fim - $inicio), '"([^"]+)"') | ForEach-Object { $_.Groups[1].Value }

$linhas = foreach ($caminho in $caminhos) {
  $arquivo = Join-Path $raiz (Join-Path 'web' $caminho)
  if (-not (Test-Path -LiteralPath $arquivo -PathType Leaf)) { throw "Falta web/${caminho}: o build não gerou o arquivo" }
  "$caminho $((Get-FileHash -Algorithm SHA256 -LiteralPath $arquivo).Hash)"
}
$bytes = [Text.Encoding]::UTF8.GetBytes(($linhas -join "`n"))
$hash = [System.Security.Cryptography.SHA256]::Create().ComputeHash($bytes)
$versao = ([BitConverter]::ToString($hash) -replace '-', '').Substring(0, 12).ToLower()

$novo = [regex]::Replace($codigo, 'const VERSAO = "[^"]*";', "const VERSAO = `"$versao`";", 1)
if ($novo -eq $codigo -and $codigo -notmatch [regex]::Escape("const VERSAO = `"$versao`";")) { throw "const VERSAO não encontrado em web/sw.js" }
[IO.File]::WriteAllText($sw, $novo, (New-Object Text.UTF8Encoding $false))
Write-Host "Versão da casca: $versao"
