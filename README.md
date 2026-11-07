# Harbor (vangAi)

Agent harness leggero in Rust, orientato a **Linux** per isolamento (Landlock / seccomp in evoluzione).

## Struttura

| Path | Ruolo |
|------|--------|
| `crates/harbor-core` | Tipi condivisi, messaggi, id di sessione e workspace |
| `crates/harbor-infer` | Catalogo provider e client Chat Completions |
| `crates/harbor-sandbox-linux` | Backend sandbox per Linux; su altri OS espone API stub |
| `apps/harbor` | Binario principale |

## Requisiti

- Rust stable (vedi `rust-toolchain.toml`)

## Sviluppo

```bash
cargo build
cargo test --workspace
cargo run -p harbor -- info
cargo run -p harbor -- run --config harbor.toml.example "ciao"
```

`harbor run` legge un catalogo TOML (default `harbor.toml`). Copia [`harbor.toml.example`](harbor.toml.example). La chiave, se il provider la richiede, sta nella variabile d'ambiente nominata da `api_key_env` e non nel file.

Su **macOS** / Windows il crate `harbor-sandbox-linux` compila in modalità stub (`UnsupportedPlatform`). Lo sviluppo della sandbox reale avviene su Linux o in CI `ubuntu-latest`.
