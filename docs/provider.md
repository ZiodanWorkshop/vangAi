# Catalogo provider

`harbor run` legge un catalogo TOML e chiama i provider in catena. L'unico protocollo accettato è `openai-chat`: `POST {base_url}/chat/completions`.

Il file di lavoro è `harbor.toml` (gitignored). Il modello committato è [`harbor.toml.example`](../harbor.toml.example): Ollama su `http://127.0.0.1:11434/v1` con modello `llama3.2`, e OpenAI su `https://api.openai.com/v1` con `api_key_env = "OPENAI_API_KEY"`. Non mettere segreti nel TOML né nella documentazione. La chiave sta solo nella variabile d'ambiente nominata da `api_key_env`.

```bash
cp harbor.toml.example harbor.toml
export OPENAI_API_KEY="..."   # solo nell'ambiente, mai nel file
cargo run -p harbor -- run "ciao"
cargo run -p harbor -- run --config harbor.toml.example "ciao"
```

## Campi

```toml
default = "ollama"
fallback = ["ollama", "openai"]

[[providers]]
id = "ollama"
protocol = "openai-chat"
base_url = "http://127.0.0.1:11434/v1"
model = "llama3.2"
context_window = 8192
```

| Campo | Ruolo |
|-------|--------|
| `default` | Id di un provider presente nel file. Obbligatorio. Se `fallback` è vuoto o assente, la catena è solo questo id |
| `fallback` | Lista ordinata di id da provare. Se non è vuota, sostituisce `default`: `default` non viene aggiunto da solo. Id sconosciuti o ripetuti rendono il file non valido |
| `providers` | Almeno un provider. `id` non vuoto, univoco, senza `/` |
| `protocol` | Solo `"openai-chat"` |
| `base_url` | Origine più eventuale prefisso (`http` o `https`), senza il suffisso `chat/completions` e senza utente o password nell'URL |
| `model` | Nome modello inviato nel JSON. Non vuoto. L'id logico è `provider/model` (esempio `openai/gpt-4.1-mini`); sul filo parte solo `model` |
| `api_key_env` | Opzionale. Nome della variabile d'ambiente con il bearer token. Se è presente non può essere vuoto. Se è assente, la richiesta non ha header `Authorization` |
| `context_window` | Opzionale, in token. Viene solo conservato. La CLI non lo invia e non lo applica |

Un TOML che non rispetta queste regole produce `invalid config: ...` e exit 1. Un path inesistente produce `missing config: ...` e exit 1. In entrambi i casi non c'è traffico di rete.

## Chiamata `openai-chat`

Per ogni provider il client aggiunge `chat/completions` a `base_url` (inserisce `/` se manca) e fa `POST` con un JSON di questa forma:

```json
{
  "model": "llama3.2",
  "messages": [{ "role": "user", "content": "ciao" }]
}
```

`role` è `system`, `user` o `assistant`. Il corpo non include tool, `stream` né parametri di sampling. `harbor run` manda un solo messaggio `user`.

Il client HTTP non segue i redirect, così un bearer non lascia l'host del catalogo. Timeout della richiesta: 60 secondi. Timeout di connessione: 5 secondi.

Dalla risposta usa la prima `choice`: `message.content` (testo, vuoto se assente), gli eventuali `tool_calls` (`id`, `function.name`, `function.arguments`) e `usage.prompt_tokens` / `usage.completion_tokens`. La CLI stampa solo il testo. Non c'è un loop che esegua i tool. Una risposta 2xx che non è una chat completion ferma la catena (`provider <id> returned an unexpected response`).

Ogni client della catena invia il proprio `model`, non il nome del provider precedente.

## Chiavi

`api_key_env` è solo un nome. Il valore si legge dall'ambiente al momento della chiamata, non quando si parsa il TOML. Non viene scritto negli errori né nel `Debug` del client.

Se la variabile manca o è vuota, l'errore è `missing api key: NOME` e la catena si ferma, senza aprire la connessione. HTTP 401 e 403 sono `provider <id> unauthorized` e fermano la catena: una chiave rifiutata non viene riprovata sul provider successivo.

## Fallback

`FallbackInference` prova i client di `chain_ids()` in ordine. Il primo successo viene restituito.

Si passa al provider successivo solo per `InferError::Retryable`: timeout, errore di connessione, HTTP 429, HTTP 5xx, e le altre risposte non di successo che non sono 4xx. L'ultimo errore della catena è quello che vede la CLI (exit 1).

La catena si ferma, senza chiamare i provider dopo, in questi casi:

- chiave mancante o vuota
- HTTP 400 e gli altri 4xx diversi da 429 (`provider <id> rejected the request`)
- HTTP 401 e 403
- corpo 2xx non interpretabile come chat completion

Non esiste un protocollo Anthropic nativo. Un endpoint Anthropic va usato qui solo se parla già Chat Completions su `base_url`; questo repository non lo implementa.
