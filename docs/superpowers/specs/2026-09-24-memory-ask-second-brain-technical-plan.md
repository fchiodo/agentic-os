AgenticOS — Piano tecnico operativo per Memory, Ask e Second Brain

Destinatari: team backend Rust/Tauri, frontend React, QA e responsabile prodotto.
Data: 24 settembre 2026.
Baseline dell’analisi: repository fchiodo/agentic-os, ramo master, commit ac3a4ed4e6abebe6dec46d277bdae9387046305f.
Natura del documento: specifica proposta per l’implementazione; non certificazione di funzionalità già rilasciate. Prima di iniziare, confrontare la baseline con il nuovo HEAD e riutilizzare le correzioni eventualmente già presenti.

1. Mandato al team e risultato atteso

Evolvere il Second Brain esistente senza sostituire Markdown/Git, SQLite e la governance delle scritture. Rendere Ask accurato, comprensibile e veloce; migliorare il recupero dei documenti e la gestione della memoria nel tempo; completare la mappa operativa come strumento di navigazione e diagnosi.

Il primo risultato da consegnare è una risposta verificabile alla domanda “What are the use cases from Movable Ink?” sul documento effettivamente presente nel vault. La risposta deve elencare gli elementi documentati con riferimenti apribili, senza aggiungere casi d’uso generici del prodotto. Il numero atteso va annotato manualmente sul documento: i “14 casi” menzionati nella conversazione sono un dato da verificare, non un risultato da imporre al modello.

L’ordine di lavoro è:

1. Riprodurre e misurare il problema reale.
2. Correggere la preparazione delle evidenze e i falsi negativi della verifica.
3. Eliminare il planner obbligatorio dalle domande semplici e rendere osservabili le fasi.
4. Valutare ricerca ibrida e reranking su un benchmark rappresentativo.
5. Consolidare ciclo di vita della memoria, integrazioni agenti e mappa operativa.

Non iniziare con un rifacimento totale, un database a grafo o nuove animazioni. Una mappa più ricca non risolve una risposta scartata dal verificatore.

2. Baseline verificata e limiti dell’analisi

|Area             |Situazione alla baseline                                           |Conseguenza                                                           |
|-----------------|-------------------------------------------------------------------|----------------------------------------------------------------------|
|Persistenza      |Vault Markdown/Git e indice SQLite                                 |Architettura da mantenere                                             |
|Search           |Recupero locale lessicale sulle memorie                            |Non richiede il planner AI                                            |
|Ask              |Planner AI iniziale, retrieval, sintesi AI, verifica locale        |Almeno due turni modello nel percorso ordinario                       |
|Planner          |Sempre eseguito; timeout di 30 s                                   |Può rallentare anche domande semplici                                 |
|Evidenze         |Normalmente fino a 14 passaggi                                     |Il conteggio non prova rilevanza o completezza                        |
|Note lunghe      |Il testo per la sintesi è limitato ai primi 1.800 caratteri        |Il match trovato più avanti può non arrivare al modello               |
|Documenti        |Chunk di circa 1.500 caratteri con overlap                         |Elenchi, tabelle e sezioni possono essere spezzati                    |
|Sintesi          |Massimo 8 claim                                                    |Possibili omissioni o accorpamenti negli elenchi                      |
|Verifica         |Vincoli lessicali, ordine delle parole e split sui ritorni a capo  |Possibili false astensioni e difficoltà con parafrasi/traduzioni      |
|Ricerca semantica|Non configurata                                                    |Mancano recupero per significato e supporto multilingua semantico     |
|Alias/fuzzy      |Opzionali, disabilitati per default                                |Non considerarli automaticamente attivi nell’installazione dell’utente|
|Isolamento MCP   |Fix presente in `structured.rs`                                    |Serve un test sul processo effettivo e sui livelli di configurazione  |
|UI Ask           |Titolo “Synthesizing with evidence” durante tutta l’attesa         |Non distingue ricerca, pianificazione e sintesi                       |
|Benchmark        |Esiste una valutazione del retrieval                               |Non copre tutta la catena di risposta e verifica                      |
|Integrazione task|Il precedente bridge runner/memoria è stato rimosso nel refactoring|Il suo eventuale ripristino deve essere un requisito esplicito        |

La CI della baseline è verde. L’analisi non ha riprodotto la query sul Mac di Fabio né certificato i tempi, i token o gli esatti passaggi locali riportati nella conversazione.

3. Vincoli architetturali

3.1 Elementi da preservare

• Markdown come fonte autorevole delle memorie e Git per la cronologia.
• Originali dei documenti conservati con provenienza; testo estratto e indici come derivati versionati.
• SQLite/FTS ricostruibile, con migrazioni esplicite e nessuna dipendenza obbligatoria da servizi cloud per la ricerca lessicale.
• Sei domini esistenti e filtri di accesso applicati nel backend.
• Gate deterministico, proposte, approvazioni e tracciabilità delle scritture.
• Autenticazione e policy aziendali del provider modello.

3.2 Separazione delle responsabilità

Separare logicamente: ingestione, normalizzazione, indicizzazione, retrieval, selezione delle evidenze, sintesi, verifica e presentazione. Estrarre moduli dal file retrieval.rs solo quando necessario ai ticket, evitando un refactoring esteso prima dei test.

Il modello propone query o risposte. Il backend decide quali fonti sono accessibili, quale budget applicare, quali citazioni esistono e quali scritture sono consentite. I contenuti recuperati rimangono dati non attendibili come istruzioni; la sola etichetta nel prompt non costituisce una barriera di sicurezza completa.

3.3 Configurazione effettiva

Introdurre una configurazione tipizzata del percorso Ask, accessibile in diagnostica: versione pipeline, modalità planner, disponibilità backend semantico, reranker, budget, modello e versioni di prompt/verificatore. Riutilizzare la configurazione esistente dove possibile. Mostrare separatamente valore configurato, disponibilità e utilizzo nell’ultima richiesta.

4. Contratti dati proposti

I nomi seguenti definiscono contratti logici, non obbligano a introdurre tutte le tabelle o a duplicare strutture esistenti. Usare tipi Rust, serializzazione coerente e schema Zod corrispondente. Versionare i payload IPC modificati.

4.1 Documento e blocchi strutturali

type CanonicalBlock = {
  blockId: string;
  sourceId: string;
  sourceRevision: string;       // hash del contenuto sorgente
  kind: "heading" | "paragraph" | "list_item" | "table_row" | "caption";
  parentId?: string;
  headingPath: string[];
  text: string;
  page?: number;                // omettere se non disponibile con affidabilità
  originalLocator?: string;    // riferimento al layout/originale, se disponibile
  extractionVersion: string;
  normalizationVersion: string;
};

Conservare l’ordine dei blocchi, gli identificatori di lista/tabella e gli header necessari a interpretare ogni riga. Un documento privo di paginazione non deve ricevere numeri di pagina inventati.

4.2 Evidenza fornita al modello

type EvidencePassage = {
  evidenceId: string;
  sourceId: string;
  sourceRevision: string;
  domain: string;
  title: string;
  headingPath: string[];
  blockIds: string[];
  text: string;
  locator: { page?: number; section?: string };
  status: "active" | "stale" | "superseded";
  sensitivity: string;          // usare l'enum di policy già esistente
  retrieval: {
    channels: string[];
    ranks: Record<string, number>;
    score?: number;
  };
};

Titolo e intestazioni utilizzati per sostenere un’affermazione devono essere anch’essi verificabili. Non è sufficiente che il modello li abbia visti se il verificatore poi considera soltanto text.

4.3 Claim e citazioni

type EvidenceSpan = {
  evidenceId: string;
  blockId: string;
  startByte: number;             // offset UTF-8 sul blocco canonico revisionato
  endByte: number;               // esclusivo, su confine UTF-8 valido
  quote: string;
};

type AnswerClaim = {
  id: string;
  text: string;
  evidence: EvidenceSpan[];
  mode: "extractive" | "paraphrase";
};

Il modello può restituire blockId e quote; il backend risolve e valida gli offset. Se la citazione è ambigua o non esiste, non accettare offset inventati. Gli offset si riferiscono al testo canonico, non automaticamente ai byte del PDF: la mappatura al documento originale è un dato separato.

4.4 Risultato Ask e copertura

type AskResult = {
  requestId: string;
  status: "answered" | "partial" | "extracts_only"
    | "insufficient_evidence" | "failed" | "cancelled";
  claims: AnswerClaim[];
  sources: EvidencePassage[];
  gaps: string[];
  coverage: {
    scope: "identified_section" | "selected_sources" | "unknown";
    completeness: "complete_for_scope" | "partial" | "unknown";
  };
  diagnosticId?: string;
};

answered non equivale a “conoscenza completa del vault”. Usare complete_for_scope soltanto quando l’ambito è delimitato e ne è stata verificata la copertura. Non mostrare percentuali di confidenza derivate direttamente dal punteggio BM25 o dal numero di fonti.

5. Ingestione e indicizzazione: recuperare il contenuto giusto

5.1 Normalizzazione rispettosa del layout

Intervenire sui percorsi reali di importazione: il convertitore documenti con OCR e l’importazione diretta PDF della Memory non vanno presunti equivalenti.

1. Conservare originale, hash, estrattore e versione.
2. Produrre blocchi canonici per paragrafi, elenchi e tabelle.
3. Ricomporre le righe soltanto quando appartengono allo stesso blocco.
4. Preservare separazioni fra bullet, titoli, colonne e celle.
5. Gestire i trattini di fine riga con regole conservative; non alterare codici, nomi o importi.
6. Registrare qualità ed eventuali limiti dell’estrazione. Proporre OCR quando il testo manca o è inutilizzabile, senza reinterpretare arbitrariamente i dati.
7. Mantenerne una versione ricostruibile: i miglioramenti al normalizzatore non aggiornano automaticamente le importazioni già esistenti.

Non effettuare una sostituzione globale di tutti i ritorni a capo con spazi. Non dividere frasi su ogni punto senza proteggere decimali e abbreviazioni.

5.2 Chunk strutturali e contesto parent

• Indicizzare unità figlie sufficientemente piccole per trovare il match e riferimenti alle sezioni parent per ricostruirne il contesto.
• Usare come punto iniziale sperimentale 300–700 token per chunk; preservare un bullet breve intero e dividere i blocchi troppo lunghi in modo esplicito. Questi valori sono parametri da tarare, non standard universali.
• Includere titolo e percorso delle intestazioni nella rappresentazione di retrieval; non aggiungere descrizioni sintetiche non tracciate come se fossero testo originale.
• Per le tabelle, mantenere associazione header/cella, unità e qualificatori.
• Per domande di elenco, espandere la sezione contenente l’elenco entro il budget. Se non entra, procedere per sezioni o dichiarare una risposta parziale.
• Deduplicare la sovrapposizione, preservando elementi simili ma distinti.

5.3 Note lunghe

Sostituire il passaggio “primi 1.800 caratteri” con finestre ancorate ai match o con blocchi indicizzati delle note. Usare intestazioni e frasi adiacenti per rendere il passaggio interpretabile. Il verificatore deve controllare esattamente l’evidenza inviata al modello, nella stessa revisione.

5.4 Indice e concorrenza

• Spostare il backfill ordinario fuori dal percorso sincrono di Ask.
• Usare aggiornamenti incrementali per hash e un job di ricostruzione interrompibile.
• Preparare una nuova generazione dell’indice e attivarla solo dopo validazione; conservare la precedente fino al completamento del controllo.
• Se l’indice è incompleto, indicarlo in UI: una ricerca parziale non deve apparire come una ricerca completa senza risultati.
• Durante una richiesta mantenere una revisione coerente delle evidenze. Se una fonte cambia, la citazione deve riferirsi allo snapshot o segnalare la revisione superata.
• Non eseguire transazioni DB lunghe durante una chiamata al modello.

Accettazione: match recuperabile in fondo a una nota; un bullet spezzato su più righe rimane un’unità; una tabella conserva significato; reindex non modifica Markdown né proposte; interruzione e ripresa non producono duplicati.

6. Retrieval adattivo: prima locale, AI quando serve

6.1 Percorso veloce

1. Validare dominio, policy, stato delle memorie e preferenze dell’utente.
2. Normalizzare Unicode/spazi e preservare nomi propri, numeri e frasi tra virgolette.
3. Eseguire ricerca titolo/frase esatta e FTS/BM25 su note e documenti autorizzati.
4. Applicare alias espliciti e versionati quando disponibili; mantenerne traccia.
5. Deduplicare, selezionare passaggi e valutare copertura della domanda.
6. Se sufficiente, passare direttamente alla sintesi senza planner.

Una domanda come quella su Movable Ink deve poter raggiungere il documento tramite nome, titolo e testo. Non richiedere un embedding per un’entità che il motore lessicale trova già.

6.2 Quando attivare l’espansione

Usare segnali congiunti, calibrati sul benchmark: presenza dell’entità richiesta, accordo fra titolo e contenuto, sezione coerente con l’intento, copertura dei sottoquesiti, ambiguità fra fonti, lingua, stato dell’indice. Non considerare “almeno N risultati” una prova di sufficienza e non usare un valore BM25 assoluto come probabilità.

Attivare un planner quando il recupero è insufficiente, l’entità è ambigua, esiste un mismatch linguistico non risolto o la domanda richiede più sottoquesiti. Se l’ambiguità cambia sostanzialmente la risposta, chiedere chiarimento anziché espandere senza limite.

Budget iniziale proposto: una sola espansione, fino a tre nuove query, tempo massimo del planner di 8–10 secondi, configurabile. Al timeout usare i risultati già disponibili. Evitare query generiche di marketing se l’utente ha chiesto i casi documentati nel proprio vault.

6.3 Fusione e ordinamento

• Evitare di sommare direttamente punteggi eterogenei di note, chunk, fuzzy e vettori.
• Sperimentare Reciprocal Rank Fusion su graduatorie deduplicate; un punto di partenza è sum(weight / (60 + rank)), con rank a partire da 1. Tarare pesi e costante sul set di sviluppo.
• Non attribuire voti indipendenti a query duplicate o quasi identiche: normalizzare il contributo delle espansioni per non premiare artificiosamente i documenti generici.
• Applicare diversificazione quando la domanda richiede più fonti; consentire più blocchi della stessa fonte quando servono a ricostruire un elenco.
• Recency e trust non devono far prevalere automaticamente una nota recente irrilevante. Gestire validità temporale e supersessione prima di usarle come tie-breaker o boost calibrato.
• I filtri di dominio, sensibilità e stato devono valere per ogni canale, espansione del grafo, reranker e contesto parent.

6.4 Budget di contesto

Definire budget in token con il tokenizer disponibile, oppure con una stima dichiarata e conservativa. Proposta iniziale: fino a 8.000 token di evidenze nel percorso ordinario e 12.000 per estrazioni esaustive. Riservare separatamente spazio per istruzioni e output; ricalibrare sui costi e sul modello realmente utilizzato.

Eliminare il limite rigido di 14 passaggi come unica politica. Mantenere comunque un tetto operativo di passaggi/byte per proteggere il processo. Non leggere l’intero vault e non introdurre cicli agentici illimitati.

6.5 Ricerca ibrida: fase successiva misurata

Introdurre un’interfaccia opzionale per embeddings multilingua e ricerca vettoriale. Selezionare modello, runtime e storage dopo aver misurato qualità IT/EN, licenza, dimensioni, RAM, velocità su Mac Apple Silicon e packaging Tauri. Non scegliere il modello soltanto dalla classifica pubblica.

L’indice vettoriale è un derivato: memorizzare modello, versione, dimensione, hash dei blocchi e generazione indice. Un cambio modello richiede reindicizzazione e invalidazione delle cache. La ricerca lessicale deve funzionare quando il backend semantico non è disponibile.

Un eventuale reranker opera su un pool limitato e autorizzato, prima della sintesi. Non introdurlo se il beneficio sul test set non giustifica latenza e risorse. Qualunque servizio remoto per embeddings/reranking deve rispettare le policy già applicate ai documenti e i provider autorizzati.

Accettazione: planner non chiamato sulle fixture semplici adeguatamente coperte; espansione limitata e osservabile; nessun risultato di dominio escluso; fallback lessicale funzionante; nessuna dichiarazione di completezza basata sul solo numero di chunk.

7. Sintesi e verifica delle risposte

7.1 Output atomico e copertura

• Un caso d’uso per claim; separare descrizione, prerequisito e beneficio se richiedono evidenze differenti.
• Rispondere nella lingua dell’utente, mantenendo nomi propri e citazioni originali consultabili.
• Dimensionare il limite dei claim rispetto all’intento e all’elenco identificato. Portare 8 a 16 può essere una mitigazione transitoria, non il limite definitivo.
• Applicare un limite configurabile di output e, se necessario, continuazione paginata o risposta parziale esplicita. Non fondere più elementi soltanto per rientrare nel limite.
• Conservare qualificatori, condizioni, attribuzioni e modalità: “proposto”, “possibile” e “già disponibile” non sono intercambiabili.
• Segnalare fonti in conflitto senza risolverle arbitrariamente.

7.2 Verifica su due livelli

Livello deterministico, sempre obbligatorio: validità schema, ID esistenti, revisione della fonte, accesso, integrità degli estratti e limiti di output. Controllare numeri, unità, entità e negazioni come segnali di contraddizione; non pretendere identità lessicale globale per una parafrasi.

Livello semantico, quando necessario: per claim parafrasati o tradotti, valutare supporto, contraddizione o evidenza insufficiente rispetto agli estratti citati. Utilizzare un verificatore NLI compatibile con le lingue richieste o un turno modello separato e limitato. Il verificatore non deve cercare nel web o usare conoscenza esterna.

Il controllo semantico non è una garanzia matematica di verità. Se si usa lo stesso modello della sintesi, misurare gli errori correlati; anche un modello diverso va valutato. Elaborare più claim in un batch per limitare la latenza. Non rendere obbligatorio un ulteriore turno generativo per una risposta interamente estrattiva e strutturalmente verificabile.

L’esatta presenza di una citazione prova che il testo esiste, non che sia pertinente alla domanda o che sostenga ogni inferenza. Anche il percorso estrattivo deve preservare contesto, condizioni e attribuzione.

7.3 Esito e fallback

• Tutti gli elementi richiesti supportati nell’ambito identificato: answered.
• Solo parte degli elementi supportata: partial, con lacune esplicite.
• Sintesi non validabile ma passaggi pertinenti disponibili: extracts_only.
• Nessuna evidenza sufficiente: insufficient_evidence, con suggerimento mirato per restringere o identificare la fonte.
• Provider, autenticazione o processo falliti: failed, distinto dall’assenza di evidenza.
• Interruzione utente: cancelled.

Una riparazione della risposta può essere ammessa una sola volta e solo se ha uno scopo esplicito, ad esempio correggere una citazione inesistente. Non ripetere automaticamente l’intera pipeline senza un budget residuo.

7.4 Diagnostica dei claim

Registrare codici come unknown_source, source_revision_mismatch, quote_not_found, ambiguous_quote, numeric_conflict, contradicted, insufficient_support, claim_too_broad, output_truncated. Conservare separatamente bozze e claim accettati solo in diagnostica locale autorizzata, con retention e redazione dei dati sensibili.

Accettazione: parafrasi supportata e traduzione IT/EN non rifiutate per il solo ordine delle parole; negazioni e soggetti invertiti respinti; estratti apribili; nessun claim senza riferimento; nessuna equivalenza fra errore del provider e fonti insufficienti.

8. Esecuzione Codex, tempi e cancellazione

Consolidare il fix di structured.rs senza eliminare autenticazione, provider o policy aziendali.

1. Definire un profilo minimo per planner, sintesi e verifica: nessun MCP, app o plugin non necessario e nessuna capacità di scrittura nel vault.
2. Verificare le opzioni supportate dalla versione installata di Codex. Se la CLI non consente di eliminare tutti gli strumenti built-in, documentare il limite e usare, dove compatibile con autenticazione e policy, un’interfaccia di generazione strutturata senza strumenti. Non simulare l’isolamento tramite il solo prompt.
3. Testare configurazioni utente, profili e altre sorgenti di configurazione applicabili. Usare fixture senza credenziali reali. Un server MCP sentinella deve poter attestare che non è stato contattato.
4. Gestire esplicitamente errore di lettura/parsing della configurazione: non dichiarare un isolamento riuscito se non è stato verificato.
5. Imporre una deadline complessiva e budget per fase. Valori iniziali proposti: planner 8–10 s, sintesi 45 s, verifica semantica eventuale 15 s, totale 75 s; configurabili per il provider aziendale e da tarare sulla baseline. Sono limiti operativi proposti, non prestazioni garantite.
6. Al raggiungimento della deadline, terminare il processo e i discendenti posseduti dalla richiesta; restituire evidenze disponibili e stato corretto.
7. Lo Stop dell’utente deve interrompere processi e lavoro pendente senza chiudere richieste indipendenti. Ignorare eventi tardivi mediante requestId e stato terminale.
8. Un eventuale retry per errore transitorio deve rispettare deadline, cancellazione e numero massimo di tentativi. Non riprovare automaticamente un errore di autenticazione.

Separare stdout strutturato da stderr diagnostico; non mostrare in UI header di autenticazione, URL sensibili o log grezzi contenenti dati riservati.

9. UI Search/Ask e osservabilità

9.1 Stati ed eventi

Esporre eventi IPC tipizzati con requestId, numero di sequenza, timestamp monotono o durate backend e payload limitato:

retrieval_started
sources_available
expansion_started       (solo quando necessaria)
evidence_ready
synthesis_started
verification_started
answer_ready | request_failed | request_cancelled

La sequenza può includere una seconda ricerca dopo l’espansione. La UI deve utilizzare la fase effettiva, non dedurla da una stringa di log. Distinguere “processo avviato”, “richiesta modello inviata” e “risposta ricevuta” solo quando tali eventi sono realmente disponibili.

9.2 Esperienza utente richiesta

• Mostrare subito l’avvio della ricerca e rendere disponibili le fonti prima della sintesi.
• Visualizzare titolo, sezione/pagina se disponibile, estratto, dominio e stato della fonte.
• Separare contenuti verificati ed eventuali bozze; non presentare testo in streaming non verificato come risposta definitiva.
• Rendere cliccabili le citazioni e aprire il passaggio della revisione corretta.
• Per una risposta parziale indicare cosa è coperto e cosa manca.
• In assenza di una sintesi valida mantenere consultabili le fonti pertinenti autorizzate.
• Rendere visibile quando Search sta cercando solo nelle memorie oppure anche nei documenti. Se si estende Search ai documenti, introdurre filtri Note/Documenti/Tutti e validare la compatibilità dell’IPC.
• Distinguere tempo totale, fase corrente e fonti candidate. Evitare il messaggio “14 relevant passages” quando la pertinenza non è ancora stata valutata: usare “14 passaggi candidati”.
• Aggiungere un pannello diagnostico copiabile con ID della richiesta e versioni, senza testo sensibile per default.

9.3 Trace minima

Registrare: versione app e pipeline, configurazione effettiva non segreta, revisione indice, query originali/espansioni secondo policy, ID/hash fonti, ranks per canale, blocchi selezionati, ragione dell’espansione, durata di ogni fase, chiamate modello, token input/output/cached quando disponibili, numero di claim accettati/scartati e motivazioni, deadline/cancellazione.

Non trasformare i token in costo monetario senza pricing applicabile e distinzione della cache. Se il provider non espone un dato, marcarlo come non disponibile. Usare trace metadata-only per default; snapshot completi del caso di errore solo con un’impostazione locale esplicita e retention breve.

10. Memoria persistente e integrazione con gli agenti

Questa fase consolida il Second Brain come sistema operativo quotidiano, oltre alla ricerca di documenti.

10.1 Fatti, episodi e decisioni

• Mantenere separazione fra fonte documentale, episodio di task e memoria distillata.
• Un riepilogo generato non deve diventare automaticamente una verità confermata: conservare provenienza e natura inferita.
• Gestire aggiornamenti e supersessioni con collegamenti espliciti e approvazioni già previste.
• Per risposte sul presente preferire informazioni attive; per domande storiche permettere l’accesso alle revisioni indicando la data. Non cancellare una fonte solo perché non è più attuale.
• TTL indica necessità di revisione o archiviazione secondo policy; non prova falsità.
• Non incrementare trust o conferme soltanto perché una memoria viene recuperata o ripetuta dal modello.
• Se una risposta viene salvata, collegarla alle fonti originali e impedirne il rafforzamento circolare: una sintesi non deve citare soltanto una precedente sintesi derivata dalle stesse fonti.
• Trattare cancellazioni e cambi di sensibilità in tutti i derivati: FTS, vettori, cache e snapshot diagnostici secondo retention. Distinguere cancellazione operativa dalla conservazione nella storia Git, senza promettere rimozioni che Git non effettua automaticamente.

10.2 Bridge agenti: requisito condizionato ma esplicito

Alla baseline il vecchio collegamento runner/memory non è presente. Prima di ricostruirlo individuare il runner effettivamente utilizzato dal prodotto attuale. Se l’integrazione resta richiesta:

• Esporre un servizio backend riutilizzabile get_task_context(goal, domain, budget, policy) con evidenze tracciate e budget esplicito.
• Riutilizzare retrieval e filtri di Ask, senza avviare una sintesi della risposta per preparare ogni task.
• Registrare quali memorie e revisioni sono state fornite al task.
• A fine task proporre un episodio o una memoria candidata tramite lo stesso gate di scrittura; rispettare le approvazioni esistenti.
• Usare un identificatore idempotente del task per evitare duplicazioni nei retry.
• Distillare skill soltanto su azione prevista dal prodotto e con revisione; non trasformare ogni risposta in una nuova skill.

Accettazione: nessuna scrittura diretta degli agenti nel vault; nessun aumento artificiale di trust; un task ripetuto non genera episodi duplicati; una decisione superata non viene presentata come attuale.

11. Mappa operativa e animazione

Questa è una fase separata dal fix Ask. Usare l’estetica orbitale della reference per rendere leggibili i livelli, con dati e relazioni reali. Non rappresentare come attive capacità soltanto dichiarate.

11.1 Modello e classificazione

Distinguere almeno runtime, skill, memoria/documento, routine, applicazione/connettore e progetto. Se il modello usa cartelle o repository come nodi, non contarli automaticamente fra le applicazioni connesse. Distinguere skill disponibili, abilitate e osservate in esecuzione.

Ogni relazione deve avere tipo, origine, revisione o timestamp, stato dichiarato/osservato/inferito e livello di confidenza quando inferita. Mostrare, ad esempio, uses, reads, produces, scheduled_by, supersedes e registered_in; non trasformare la semplice appartenenza all’inventario in un uso operativo.

11.2 Layout proposto

• Centro: runtime AgenticOS.
• Fascia interna: skill aggregate per famiglia o progetto.
• Fascia memoria: sei settori di dominio, espandibili in fonti e memorie.
• Fascia routine: automazioni realmente configurate, con stato e ultimo esito.
• Fascia esterna: applicazioni e connettori, con stato di connessione e capacità autorizzate.

Le fasce indicano categorie e non autorizzazioni implicite. I collegamenti fra categorie compaiono su selezione, ricerca o focus, evitando centinaia di raggi sempre visibili.

11.3 Interazione e motion

• Pan, zoom, ricerca, breadcrumb e ripristino della vista.
• Aggregazione per dominio/famiglia a zoom basso; singoli elementi solo quando lo spazio lo consente.
• Posizioni stabili basate sugli ID; un refresh non deve rimescolare l’intera mappa.
• Selezione di un nodo: evidenziare vicini e relazioni, aprire un inspector con provenienza, stato e azioni consentite.
• Selezione di una risposta Ask: evidenziare esclusivamente le fonti utilizzate e le citazioni accettate.
• Transizioni brevi, inizialmente 150–300 ms, per apertura dei gruppi e cambio focus. Nessuna rotazione continua necessaria per leggere i dati.
• Animare flussi soltanto quando esistono eventi reali. Un replay deve essere etichettato come replay; una demo come demo.
• Rispettare prefers-reduced-motion, tastiera, focus visibile e alternativa tabellare. Non affidare il significato al solo colore.
• Filtri su dati sensibili applicati dal backend, non soltanto nascondendo nodi già inviati al frontend.

11.4 Prestazioni

Misurare prima il renderer esistente. Evitare una migrazione automatica a WebGL: adottare Canvas/WebGL se le misure di scala lo richiedono. Precalcolare o spostare in worker il layout costoso; evitare un ciclo fisico continuo quando le posizioni sono già definite.

Fixture proposte: 1.000, 10.000 e 35.000 elementi di inventario, rappresentati inizialmente tramite aggregazione. Registrare dimensione totale e numero di nodi effettivamente renderizzati.

Obiettivi iniziali sul Mac di riferimento: vista aggregata interattiva entro 2 s dopo la disponibilità dei dati; risposta alla selezione p95 entro 100 ms; frame p95 entro 33 ms durante pan/zoom. Misurare separatamente cold start, lettura dati, layout e rendering. Queste soglie vanno confermate sul dispositivo target, non dichiarate raggiunte senza misurazione.

12. Test e benchmark necessari

12.1 Dataset

Preparare circa 60 domande iniziali annotate, divise per documento/progetto fra sviluppo e holdout, evitando che frammenti dello stesso documento finiscano in entrambi. Aggiungere fixture sintetiche per errori strutturali e mantenere riservato il documento Movable Ink reale se non può essere incluso nel repository.

Per ogni domanda registrare: intento, dominio, lingua, rispondibilità, fonti/revisioni attese, elementi richiesti, citazioni accettabili, varianti lecite e condizioni di risposta parziale. Versionare dataset, normalizzatore, configurazione e modello. Non usare la risposta del modello come unica annotazione di verità.

12.2 Matrice minima dei casi

|Caso                                 |Cosa deve essere verificato                                        |
|-------------------------------------|-------------------------------------------------------------------|
|Movable Ink EN                       |Elenco corretto, fonte reale, nessuna aggiunta generica            |
|Stessa domanda IT                    |Recupero della stessa evidenza e risposta tradotta supportata      |
|Bullet su più righe                  |Ricomposizione senza fusione con bullet vicini                     |
|Elenco oltre 16 elementi             |Nessun troncamento silenzioso; continuazione o parzialità esplicita|
|Nota con match in fondo              |Il passaggio utile arriva alla sintesi                             |
|Titolo con entità, corpo senza entità|Provenienza del titolo utilizzabile e verificabile                 |
|Tabella e PDF multicolonna           |Relazioni fra righe, colonne e header preservate                   |
|Numeri, decimali e unità             |Nessuna separazione o trasformazione che cambi significato         |
|Negazione/soggetto invertito         |Claim errato respinto                                              |
|Due fonti complementari              |Claim atomici sostenuti dalle fonti necessarie                     |
|Fonti in conflitto                   |Conflitto esplicito e date/provenienze visibili                    |
|Informazione assente                 |Astensione corretta, non risposta inventata                        |
|Fonte stale/superseded               |Comportamento coerente con domanda attuale o storica               |
|Dominio escluso/sensibilità          |Nessuna esposizione in risultati, prompt, cache o mappa            |
|Prompt injection nel documento       |Nessuna azione o cambio di istruzioni indotto dalla fonte          |
|MCP con autenticazione mancante      |Nessun contatto nei turni interni isolati                          |
|Provider lento o indisponibile       |Deadline, stato tecnico e fonti preservate                         |
|Stop e richieste concorrenti         |Cancellazione corretta; eventi non mescolati                       |
|Indice incompleto o in rebuild       |Stato dichiarato e nessuna perdita delle fonti                     |
|Modifica fonte durante Ask           |Citazione revisionata e coerente                                   |
|Cache dopo modifica dei permessi     |Nessun riuso di dati non più autorizzati                           |

12.3 Metriche

Misurare separatamente:

• Retrieval: hit/recall@k, MRR e copertura degli elementi dell’elenco nelle evidenze selezionate.
• Risposta: precisione dei claim supportati, copertura degli elementi attesi e correttezza/completa copertura delle citazioni.
• Astensione: falsi rifiuti sulle domande rispondibili e risposte non supportate sulle non rispondibili.
• Prestazioni: p50/p95 per fase e totale, cold/warm, quantità di dati, chiamate modello e token distinti.
• Operatività: cancellazioni riuscite, timeout, errori di provider, stato dell’indice e isolamenti MCP falliti.

Non confondere copertura del retrieval con copertura della risposta: sono misure diverse. Le valutazioni automatiche semantiche vanno campionate e controllate manualmente, soprattutto quando decidono l’accettazione del rilascio.

12.4 Gate di rilascio proposti

Queste soglie sono obiettivi iniziali di prodotto da confermare sulla baseline, non risultati già dimostrati né standard di letteratura.

• Tutte le fixture deterministiche critiche passano: citazioni, filtri, negazioni note, cancellazione, migrazione e isolamento.
• Caso Movable Ink: copertura completa dell’elenco manualmente annotato nel documento, oppure parzialità esplicita motivata da un limite reale; zero elementi inventati. Almeno cinque esecuzioni live sullo stesso snapshot per osservare la variabilità.
• Test set iniziale: almeno 95% dei claim giudicati supportati; false astensioni non oltre 5% delle domande rispondibili. Riportare sempre conteggi assoluti, dimensione del campione e incertezza; un set piccolo non certifica tassi di errore in produzione.
• Nessuna regressione sui casi avversariali di supporto e nessuna fuga di dominio nelle fixture. “Zero nel test” non equivale a rischio zero nel mondo reale.
• Percorso semplice: zero chiamate planner nei casi di accettazione coperti localmente. Sintesi unica nel percorso estrattivo; eventuale verifica semantica conteggiata separatamente.
• Su hardware di riferimento, ricerca lessicale locale warm p95 target entro 500 ms su corpus concordato; fonti visibili entro 1 s, esclusi cold start e ricostruzione indice. Misurare corpus, hardware e distribuzione query.
• Risposta totale: misurare e migliorare p50/p95 rispetto alla baseline; nessun target assoluto attribuito al provider senza misure. Ogni richiesta deve terminare entro la deadline configurata o per cancellazione.

12.5 Livelli di esecuzione

• CI ordinaria: test Rust, frontend, build e contratti sidecar già presenti; aggiungere fixture deterministiche pertinenti e integrazioni con processi finti.
• Test live controllati: modello reale, autenticazione aziendale e vault di test. Separarli dalla CI che non possiede credenziali e dai test riproducibili.
• Valutazione manuale prima del rilascio: fonte originale, citazioni cliccabili, query IT/EN e tempi percepiti sulla build desktop Tauri. I mock del browser non dimostrano il funzionamento del vault reale.

13. Cache, migrazioni e rilascio

13.1 Cache

Introdurre cache solo dopo il percorso corretto. Chiavi: domanda normalizzata, dominio, filtri, revisione indice/fonti, policy di accesso, configurazione retrieval, versione prompt e modello quando rilevante. Una cache di risposta richiede anche invalidazione dei claim al cambiamento delle fonti. La cache non deve mascherare i tempi del percorso cold nei benchmark.

13.2 Migrazione dell’indice

1. Inventario degli schemi, backup verificato e stato iniziale documentato.
2. Migrazione additiva delle strutture necessarie, riutilizzando tabelle e campi già presenti.
3. Ricostruzione derivati in una nuova generazione, a lotti e con progresso.
4. Verifica di conteggi, hash, accessi e retrieval campionato.
5. Attivazione atomica della generazione e invalidazione cache.
6. Conservazione temporanea della generazione precedente per rollback.

Se non esiste l’originale o non è possibile riallineare i locator, mantenere l’estrazione precedente e segnalarne il limite. Non riscrivere massivamente le memorie curate o le proposte per farle aderire al nuovo chunking.

13.3 Flag di rollout proposti

Usare nomi coerenti con la configurazione del progetto; i seguenti sono esempi:

• ask_pipeline_v2
• structured_chunks_v2
• adaptive_query_planning
• semantic_retrieval
• evidence_verification_v2
• operational_map_v2

Evitare combinazioni incompatibili: validare la configurazione e versionare il comportamento. Nuovo retrieval e vecchio indice devono avere una compatibilità esplicita, non implicita.

13.4 Sequenza di rilascio

• R0: diagnostica e test del problema, senza cambiare l’output.
• R1: selezione passaggi, normalizzazione/verifica e stati di risposta; correggere Movable Ink.
• R2: planner adattivo, deadline, isolamento testato e UI delle fasi.
• R3: esperimento ibrido/reranker attivabile; promozione solo se supera il confronto.
• R4: ciclo di memoria, eventuale bridge agenti e mappa operativa migliorata.

Prima di R1/R2 eseguire valutazioni comparative offline o in ambiente di test. Non duplicare silenziosamente le richieste live dell’utente per fare esperimenti a costo doppio.

Rollback tramite flag e indice precedente, preservando dati e audit. Un problema nel verificatore nuovo non autorizza ad accettare risposte senza verifiche: tornare al percorso estrattivo o al comportamento precedente con il limite dichiarato.

14. Backlog implementabile e dipendenze

|ID   |Priorità    |Attività e deliverable                                              |Owner prevalente  |Dipendenze                          |
|-----|------------|--------------------------------------------------------------------|------------------|------------------------------------|
|SB-01|P0          |Baseline riproducibile, fixture Movable Ink, trace per fase e claim |Backend + QA      |Nessuna                             |
|SB-02|P0          |Isolamento reale dei turni interni, deadline e cancellazione        |Backend           |SB-01                               |
|SB-03|P0          |Blocchi canonici, reflow conservativo, locator e versioni           |Backend/ingestione|SB-01                               |
|SB-04|P0          |Chunk strutturali, note lunghe e selezione parent/child             |Backend           |SB-03                               |
|SB-05|P0          |Claim atomici, citazioni puntuali, verificatore e fallback          |Backend + QA      |SB-01, SB-03; integrazione con SB-04|
|SB-06|P0          |Stati IPC/UI, fonti anticipate, risposte parziali ed errori distinti|Frontend + backend|Contratti di SB-01/SB-05            |
|SB-07|P1          |Retrieval locale prima del planner, fusione e budget adattivi       |Backend           |SB-01, SB-04, SB-05                 |
|SB-08|P1          |Benchmark end-to-end, report comparativo e gate release             |QA + backend      |Inizia con SB-01; blocca R1/R2      |
|SB-09|P1          |Migrazione indice, aggiornamento incrementale, cache sicura         |Backend           |SB-03, SB-04; cache dopo SB-07      |
|SB-10|P2          |Esperimento embeddings multilingua e reranker                       |Backend           |SB-07, SB-08, SB-09                 |
|SB-11|P1/P2       |Validità temporale, supersessione e prevenzione dei cicli di sintesi|Backend + prodotto|SB-05, SB-08                        |
|SB-12|Condizionale|Bridge al runner effettivo e cattura task governata                 |Backend + prodotto|Decisione sul runner, SB-07, SB-11  |
|SB-13|P2          |Tassonomia della mappa, stati e provenienza delle relazioni         |Backend + frontend|Dati e contratti reali inventario   |
|SB-14|P2          |Layout orbitale, aggregazione, motion, accessibilità e performance  |Frontend + QA     |SB-13; fonti Ask da SB-06           |

Alcuni ticket possono procedere in parallelo dopo la definizione dei contratti; le dipendenze indicano integrazione e gate, non obbligano a lavorare in modo seriale su ogni riga.

14.1 File di partenza e moduli suggeriti

|File esistente alla baseline                   |Intervento                                                                      |
|-----------------------------------------------|--------------------------------------------------------------------------------|
|`src-tauri/src/memory/retrieval.rs`            |Orchestrazione Ask, planner condizionale, selezione evidenze, ranking e verifica|
|`src-tauri/src/memory/index.rs`                |Chunking, metadati strutturali, revisioni e generazioni indice                  |
|`src-tauri/src/memory/importer.rs`             |Allineamento ingestione, provenienza e reindicizzazione                         |
|`src-tauri/src/memory/pdf_extraction.rs`       |Preservazione struttura e distinzione dei percorsi estrattivi                   |
|`src-tauri/src/document_converter/canonical.rs`|Collegamento al formato canonico comune, senza duplicare l’estrazione           |
|`src-tauri/src/harness/structured.rs`          |Isolamento, gestione processi, deadline e metriche modello                      |
|`src-tauri/src/commands.rs`                    |Contratti IPC e cancellazione                                                   |
|`src/features/memory/hooks.ts`                 |Consumo eventi, richieste concorrenti, invalidazione                            |
|`src/features/memory/memory-page.tsx`          |Stati, evidenze, citazioni e diagnostica                                        |

Possibili nuovi moduli, solo se utili a separare responsabilità testabili: ask_pipeline, evidence, verification, query_planner, evaluation. I percorsi dei componenti della mappa vanno individuati sul nuovo HEAD prima di aprire i ticket; non assumerli dalla sola schermata.

14.2 Pianificazione indicativa

Con un backend engineer, un frontend engineer e QA disponibile: primo incremento P0 nell’ordine di 1–2 settimane; planner adattivo e consolidamento nella settimana successiva; esperimento semantico e mappa in ulteriori incrementi separati. È una stima di pianificazione, non un impegno: disponibilità delle fixture, qualità dei PDF e runtime aziendale possono modificarla. Ristimare dopo SB-01; con un solo dev ridurre il lavoro parallelo.

15. Definition of Done complessiva

☐ Il caso Movable Ink è riprodotto sul desktop con fonte originale e risultato atteso annotato.
☐ Il report distingue cosa è stato trovato, passato al modello, generato e scartato.
☐ Note lunghe, bullet e tabelle producono evidenze utilizzabili.
☐ La risposta non dipende da un limite fisso di otto o sedici elementi.
☐ Claim e citazioni sono revisionati e verificabili; i limiti della verifica semantica sono documentati.
☐ Le domande semplici coperte localmente saltano il planner.
☐ I turni interni non contattano MCP estranei; il test verifica il comportamento effettivo.
☐ Stop, timeout e errori provider hanno esiti distinti e non lasciano processi orfani.
☐ La UI mostra fase reale, fonti anticipate e lacune della risposta.
☐ Il benchmark comprende l’intero Ask e misura anche false astensioni e copertura.
☐ Le migrazioni sono riprendibili e il rollback è provato su fixture.
☐ Le policy di dominio/sensibilità valgono in ogni canale e cache.
☐ La mappa distingue inventario, disponibilità e uso osservato; non genera attività fittizia.
☐ È dichiarato esplicitamente se il bridge automatico task/memory è presente oppure fuori dal rilascio.
☐ Le verifiche desktop reali sono separate da CI, mock e benchmark sintetici.

Alla consegna di ogni release il team deve fornire: commit e PR, flag attivi, migrazioni, test eseguiti, tabella prima/dopo su stesso dataset, limiti residui e istruzioni di avvio/verifica sulla build corretta. Non dichiarare “completato” un requisito soltanto perché esistono il componente UI o il relativo test mock.

16. Riferimenti e basi delle decisioni

Le fonti seguenti sostengono principi di progettazione, non i budget, le soglie o le stime specifiche proposte in questo documento.

• Baseline codice — commit ac3a4ed.
• CI della baseline.
• Adaptive-RAG, NAACL 2024: adattare il percorso alla complessità della domanda.
• Anthropic, Contextual Retrieval: complementarità fra lessicale, semantica, contesto e reranking; necessità di valutazione sul proprio corpus.
• ALCE, EMNLP 2023: distinguere qualità della risposta e supporto delle citazioni.
• Anthropic, Effective context engineering for AI agents: contesto selezionato e compromessi fra recupero anticipato ed esplorazione al bisogno.
• LongMemEval-V2, preprint maggio 2026: valutazione della memoria su aggiornamenti, stato e vincoli oltre il recupero statico. È un preprint, non uno standard consolidato.
• Configurazione Codex: livelli di configurazione da considerare nel verificare l’isolamento effettivo.

Istruzione conclusiva per il team: iniziare da SB-01 e consegnare il primo incremento con il caso reale corretto e misurato. Ogni nuova componente deve risolvere un limite osservato o un requisito esplicito; la ricerca semantica e la visualizzazione avanzata vengono promosse solo dopo aver dimostrato utilità e correttezza.