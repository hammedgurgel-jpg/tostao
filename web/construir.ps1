# Gera web/pkg: o núcleo do Tostão em WebAssembly, para abrir web/index.html.
# Precisa do target wasm32-unknown-unknown e do CLI wasm-bindgen 0.2.129
# (cargo install wasm-bindgen-cli --version 0.2.129 --locked).
$ErrorActionPreference = 'Stop'
$raiz = Split-Path -Parent $PSScriptRoot
Set-Location $raiz
# cdylib só para este build: o `.wasm` que o wasm-bindgen converte.
cargo rustc --target wasm32-unknown-unknown --lib --no-default-features --features web --release --crate-type cdylib
wasm-bindgen --target web --out-dir web/pkg target/wasm32-unknown-unknown/release/tostao.wasm
