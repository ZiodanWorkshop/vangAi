# Harbor (vangAi)

Harness di agent in Rust. Oggi il binario `harbor` fa due cose: stampa un workspace e una sessione appena generati, e invia un singolo messaggio utente a una catena di provider compatibili con OpenAI Chat Completions.

Non è ancora un agent. Non esegue tool, non isola i processi, non tiene sessioni su disco.

Licenza dei crate: `MIT OR Apache-2.0` (edition 2021, `rust-version` 1.74). Il testo MIT è in [`LICENSE`](LICENSE). Toolchain: `stable`, vedi [`rust-toolchain.toml`](rust-toolchain.toml).

## Cosa c'è adesso

- `harbor info` stampa versione, un `WorkspaceId`, un `SessionId` e la piattaforma. Gli id sono 16 byte da `getrandom`, mostrati come `ws-` o `sess-` più 32 cifre esadecimali. Ogni invocazione ne genera di nuovi: non sono persistiti.
- `harbor run "testo"` carica un catalogo TOML, chiama i provider in ordine e stampa il testo dell'assistente. Un prompt vuoto esce con codice 1.
- Il catalogo di default è `harbor.toml` nella directory di lavoro. `--config` ne indica un altro. File assente o TOML non valido: exit 1, nessuna richiesta di rete.
- Se il provider in uso ha `api_key_env` e la variabile manca o è vuota, il processo esce con 1 e non apre la connessione.
- Su timeout, errore di connessione, HTTP 429 o 5xx la catena prova il provider successivo. HTTP 400, 401 e 403 fermano la catena. Vale anche per gli altri 4xx diversi da 429, per un corpo di risposta non parsabile e per una chiave mancante.

Il protocollo, i campi del catalogo e le regole di fallback sono in [`docs/provider.md`](docs/provider.md).

## Cosa non c'è

Queste parti non sono implementate. Non vanno trattate come funzioni disponibili:

- loop dell'agent e esecuzione di tool (`read_file`, `list_dir` o altri)
- streaming della risposta
- sessioni SQLite
- canali e dashboard
- API nativa Anthropic
- seccomp
- confinamento Landlock funzionante

`Completion` può contenere `tool_calls` se il provider li manda nel JSON. La CLI stampa solo il testo e non li esegue. `context_window` nel TOML è metadato: non viene inviato e non viene applicato.

## Crate

| Path | Ruolo |
|------|--------|
| `crates/harbor-core` | `WorkspaceId` e `SessionId` non indovinabili, `Message` / `Role`, `Completion` / `ToolCall` / `Usage`, `InferError` |
| `crates/harbor-infer` | Lettura di `harbor.toml`, client OpenAI Chat Completions, catena di fallback. `ScriptedInference` è un doppio per i test, non un comando |
| `crates/harbor-sandbox-linux` | Su Linux, bozza Landlock che non compila in modo pulito contro `landlock` 0.4. Fuori da Linux, stub che rifiuta sempre (`UnsupportedPlatform`, `landlock_supported=false`) |
| `apps/harbor` | Binario `harbor` 0.1.0 |

`harbor run` non applica una sandbox. Il crate Linux non confina il processo: `linux.rs` è un MVP incompleto (controllo ABI e una regola di sola lettura sotto un path) e non compila in modo pulito con `landlock` 0.4. Lo stub, usato su macOS e Windows, rifiuta `Sandbox::new` e `apply_read_only_beneath`. Non esiste un workflow CI in questo repository.

## Build e test

Serve Rust stable con `rust-version` almeno 1.74.

```bash
cargo build
cargo test --workspace
```

Su Linux `cargo build` compila `crates/harbor-sandbox-linux/src/linux.rs`, che non compila in modo pulito contro `landlock` 0.4. Sugli altri sistemi operativi il crate compila in modalità stub.

## Uso

`harbor.toml` è in [`.gitignore`](.gitignore). Si parte dalla copia di esempio (Ollama in locale e OpenAI tramite la variabile `OPENAI_API_KEY`; nessuna chiave è scritta nel file):

```bash
cp harbor.toml.example harbor.toml
cargo run -p harbor -- info
cargo run -p harbor -- run "ciao"
```

Per provare il file di esempio senza copiarlo:

```bash
cargo run -p harbor -- run --config harbor.toml.example "ciao"
```

Il primo provider di [`harbor.toml.example`](harbor.toml.example) è Ollama su `http://127.0.0.1:11434/v1`, modello `llama3.2`. Se Ollama fallisce in modo ritentabile, la catena chiama OpenAI. Senza `OPENAI_API_KEY` quella chiamata non esce in rete e il comando esce con 1. Il valore della chiave non va messo nel TOML.

I log usano `tracing`. Il filtro di default è `info`; si sovrascrive con `RUST_LOG`.
