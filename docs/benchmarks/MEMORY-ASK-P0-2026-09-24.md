# Memory Ask P0 — verifica R0/R1

Data: 24 settembre 2026  
Baseline della specifica: `ac3a4ed4e6abebe6dec46d277bdae9387046305f`  
HEAD remoto sincronizzato prima della verifica: `3af0fdd5a58e4f3e4fc00434ae71b3206d42861f`  
Branch di completamento: `fix/memory-ask-p0-completion`

## Esito

L'incremento deterministico SB-01/R0-R1 è verificato: trace metadata-only, ricomposizione conservativa dei bullet, claim atomici, limite transitorio a 16 elementi e avviso esplicito di parzialità sono coperti da test.

Il gate live Movable Ink **non è stato eseguito su questo Mac**. Il database desktop disponibile contiene `0` righe in `document_imports` e `0` in `document_chunks`; non è presente una lista attesa annotata né il documento Movable Ink. In assenza della fonte reale non sono stati inventati elementi, risultati, token o latenze. La release non deve essere dichiarata validata sul caso reale finché i cinque run controllati non sono completati sul Mac che contiene il vault autorizzato.

## Delta verificato

| Aspetto | Baseline | Incremento P0 |
|---|---|---|
| Diagnostica Ask | Nessun esito stabile per singolo claim | Codici `accepted`, `unknown_source`, `insufficient_support` e altri, senza testo delle bozze |
| Provenienza retrieval | Non raccolta in una trace dedicata | ID evidenza, percorso, tipo fonte, posizione chunk e score nell'audit append-only |
| Elenchi con hard wrap | Il verificatore poteva spezzare l'elemento | Reflow entro il singolo bullet e unione dei soli chunk citati, consecutivi e della stessa fonte |
| Elementi adiacenti | Rischio di fusione durante la normalizzazione | Boundary dei bullet conservato e test avversariale dedicato |
| Limite claim | 8 | 16, come mitigazione transitoria |
| Elenchi | Il prompt poteva accorpare più elementi | Un elemento sorgente per claim; vietata la fusione per rientrare nel limite |
| Overflow | Omissione non sufficientemente esplicita | `outputTruncated: true` e avviso di risposta parziale oltre 16 claim |
| Test live | Riepilogo minimo | JSON per run con copertura, fonti, decisioni, chiamate modello, token disponibili e latenze per fase |

Il confronto descrive il comportamento e i contratti testati. Non è un confronto statistico live sul documento privato.

## Verifiche riproducibili eseguite

Ambiente di riferimento:

- MacBook Air, Apple M2, 8 GB RAM;
- macOS 26.4.1 (build 25E253);
- Rust/Cargo 1.98.1;
- Node.js 25.2.1;
- pnpm 11.25.0.

| Verifica | Risultato |
|---|---|
| `cargo test --manifest-path src-tauri/Cargo.toml` | 116 passati, 0 falliti, 2 ignorati |
| `pnpm vitest run` | 33 passati, 0 falliti |
| `pnpm lint` | superato |
| `pnpm build` | superato; Vite segnala chunk oltre 500 kB, incluso il bundle della mappa |
| `pnpm check:native` | superato |
| `git diff --check` | superato |

I due test Rust ignorati richiedono risorse esterne o locali non adatte alla CI; uno è `live_ask_covers_expected_list_items`.

Copertura deterministica rilevante:

- 14 elementi sintetici supportati rimangono 14 claim distinti;
- il diciassettesimo claim attiva troncamento e parzialità esplicita;
- le righe spezzate vengono ricomposte solo entro lo stesso bullet;
- due bullet adiacenti non vengono fusi;
- due chunk vengono ricomposti solo se citati, consecutivi e appartenenti alla stessa fonte;
- restano verdi i casi di negazione, numeri/date, inversione dei soggetti, attribuzione, modalità e citazioni non valide;
- la trace audit contiene metadati e codici decisionali, non il testo delle bozze rifiutate né il testo integrale delle evidenze.

## Caso reale Movable Ink

Domanda:

> What are the use cases from Movable Ink?

| Run | Elementi attesi | Coperti | Claim accettati/rifiutati | Citazione sorgente | Planner / sintesi / totale | Token | Esito |
|---:|---:|---:|---|---|---|---|---|
| 1 | non disponibile | non eseguito | non eseguito | non verificata | non misurato | non disponibile | bloccato: fonte assente |
| 2 | non disponibile | non eseguito | non eseguito | non verificata | non misurato | non disponibile | bloccato: fonte assente |
| 3 | non disponibile | non eseguito | non eseguito | non verificata | non misurato | non disponibile | bloccato: fonte assente |
| 4 | non disponibile | non eseguito | non eseguito | non verificata | non misurato | non disponibile | bloccato: fonte assente |
| 5 | non disponibile | non eseguito | non eseguito | non verificata | non misurato | non disponibile | bloccato: fonte assente |

Prima dell'esecuzione, una persona deve aprire il documento originale, annotare un'intestazione univoca per ogni caso d'uso in `output/movable-ink-expected-items.txt` e verificare il percorso della sorgente importata. Il numero non deve essere fissato a 14 in anticipo.

Esecuzione controllata:

```bash
AGENTIC_OS_LIVE_DB="$HOME/Library/Application Support/com.fchiodo.agentcontrol/agent-control.db" \
AGENTIC_OS_LIVE_EXPECTED_ITEMS="$PWD/output/movable-ink-expected-items.txt" \
AGENTIC_OS_LIVE_EXPECTED_SOURCE="_sources/work/<percorso-reale>.md" \
AGENTIC_OS_LIVE_RUNS=5 \
cargo test --manifest-path src-tauri/Cargo.toml \
  memory::retrieval::tests::live_ask_covers_expected_list_items \
  -- --ignored --nocapture
```

Ogni riga `LIVE_MEMORY_ASK=...` espone solo metadati:

- elementi attesi e coperti;
- conteggio di evidenze, citazioni e fonti;
- claim grezzi, accettati e rifiutati;
- flag di troncamento;
- numero di chiamate modello;
- token di planning, sintesi, retry e totale quando il provider li rende disponibili;
- latenze di planning, sintesi, retry e Ask totale;
- versione della pipeline.

Il test fallisce se la lista attesa è vuota, se Ask si astiene, se manca un elemento annotato o se nessuna citazione punta alla sorgente attesa.

## Limiti residui

- Il planner è ancora obbligatorio e mantiene il timeout attuale di 30 secondi; il percorso locale-first adattivo appartiene a R2.
- Il pool ordinario resta limitato a 14 passaggi e ogni passaggio a 1.800 caratteri. Il fix P0 migliora la verifica degli elenchi ma non sostituisce chunk strutturali e budget a token.
- Il limite di 16 claim è transitorio: oltre tale soglia la risposta è esplicitamente parziale, non paginata.
- Non è configurato un backend semantico e non è stata misurata una ricerca ibrida.
- Il controllo live confronta intestazioni annotate con la risposta inglese normalizzata; non sostituisce una revisione umana della fedeltà di ciascun claim.
- Le prestazioni e la variabilità del provider aziendale restano non misurate su questo snapshot.
- Questo incremento non modifica planner adattivo, embeddings, ciclo persistente della memoria, bridge del runner o mappa operativa.

## Gate ancora aperto

Importare o rendere disponibile il documento reale nel vault autorizzato, annotare manualmente gli elementi, eseguire cinque run sullo stesso snapshot e sostituire la tabella “Caso reale Movable Ink” con i valori JSON prodotti dal test. Solo dopo questa verifica si può dichiarare completato il gate live SB-01.
