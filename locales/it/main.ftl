# That's it — stringhe dell'interfaccia in italiano.
# Il nome dell'app "That's it" non si traduce.

## Tagline

tagline = Trascina. Scegli. Ecco fatto.

## Categorie dell'anello

category-images = Immagini
category-audio-video = Audio e video
category-documents = Documenti
category-utilities = Utilità
category-with-count = { $name } ({ $count })

## Notifica del risultato

summary-saved = { $conversion }: salvato { $file }
summary-converted = { $conversion }: { $count ->
    [one] 1 file convertito
   *[other] { $count } file convertiti
}
summary-fallback = La cartella originale non è scrivibile, quindi il risultato è stato salvato in { $folder }.
summary-failed-one = Non riuscito: { $error }
summary-failed-many = { $count ->
    [one] 1 file non riuscito (vedi il log)
   *[other] { $count } file non riusciti (vedi il log)
}
summary-cancelled = Annullato.

## Barra dei menu

menu-preferences = Preferenze…
menu-show-widget = Mostra widget
menu-hide-widget = Nascondi widget
menu-cancel = Annulla conversione
menu-open-conversions = Apri cartella conversioni
menu-open-log = Apri cartella log
menu-about = Informazioni su That's it
menu-quit = Esci da That's it

## Finestra Preferenze

prefs-title = Preferenze di That's it
prefs-tab-general = Generali
prefs-tab-conversions = Conversioni
prefs-tab-tools = Strumenti

prefs-screen = Mostra il widget su:
prefs-position = Posizione del widget:
prefs-position-top-left = In alto a sinistra
prefs-position-top-center = In alto al centro
prefs-position-top-right = In alto a destra
prefs-position-bottom-left = In basso a sinistra
prefs-position-bottom-right = In basso a destra
prefs-position-hint = Oppure tieni premuto Option e trascina il widget.
prefs-size = Dimensione widget:
prefs-size-small = Piccolo
prefs-size-medium = Medio
prefs-size-large = Grande
prefs-login = Apri al login
prefs-login-unavailable = Disponibile quando l'app è installata.
prefs-accent = Usa il colore d'accento di sistema
prefs-summon = Modalità richiamo
prefs-summon-info = Quando finestre o un'app a schermo intero coprono la scrivania, inizia a trascinare i file e tieni premuto il tasto per mezzo secondo: accanto al puntatore compare un secondo widget, sopra tutto, che funziona allo stesso modo.
prefs-fallback = Se la cartella originale non è scrivibile, salva in:
prefs-choose = Scegli…
prefs-update = Per aggiornare That's it installato con Homebrew, esegui nel Terminale:

prefs-conversions-hint = Le conversioni non selezionate non compaiono mai nell'anello.
prefs-needs = richiede { $tools }
prefs-from = da { $formats }

prefs-tools-hint = Strumenti a riga di comando usati dalle conversioni. Le app aperte dal Finder non vedono il PATH della shell, quindi That's it cerca anche in /opt/homebrew/bin e /usr/local/bin.
prefs-tool-found = { $tool }: { $path }
prefs-tool-missing = { $tool }: non trovato
prefs-ffmpeg-hint = Installa ffmpeg per sbloccare le conversioni video e MP3: brew install ffmpeg
prefs-invalid-title = File di conversione non validi
prefs-invalid-none = Tutti i file di conversione sono validi.
prefs-rescan = Ricontrolla

## Finder service (right-click → Services)
service-no-match = Nessuna conversione per questi file
variant-default = { $label } (predefinita)
