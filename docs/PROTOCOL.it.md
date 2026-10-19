# Protocollo AK820 Pro (firmware originale)

[English](PROTOCOL.md) · **Italiano**

Questo è il protocollo usato dal driver ufficiale Windows `DeviceDriver.exe` v1.0.0.5,
ricostruito da più progetti della comunità (vedi i crediti nel README) e implementato in
`src/protocol.rs`, `src/device.rs`, `src/macros.rs`, `src/screen.rs`.

Tutti i numeri a più byte sono little-endian. "Pacchetto" = 64 byte.

## Dispositivi

| Collegamento | VID:PID | Interfacce |
|---|---|---|
| Cavo USB | `0C45:8009` o `0C45:800A` | usage page `0xFF13` (if 3): feature report da 64 byte, comandi · `0xFF68` (if 2): output report da 4096 byte, immagini del display |
| Ricevitore 2.4 GHz | `0C45:FDFD` | if 3, usage page `0xFF60`: report input/output da 32 byte |
| Bluetooth | `0C45:FEFE` | nessuna interfaccia vendor: non configurabile |

Report ID sempre `0` (hidapi lo vuole davanti al buffer).

> Nota: alcuni firmware più recenti espongono `0xFF67`/`0xFF68` e parlano il protocollo del
> driver web (`AA cmd …`, vedi `wsclx/ak820pro-modder`). Questo strumento implementa il
> protocollo `0xFF13`, quello esposto dalla tastiera su cui è stato sviluppato.

## Cavo: comandi

Un comando è `[0]=04 [1]=cmd [2]=arg [8..10]=conteggio`, il resto a zero. Dopo ogni
comando si legge un feature report (la risposta). I pacchetti dati che seguono un comando
si inviano senza leggere risposta.

**Il driver ufficiale attende 35 ms prima e dopo ogni comando.** Senza la pausa *prima*,
la tastiera spesso ignora il comando.

| Cmd | Significato |
|---|---|
| `18` | BEGIN |
| `02` | COMMIT. Risposta (buffer hidapi con report ID in testa): byte 5–6 = somma a 16 bit dei byte dati ricevuti → verifica e ritenta |
| `F0` | SAVE in flash |
| `13` | Effetto luci |
| `11` / `27` | Tabella tasti, layer normale / Fn |
| `19` / `15` | Macro: cancella / dati |
| `17` | Impostazioni di sistema |
| `23` | Colori per tasto (salvati) |
| `20` | Colori per tasto live (non salvati) |
| `28` | Orologio |
| `72` | Immagine display |

### Luci — `18`(1), `13`(1), dati, `02`, `F0`
Dati: `[0]=effetto [1..4]=R G B [8]=arcobaleno [9]=luminosità 0–5 [10]=velocità 0–5
[11]=direzione [14..16]=AA 55`.

Direzione: il firmware ne ha solo due per effetto. `0` (come `3`) = verso destra
(rolling, rotating, flowing, tilt) o verso il basso (scrolling); `1` (come `2`) = verso
sinistra o verso l'alto. Verificato su hardware per scrolling e rolling. Gli altri effetti
ignorano la direzione.

Effetti: `00` off, `02` single-on, `03` single-off, `04` glittering, `05` falling,
`06` colourful, `07` breath (= "static" con velocità 0), `08` spectrum, `09` outward,
`0A` scrolling, `0B` rolling, `0C` rotating, `0D` explode, `0E` launch, `0F` ripples,
`10` flowing, `11` pulsating, `12` tilt, `13` shuttle, `80` colori per tasto.

### Colori per tasto — effetto `80`, poi `18`(0), `23`(9), 9 pacchetti, `02` (verificato), `F0`
Tabella da 576 byte (144 slot × 4, termina con `AA 55`): `[indice, R, G, B]` nello slot
`indice` di ogni tasto.

### Live — `20`(8), 8 pacchetti, `02`
512 byte di `[indice, R, G, B]` consecutivi. Pause di 5 ms sui comandi e 2 ms tra i
pacchetti (~23 fps). Smettendo di inviare, la tastiera torna all'effetto salvato.

### Orologio — `18`(1), `28`(1), dati, `02`
`[1]=01 [2]=5A [3]=anno-2000 [4]=mese [5]=giorno [6]=ora [7]=min [8]=sec [10]=04 [62..64]=AA 55`.
Impostare l'ora riporta il display sull'orologio.

### Sistema — `18`(0), `17` arg 1 (1), dati, `02` (verificato)
`[1]=01 [2]=blocca Win [3]=blocca Alt+F4 [4]=blocca Alt+Tab [5]=Fn (0 tenuto, 1 toggle)
[6]=sleep (0 mai, 1=1 min, 2=5 min, 3=30 min) [8]=risposta 1–5 [62..64]=AA 55`.
Default: Fn 0, sleep 2, risposta 2. Non si può rileggere.

### Tabella tasti — `18`(0), `11`/`27`(9), 9 pacchetti, `02` (verificato), `F0`
576 byte, 4 byte nello slot `key_index` di ogni tasto:

| Byte | Significato |
|---|---|
| `00 00 00 00` | comportamento di fabbrica |
| `02 mods key 00` | tasto/scorciatoia (mods: Ctrl 01, Shift 02, Alt 04, Win 08; destri 10 20 40 80) |
| `03 lo hi 00` | consumer (play CD, next B5, prev B6, stop B7, mute E2, vol+ E9, vol− EA…) |
| `01 01 btn 00` | mouse: sx 01, dx 02, centrale 04 |
| `01 03 rot 00` | rotella: 01 su, FF giù |
| `06 id modo n` | macro `id`; modo 0 una volta, 1 n volte, 2 toggle, 3 finché tenuto, 4 passo-passo |
| `07 k1 k2 k3` | Multi-Key: fino a 3 tasti insieme |

Sul layer Fn non si possono cambiare Esc, Backspace, Home, `\`, Win e Delete.
La tastiera non si può rileggere: si invia sempre l'intera tabella di entrambi i layer.

### Macro — `19`(0), `15`(n pacchetti), blocco, `02` (verificato). Niente BEGIN.
Blocco (max 3584 byte, unico per entrambi i layer):
- byte 0–399: indice di 100 voci × 4 byte, byte 0–1 = offset della macro *i*
- da 400: numero eventi (2 byte) + 6 byte zero + eventi da 4 byte:
  `00 00 key B0` premi, `00 00 key 30` rilascia, `lo hi 00 50` pausa ms,
  `00 00 btn 90`/`10` mouse giù/su
- padding a multipli di 64 (`len/64 + 1` pacchetti se divisibile, altrimenti `+2`), `AA 55` in coda.

Si inviano prima le macro, poi le tabelle tasti. Il primo invio a volte va perso: si ritenta.

### Display — `18`(0), `72` arg 1 (n blocchi), blocchi su `0xFF68`, `02` (niente `F0`)
Contenuto: header da 256 byte (`[0]` = n. fotogrammi ≤ 140, `[1+i]` = durata in unità di
2 ms, min 1, resto `FF`) + fotogrammi 128×128 RGB565 little-endian, padding `FF` fino a
blocchi interi da 4096. Dopo ogni blocco si attende l'ack `01 5A 02` sull'interfaccia dati.
Un'immagine fissa = 9 blocchi.

Reset di fabbrica: non c'è un comando; il driver reinvia il profilo di default.
Sulla tastiera: Fn+Spazio per 3–5 s.

## Ricevitore 2.4 GHz

Messaggi da 32 byte: `[ordine] [lunghezza] [n. chunk] [≤28 byte dati] [somma byte 0–30]`.
Il ricevitore risponde ripetendo i primi 3 byte. Nessun BEGIN/COMMIT/SAVE.

| Ordine | Dati |
|---|---|
| `05` luci | primi 14 byte del pacchetto luci + `AA 55` |
| `07` sistema | primi 14 byte del pacchetto sistema + `AA 55` |
| `0C` orologio | come il pacchetto via cavo, `[10]` = giorno della settimana (domenica 0) |
| `10` / `12` | tabella tasti normale / Fn (21 chunk) |
| `09` macro | blocco con padding a multipli di 28; l'ultimo chunk ha lunghezza `len % 28` |
| `20` batteria | risposta `[3]` = % (0 = sconosciuta) |

Colori per tasto e display solo via cavo.
