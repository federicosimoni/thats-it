<div align="center">

<img src="resources/AppIcon.svg" width="112" alt="">

# That's it

**Convertitore di file drag & drop per macOS**

*Trascina. Scegli. Ecco fatto.*

[![CI](https://github.com/federicosimoni/thats-it/actions/workflows/ci.yml/badge.svg)](https://github.com/federicosimoni/thats-it/actions/workflows/ci.yml)
[![Licenza: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#licenza)
![macOS 13+](https://img.shields.io/badge/macOS-13%2B-black)

[English](README.md) · Italiano

<img src="docs/assets/demo.gif" width="560" alt="Una foto HEIC trascinata sul widget: si apre il ring e la foto viene convertita in JPG">

</div>

Un piccolo disco di vetro vive sulla tua scrivania. Trascinaci sopra dei file e si apre in un
anello con le conversioni adatte a quei file. Spostati su una, rilascia, e il risultato
compare accanto all'originale. Niente finestre, niente dialoghi, niente upload: tutto gira
sul tuo Mac.

## Caratteristiche

- **Solo ciò che serve.** L'anello mostra solo le conversioni valide per i file trascinati.
  Più file insieme? Vedi le conversioni valide per *tutti*, eseguite in blocco.
- **Nativa.** Scritta in Rust su AppKit e Core Animation, con il vero vetro di macOS,
  SF Symbols, modalità chiara e scura. Niente Electron, niente web view, niente icona nel Dock.
- **Rispetta i tuoi file.** I risultati vengono salvati accanto all'originale come
  `foto.jpg`, `foto (1).jpg`… L'originale non viene mai spostato, sovrascritto o cancellato.
- **Privata.** Nulla esce dal tuo Mac. Le conversioni usano gli strumenti integrati di macOS
  (`sips`, `afconvert`, `textutil`, PDFKit…) e librerie Rust.
- **Estendibile.** Ogni conversione è un piccolo file TOML. Aggiungi le tue senza scrivere
  Rust: vedi [docs/conversions.md](docs/conversions.md) (in inglese).
- **Localizzata.** Inglese e italiano, altre lingue benvenute.

## Cosa converte

| Categoria | Conversioni |
|---|---|
| **Immagini** | PNG, JPG, TIFF, BMP, GIF, WebP (lossless) in tutte le direzioni · HEIC → JPG/PNG · SVG → PNG · PNG → icona app (ICNS) · riduci al 50% · riduci a max 1920 px · immagini → PDF |
| **Audio e video** | WAV, AIFF, M4A (AAC), CAF · → MP3¹ · MOV → MP4¹ · comprimi video¹ · video → GIF¹ · estrai audio¹ |
| **Documenti** | DOCX, DOC, RTF, ODT, HTML, TXT in tutte le direzioni · questi → PDF · Markdown → HTML/PDF/DOCX · pagine PDF → PNG |
| **Utility** | comprimi in ZIP · rimuovi i metadati delle foto (EXIF/GPS) senza ricodificare · ottimizza PNG senza perdita |

¹ Richiede [ffmpeg](https://ffmpeg.org): `brew install ffmpeg`. Senza, queste conversioni
semplicemente non compaiono.

## Installazione

**Homebrew**

```sh
brew install --cask federicosimoni/tap/thats-it
```

**A mano:** scarica il `.dmg` da [Releases](https://github.com/federicosimoni/thats-it/releases),
aprilo e trascina *That's it* in Applicazioni.

Richiede macOS 13 Ventura o successivo, Apple silicon o Intel.

> [!NOTE]
> That's it non è ancora notarizzata da Apple, quindi macOS blocca il primo avvio. Apri
> **Impostazioni di Sistema → Privacy e sicurezza** e fai clic su **Apri comunque**, oppure
> esegui `xattr -dr com.apple.quarantine "/Applications/That's it.app"`.

## Come si usa

1. Trascina uno o più file dal Finder sul disco della scrivania.
2. Si apre l'anello. Spostati verso una conversione: si illumina.
3. Rilascia. Un anello di avanzamento gira intorno al disco e una notifica ti avvisa alla fine.

Per annullare, rilascia al centro dell'anello o allontanati. Con più di otto conversioni
l'anello mostra prima le categorie: fermati su una per aprirla, fermati al centro per tornare
indietro.

**Spostare il widget:** tieni premuto **Option** e trascina il disco. Si aggancia a uno di
cinque punti (in alto a sinistra, in alto al centro, in alto a destra, in basso a destra, in
basso a sinistra), sempre abbastanza lontano dai bordi perché l'anello aperto ci stia.
**Option-clic** salta al punto successivo.

**Barra dei menu:** l'icona nella barra dei menu apre le Preferenze (schermo, dimensione del
widget, apertura al login, colore di accento, conversioni visibili, strumenti trovati),
nasconde il widget, annulla una conversione in corso e apre la cartella delle tue conversioni.

Il widget sta sulla scrivania, sotto le finestre, quindi è nascosto dalle app a tutto schermo.

## Aggiungere conversioni

Metti un file `.toml` in
`~/Library/Application Support/io.github.federicosimoni.thatsit/conversions/`
(**barra dei menu → Apri cartella conversioni**). Un esempio e il riferimento completo sono
in [README.md](README.md#add-your-own-conversions) e [docs/conversions.md](docs/conversions.md).
I comandi vengono eseguiti direttamente, mai attraverso una shell.

## Contribuire

Nuove conversioni, traduzioni, segnalazioni di bug e correzioni sono tutte benvenute. Parti da
[CONTRIBUTING.md](CONTRIBUTING.md) (in inglese). Le traduzioni si fanno su
[Weblate](docs/translating.md) o direttamente in `locales/`.

## Licenza

A tua scelta, [Apache License, Version 2.0](LICENSE-APACHE) oppure
[licenza MIT](LICENSE-MIT).
