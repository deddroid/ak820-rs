# ak820 — configuratore AJAZZ AK820 Pro per Linux e Windows

Un unico programma in Rust per configurare la AJAZZ AK820 Pro senza il software ufficiale:

- **Luci**: tutti i 20 effetti, colore, arcobaleno, luminosità, velocità, direzione
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

## Installazione

```bash
cargo build --release
```

Il binario è `target/release/ak820` (`ak820.exe` su Windows).

### Linux: permessi

I device `hidraw` sono accessibili solo a root. Installa una volta la regola udev:

```bash
sudo cp udev/70-ajazz-ak820.rules /etc/udev/rules.d/ && sudo udevadm control --reload && sudo udevadm trigger
```

poi scollega e ricollega la tastiera. Serve `libudev` (presente su tutte le distro con systemd).

### Windows

Nessun driver richiesto: compila con `cargo build --release` (toolchain MSVC) e usa
`ak820.exe`. Chiudi il software ufficiale AJAZZ se è aperto.

## Uso

```bash
ak820 list                                   # interfacce HID trovate
ak820 keys                                   # nomi dei tasti, target ed effetti

ak820 light rolling --rainbow -b 5 -s 3      # effetto onda arcobaleno
ak820 light static -c ff6600 -b 4            # colore fisso
ak820 light off

ak820 colors --base 000020 w=red a=red s=red d=red   # colori per tasto
ak820 demo --seconds 10                      # animazione live (non salvata)

ak820 clock                                  # sincronizza l'orologio
ak820 screen gatto.gif --fit crop            # immagine/GIF sul display

ak820 system --sleep 30m --fn-mode hold --response 2 --block-win off

ak820 remap capslock=ctrl pgdn=end           # rimappa
ak820 remap --fn left=prev right=next down=play
ak820 remap delete=text:ciao                 # macro che scrive "ciao"
ak820 macro set copia "tap:ctrl+c" "delay:50" "tap:ctrl+v"
ak820 remap insert=macro:copia
ak820 remap pgdn=default                     # torna al tasto originale
ak820 remap --reset                          # layer normale di fabbrica
ak820 remap                                  # mostra le rimappature

ak820 apply examples/profile.toml            # applica un profilo completo
ak820 export mio-profilo.toml                # salva la configurazione corrente
ak820 resend                                 # reinvia tutto (es. dopo un reset)
ak820 reset --yes                            # impostazioni di fabbrica
ak820 battery                                # batteria (solo via ricevitore 2.4G)
```

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
[docs/PROTOCOL.md](docs/PROTOCOL.md).

## Crediti

Codice scritto da [Claude](https://claude.com/claude-code) (Anthropic) insieme a
[deddroid](https://github.com/deddroid), che lo ha testato sulla tastiera.

Protocollo consolidato da questi progetti (grazie!):

- [gha-projects/ak820-mac](https://github.com/gha-projects/ak820-mac) — analisi di `DeviceDriver.exe`: tasti, macro, sistema, ricevitore 2.4G, checksum
- [hcristosm/ak820-linux](https://github.com/hcristosm/ak820-linux) — luci, colori per tasto, live, display, orologio su Linux
- [gohv/EPOMAKER-Ajazz-AK820-Pro](https://github.com/gohv/EPOMAKER-Ajazz-AK820-Pro) e [TaxMachine/ajazz-keyboard-software-linux](https://github.com/TaxMachine/ajazz-keyboard-software-linux) — primo reverse engineering di luci, sleep, orologio, immagini
- [Beattrey/ajazz-ak820-config](https://github.com/Beattrey/ajazz-ak820-config), [gusleig/ajazz-ak820-pro-mac](https://github.com/gusleig/ajazz-ak820-pro-mac) — note di protocollo
- [wsclx/ak820pro-modder](https://github.com/wsclx/ak820pro-modder) — protocollo alternativo dei firmware `0xFF67`
