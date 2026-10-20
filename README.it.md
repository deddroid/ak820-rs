# ak820 — configuratore AJAZZ AK820 Pro per Linux e Windows

[English](README.md) · **Italiano**

## Download

Programmi già pronti, senza installare Rust: scarica l'ultima versione dalla
[**pagina Releases**](https://github.com/deddroid/ak820-rs/releases/latest).

- **Windows**: scarica `ak820-…-windows-x86_64.zip`, estrailo e avvia `ak820-gui.exe`.
- **Linux** (x86_64): scarica `ak820-…-linux-x86_64.tar.gz`, estrailo e lancia
  `./install.sh --udev` dentro la cartella (installa in `~/.local` e aggiunge la regola udev;
  vedi [Linux: permessi](#linux-permessi)).

Un unico programma in Rust per configurare la AJAZZ AK820 Pro senza il software ufficiale:

- **Luci**: tutti i 20 effetti, colore, arcobaleno, luminosità, velocità, direzione (gli effetti che si muovono hanno due direzioni)
- **Colori per tasto** (salvati nella tastiera) e animazione live di prova
- **Rimappatura tasti** su layer normale e Fn: tasti, scorciatoie, media, mouse, Multi-Key
- **Macro** salvate nella tastiera (testo, combinazioni, pause, clic)
- **Display** 128×128: immagini e GIF animate
- **Orologio** del display sincronizzato col PC
- **Impostazioni**: sleep, modalità Fn, antirimbalzo, blocco Win / Alt+F4 / Alt+Tab
- Funziona via **cavo USB** e, per luci/tasti/macro/orologio/impostazioni, anche via **ricevitore 2.4 GHz**
- **Profili TOML**: tutta la configurazione in un file, applicabile con un comando

Le modifiche sono salvate nella tastiera stessa: valgono su qualsiasi computer, senza
programmi in esecuzione.

Disponibile sia come **interfaccia grafica** (`ak820-gui`) sia da **riga di comando** (`ak820`):
condividono la stessa configurazione salvata.

![Rimappatura tasti](docs/screenshots/3-keys.png)

| Luci | Display |
|---|---|
| ![Luci](docs/screenshots/1-lighting.png) | ![Display](docs/screenshots/5-screen.png) |

| Profili | Sistema |
|---|---|
| ![Profili](docs/screenshots/7-profiles.png) | ![Sistema](docs/screenshots/6-system.png) |

## Installazione

### Arch Linux (AUR)

```bash
yay -S ak820-bin   # binari pronti
yay -S ak820       # compilato dai sorgenti
```

Il pacchetto installa anche la regola udev, quindi la tastiera funziona senza root (scollegala
e ricollegala una volta dopo l'installazione).

### Dai sorgenti

Per compilare dai sorgenti serve [Rust](https://rustup.rs). Su Linux, il modo più semplice:

```bash
./install.sh            # compila e installa in ~/.local (aggiunge "AK820 Pro" al menu applicazioni)
./install.sh --udev     # come sopra, più la regola udev per i permessi (chiede la password)
./install.sh --uninstall
```

Oppure compila a mano:

```bash
cargo build --release
```

Produce due eseguibili in `target/release/`:

- `ak820-gui` (`ak820-gui.exe` su Windows): interfaccia grafica
- `ak820` (`ak820.exe`): riga di comando

Per compilare solo la CLI, senza dipendenze grafiche: `cargo build --release --no-default-features`.

### Linux: permessi

I device `hidraw` sono accessibili solo a root. Dalla GUI basta premere **Concedi accesso**
nella barra in basso (chiede la password di amministratore e installa la regola udev).
In alternativa, da terminale:

```bash
sudo cp udev/70-ajazz-ak820.rules /etc/udev/rules.d/ && sudo udevadm control --reload && sudo udevadm trigger
```

poi scollega e ricollega la tastiera. Serve `libudev` (presente su tutte le distro con systemd).

### Windows

Nessun driver richiesto: compila con `cargo build --release` (toolchain MSVC) e avvia
`ak820-gui.exe` (l'icona è incorporata nell'eseguibile). Chiudi il software ufficiale AJAZZ
se è aperto.

## Interfaccia grafica

```bash
ak820-gui
```

Schede: **Luci** (effetti e parametri), **Colori tasti** (dipingi i tasti sulla tastiera
disegnata, clic destro per cancellare), **Tasti** (clicca un tasto e scegli cosa deve fare,
layer normale e Fn), **Macro** (editor di sequenze), **Display** (anteprima animata; in modalità
ritaglia trascina l'immagine per scegliere la parte visibile e usa la rotella per lo zoom;
trascina un'immagine o una GIF nella finestra), **Sistema** (impostazioni dell'app, orologio,
sospensione, Fn, antirimbalzo, batteria) e **Profili** (profili salvati, importa/esporta/reinvia,
ripristino di fabbrica). La barra in basso mostra lo stato del collegamento e il registro delle
operazioni.

I **profili salvati** contengono tutta la configurazione: luci o colori per tasto, rimappature,
macro, immagine del display (copiata accanto al profilo) e impostazioni. **Applica** porta la
tastiera a quel profilo con un clic, anche dal menu della tray.

**Sistema → Applicazione**:

- **Lingua**: italiano o inglese. Di default segue la lingua del sistema
  (`LANG`/`LC_ALL`/`LC_MESSAGES` su Linux, la lingua dell'utente su Windows);
  `AK820_GUI_LANG=en|it` la forza per un singolo avvio.
- **Avvia all'accensione del PC**: all'accesso l'app parte ridotta nella tray (autostart XDG su
  Linux, chiave `Run` del registro su Windows).
- **Riduci nella tray** alla chiusura della finestra: un clic sull'icona la riapre, il suo menu
  cambia profilo o chiude l'app. Su GNOME serve l'estensione AppIndicator.
- **Sincronizza l'orologio del display** all'avvio dell'app.

Le preferenze sono salvate in `gui.toml`, nella stessa cartella del file di stato. L'app gira in
una sola copia: se la riapri, si mostra la finestra già aperta.

Su Linux la finestra si apre tramite XWayland quando è disponibile, perché winit (la libreria
delle finestre) non supporta il drag & drop di file su Wayland nativo; `AK820_WAYLAND=1` mantiene
Wayland nativo (in quel caso usa "Scegli immagine" o incolla un file copiato con Ctrl+V).

## Riga di comando

```bash
ak820 list                                   # interfacce HID trovate
ak820 keys                                   # nomi dei tasti, target ed effetti

ak820 light rolling --rainbow -b 5 -s 3      # effetto onda arcobaleno
ak820 light rolling -c 00aaff -d left        # onda verso sinistra
ak820 light static -c ff6600 -b 4            # colore fisso
ak820 light off

ak820 colors --base 000020 w=red a=red s=red d=red   # colori per tasto
ak820 demo --seconds 10                      # animazione live (non salvata)

ak820 clock                                  # sincronizza l'orologio
ak820 screen gatto.gif --fit crop            # immagine/GIF sul display
ak820 screen foto.jpg --zoom 2 --x 0.3 --y 0.4    # ritaglio: zoom 2×, centrato al 30% / 40%

ak820 system --sleep 30m --fn-mode hold --response 2 --block-win off

ak820 remap capslock=ctrl pgdn=end           # rimappa
ak820 remap --fn left=prev right=next down=play
ak820 remap delete=text:ciao                 # macro che scrive "ciao"
ak820 macro set copia "tap:ctrl+c" "delay:50" "tap:ctrl+v"
ak820 remap insert=macro:copia
ak820 remap pgdn=default                     # torna al tasto originale
ak820 remap --reset                          # layer normale di fabbrica
ak820 remap                                  # mostra le rimappature

ak820 profile save gaming                    # salva la configurazione attuale come "gaming"
ak820 profile apply gaming                   # porta la tastiera a quel profilo
ak820 profile list                           # profili salvati (anche: profile rm NOME)

ak820 apply examples/profile.toml            # applica un file profilo
ak820 export mio-profilo.toml                # salva la configurazione corrente
ak820 resend                                 # reinvia tutto (es. dopo un reset)
ak820 reset --yes                            # impostazioni di fabbrica
ak820 battery                                # batteria (solo via ricevitore 2.4G)
```

Direzioni: `right`/`left` per rolling, rotating, flowing e tilt; `down`/`up` per scrolling.
Gli altri effetti non hanno direzione.

### Target per la rimappatura

| Target | Esempio |
|---|---|
| tasto | `b`, `f13`, `home`, `printscreen` |
| scorciatoia / modificatore | `ctrl+shift+t`, `win+e`, `ctrl` |
| media | `play` `next` `prev` `stop` `mute` `vol+` `vol-` `bright+` `bright-` `calc` `mail` … |
| mouse | `mouse:left` `mouse:right` `mouse:middle` `wheel:up` `wheel:down` |
| più tasti insieme | `multi:a+s+d` |
| macro | `text:Ciao!`, `macro:NOME` |
| originale | `default` |

Passi delle macro: `text:…`, `tap:ctrl+c`, `down:KEY`, `up:KEY`, `delay:MS`,
`click:left`, `mouse-down:left`, `mouse-up:left`. Modalità: `once`, `repeat` (con `--count`),
`toggle`, `hold`, `step`. Il testo usa il layout USA (lettere accentate non supportate).

### Stato

La tastiera non permette di rileggere la configurazione, quindi `ak820` ricorda l'ultima
inviata in `~/.config/ak820/state.toml` (Windows: `%APPDATA%\ak820\state.toml`) e ogni
comando modifica solo la parte interessata. Si può cambiare il percorso con `AK820_STATE`.
`AK820_DEBUG=1` stampa i pacchetti scambiati.

## Limiti noti

- Bluetooth: la tastiera non espone un'interfaccia di configurazione.
- Colori per tasto e display: solo via cavo.
- Nessuna lettura della configurazione dalla tastiera (limite del firmware).
- Firmware che espongono `0xFF67` invece di `0xFF13` usano un altro protocollo, non supportato.

## Libreria

`src/lib.rs` espone tutto come crate (`ak820::Keyboard`, `ak820::Profile`, …) per chi
vuole costruirci sopra una GUI o un demone. Il protocollo è documentato in
[docs/PROTOCOL.it.md](docs/PROTOCOL.it.md).

## Licenza

[MIT](LICENSE).

## Crediti

Codice scritto da [Claude](https://claude.com/claude-code) (Anthropic) insieme a
[deddroid](https://github.com/deddroid), che lo ha testato sulla tastiera.

Protocollo consolidato da questi progetti (grazie!):

- [gha-projects/ak820-mac](https://github.com/gha-projects/ak820-mac) — analisi di `DeviceDriver.exe`: tasti, macro, sistema, ricevitore 2.4G, checksum
- [hcristosm/ak820-linux](https://github.com/hcristosm/ak820-linux) — luci, colori per tasto, live, display, orologio su Linux
- [gohv/EPOMAKER-Ajazz-AK820-Pro](https://github.com/gohv/EPOMAKER-Ajazz-AK820-Pro) e [TaxMachine/ajazz-keyboard-software-linux](https://github.com/TaxMachine/ajazz-keyboard-software-linux) — primo reverse engineering di luci, sleep, orologio, immagini
- [Beattrey/ajazz-ak820-config](https://github.com/Beattrey/ajazz-ak820-config), [gusleig/ajazz-ak820-pro-mac](https://github.com/gusleig/ajazz-ak820-pro-mac) — note di protocollo
- [wsclx/ak820pro-modder](https://github.com/wsclx/ak820pro-modder) — protocollo alternativo dei firmware `0xFF67`
