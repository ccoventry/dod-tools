# web-analyzer

`analysis/` compiled to `wasm32-unknown-unknown`, with a static vanilla-JS
frontend (`www/`) that runs the demo parser and analytics entirely in the
browser — drop a `.dem` file in, no server-side upload. Deployed to GitHub
Pages on every push to `main` that touches `web-analyzer/`, `analysis/`,
`dod/`, or `dem-patch/` (`.github/workflows/deploy_web.yml`).

## Build and serve locally

    rustup target add wasm32-unknown-unknown
    cargo install wasm-bindgen-cli --version 0.2.126 --locked
    cargo build -p web-analyzer --target wasm32-unknown-unknown --release
    wasm-bindgen --target web --out-dir web-analyzer/www/pkg \
      target/wasm32-unknown-unknown/release/web_analyzer.wasm

`www/pkg/` is generated output (gitignored) — re-run `wasm-bindgen` after any
`src/lib.rs` change. Then serve `www/` with any static file server (opening
`index.html` directly via `file://` won't work — the wasm module needs to be
fetched over HTTP):

    npx serve web-analyzer/www

The `wasm-bindgen` crate dependency and CLI version **must match exactly** —
a mismatch fails at the bindgen step. Bump both together (see the pinning
comment in `Cargo.toml`).
