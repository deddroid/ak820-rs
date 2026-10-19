# AK820 Pro protocol (stock firmware)

**English** · [Italiano](PROTOCOL.it.md)

This is the protocol used by the official Windows driver `DeviceDriver.exe` v1.0.0.5,
consolidated from several community projects (see the credits in the README) and implemented
in `src/protocol.rs`, `src/device.rs`, `src/macros.rs`, `src/screen.rs`.

All multi-byte numbers are little-endian. "Packet" = 64 bytes.

## Devices

| Connection | VID:PID | Interfaces |
|---|---|---|
| USB cable | `0C45:8009` or `0C45:800A` | usage page `0xFF13` (if 3): 64-byte feature reports, commands · `0xFF68` (if 2): 4096-byte output reports, screen images |
| 2.4 GHz receiver | `0C45:FDFD` | if 3, usage page `0xFF60`: 32-byte input/output reports |
| Bluetooth | `0C45:FEFE` | no vendor interface: cannot be configured |

Report ID is always `0` (hidapi wants it in front of the buffer).

> Note: some newer firmwares expose `0xFF67`/`0xFF68` and speak the web-driver protocol
> (`AA cmd …`, see `wsclx/ak820pro-modder`). This tool implements the `0xFF13` protocol,
> the one exposed by the keyboard it was developed on.

## Cable: commands

A command is `[0]=04 [1]=cmd [2]=arg [8..10]=count`, the rest zero. After every command a
feature report is read (the reply). Data packets following a command are sent without
reading a reply.

**The official driver waits 35 ms before and after every command.** Without the pause
*before*, the keyboard often ignores the command.

| Cmd | Meaning |
|---|---|
| `18` | BEGIN |
| `02` | COMMIT. Reply (hidapi buffer, report ID first): bytes 5–6 = 16-bit sum of the data bytes received → verify and retry |
| `F0` | SAVE to flash |
| `13` | Lighting effect |
| `11` / `27` | Key table, normal / Fn layer |
| `19` / `15` | Macros: erase / data |
| `17` | System settings |
| `23` | Per-key colors (saved) |
| `20` | Per-key colors live (not saved) |
| `28` | Clock |
| `72` | Screen image |

### Lighting — `18`(1), `13`(1), data, `02`, `F0`
Data: `[0]=effect [1..4]=R G B [8]=rainbow [9]=brightness 0–5 [10]=speed 0–5
[11]=direction [14..16]=AA 55`.

Direction: the firmware only has two per effect. `0` (same as `3`) = rightward (rolling,
rotating, flowing, tilt) or downward (scrolling); `1` (same as `2`) = leftward or upward.
Verified on hardware for scrolling and rolling. Other effects ignore the direction.

Effects: `00` off, `02` single-on, `03` single-off, `04` glittering, `05` falling,
`06` colourful, `07` breath (= "static" with speed 0), `08` spectrum, `09` outward,
`0A` scrolling, `0B` rolling, `0C` rotating, `0D` explode, `0E` launch, `0F` ripples,
`10` flowing, `11` pulsating, `12` tilt, `13` shuttle, `80` per-key colors.

### Per-key colors — effect `80`, then `18`(0), `23`(9), 9 packets, `02` (verified), `F0`
576-byte table (144 slots × 4, ending in `AA 55`): `[index, R, G, B]` in each key's
`index` slot.

### Live — `20`(8), 8 packets, `02`
512 bytes of consecutive `[index, R, G, B]`. 5 ms pauses around the commands and 2 ms
between packets (~23 fps). When frames stop, the keyboard returns to its saved effect.

### Clock — `18`(1), `28`(1), data, `02`
`[1]=01 [2]=5A [3]=year-2000 [4]=month [5]=day [6]=hour [7]=min [8]=sec [10]=04 [62..64]=AA 55`.
Setting the time switches the screen back to the clock.

### System — `18`(0), `17` arg 1 (1), data, `02` (verified)
`[1]=01 [2]=block Win [3]=block Alt+F4 [4]=block Alt+Tab [5]=Fn (0 held, 1 toggle)
[6]=sleep (0 never, 1=1 min, 2=5 min, 3=30 min) [8]=response 1–5 [62..64]=AA 55`.
Defaults: Fn 0, sleep 2, response 2. Cannot be read back.

### Key table — `18`(0), `11`/`27`(9), 9 packets, `02` (verified), `F0`
576 bytes, 4 bytes in the `key_index` slot of each key:

| Bytes | Meaning |
|---|---|
| `00 00 00 00` | factory behaviour |
| `02 mods key 00` | key/shortcut (mods: Ctrl 01, Shift 02, Alt 04, Win 08; right-hand 10 20 40 80) |
| `03 lo hi 00` | consumer (play CD, next B5, prev B6, stop B7, mute E2, vol+ E9, vol− EA…) |
| `01 01 btn 00` | mouse: left 01, right 02, middle 04 |
| `01 03 wheel 00` | wheel: 01 up, FF down |
| `06 id mode n` | macro `id`; mode 0 once, 1 n times, 2 toggle, 3 while held, 4 step |
| `07 k1 k2 k3` | Multi-Key: up to 3 keys at once |

On the Fn layer Esc, Backspace, Home, `\`, Win and Delete cannot be changed.
The keyboard cannot be read back: the whole table of both layers is always sent.

### Macros — `19`(0), `15`(n packets), block, `02` (verified). No BEGIN.
Block (max 3584 bytes, shared by both layers):
- bytes 0–399: index of 100 entries × 4 bytes, bytes 0–1 = offset of macro *i*
- from 400: event count (2 bytes) + 6 zero bytes + 4-byte events:
  `00 00 key B0` press, `00 00 key 30` release, `lo hi 00 50` delay ms,
  `00 00 btn 90`/`10` mouse down/up
- padded to multiples of 64 (`len/64 + 1` packets if divisible, else `+2`), `AA 55` at the end.

Macros are sent first, then the key tables. The first upload is sometimes lost: retry.

### Screen — `18`(0), `72` arg 1 (n blocks), blocks on `0xFF68`, `02` (no `F0`)
Content: 256-byte header (`[0]` = frame count ≤ 140, `[1+i]` = duration in 2 ms units,
min 1, rest `FF`) + 128×128 RGB565 little-endian frames, padded with `FF` to whole
4096-byte blocks. After each block, wait for the `01 5A 02` ack on the data interface.
A still image = 9 blocks.

Factory reset: there is no command; the driver resends the default profile.
On the keyboard: hold Fn+Space for 3–5 s.

## 2.4 GHz receiver

32-byte messages: `[order] [length] [chunk #] [≤28 data bytes] [sum of bytes 0–30]`.
The receiver answers by echoing the first 3 bytes. No BEGIN/COMMIT/SAVE.

| Order | Data |
|---|---|
| `05` lighting | first 14 bytes of the lighting packet + `AA 55` |
| `07` system | first 14 bytes of the system packet + `AA 55` |
| `0C` clock | like the cable packet, `[10]` = day of week (Sunday 0) |
| `10` / `12` | key table normal / Fn (21 chunks) |
| `09` macros | block padded to multiples of 28; the last chunk's length is `len % 28` |
| `20` battery | reply `[3]` = % (0 = unknown) |

Per-key colors and screen: cable only.
