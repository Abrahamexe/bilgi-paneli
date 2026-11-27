# Keyboard family: AULA / Epomaker-x-AULA / generic CompX 2.4G dongles (Sonix 0x0C45, borrowed Apple VID 0x05AC) and the "AA-frame" boards (AULA F87 Pro, PMO)

Legend: **V** verified in third-party source/docs, **I** inferred, **U** unknown. Hardware-tested status is stated where the source says so.

AULA is a *vendor with many unrelated firmware generations*. A Windows tray tool (closed protocol crate, non-commercial licence) lists at least twelve
separate AULA battery protocol variants (Legacy, Yb, Vision, Yxt, Xxwd, Bytech, BytechMagnetic, Hfd, SparkLink, Hx, Jm, Rlw) - V (names only) in
https://github.com/ZGMFX01A/Wireless-Device-Battery-Monitor `src/service.rs`. So "AULA" is **not one protocol**. The three that have public byte-level docs are below.

## A. "aula04" / CompX dongle family (AULA S75 Pro, Epomaker x AULA F75 Max, generic CompX 2.4G boards)

### Identifiers
| Item | Value | Conf |
|---|---|---|
| Wired keyboard | VID 0x0C45 PID 0x800A (Sonix) - 64-byte feature reports, no report ID; 4096-byte output report for screen | V (open-aula PROTOCOL.md) |
| 2.4 GHz dongle | **VID 0x05AC PID 0x024F** (Apple VID borrowed), keyboard-side interface MI_03; product string `"2.4G Wireless Receiver"` | V |
| Dongle raw collection | usage page **0xFF60** (32-byte In/Out, preferred) ; sibling collection usage page 0xFF59 = command channel; other pages are the standard keyboard/consumer | V (Aula-F75-Max-Driver AulaCore.swift: wirelessRawUsagePage 0xff60, wirelessCommandUsagePage 0xff59) |
| Windows buffer length | 33 bytes (report ID 0 + 32) per FerrisPulse; hidapi on Linux/macOS 32 or 64 accepted by the F75 driver | V |
| Same VID/PID in other catalogs | `05AC:024F` is also listed for "AJAZZ K690T" in the RongYuan web-driver catalog and used by "generic CompX 2.4G wireless keyboards"; FerrisPulse separates them from "Ganss" devices by the product string | V |
| Other CompX/shared receivers | `3554:FA09` (MCHOSE G75 PRO via 0x13 frames; KZZI K75; AULA F75 dongle uses a *separate, unsupported* delegate), `3554:FB34` (Imecoo mouse) | V (FerrisPulse; open-aula) |

### Framing
32-byte output report: `[cmd, len, seq, data[28], checksum]`, **checksum = sum of the first 31 bytes mod 256** at index 31 (the F75 driver zeroes byte 31, sums the whole 32 bytes - identical). The dongle echoes `cmd len seq` in an input report. V (open-aula PROTOCOL.md; Aula-F75-Max-Driver `AulaChecksum`).
When a Windows report-ID byte is prepended (value 0x00) the checksum index becomes 32 and all offsets shift by +1 (FerrisPulse example `00 20 01 00 ... 21`: 0x21 = 0x20+0x01).

### Battery  (V, hardware-tested on S75 Pro / F75 Max; read-only)
* Request: `cmd=0x20, len=0x01` and all other bytes zero, checksum last: `20 01 00 ... 00 21` (32 B) ; with report-ID prefix: `00 20 01 00 ... 21` (33 B).
  The F75 driver tries three lengths (64/33/32) and with/without the ID prefix because hid stacks differ; after a send it waits up to ~250 ms for the matching input report.
* Reply (no ID prefix): `byte0=0x20, byte1=0x01, byte3 = battery percent (1..100)`. With ID prefix: `byte1=0x20, byte2=0x01, byte4 = percent`. Ignore 0 and >100 (0/sleep frames).
* Charging flag: **U** (none documented for this family; FerrisPulse reads only the percent). Charge state could be inferred from "wired PID present".
* Dongle-only: over USB cable no battery command exists (V).
* Refresh: apps poll every 60 s - 5 min.

### Polling rate
Not a Hz setting in this family: settings command `0x07` carries "key response level 1..5" (debounce-like), not Hz. **U** for a Hz query. Fall back to USB endpoint interval.

### Model behind the dongle
Shared dongle for every CompX board; model is **U** over HID (F75 driver hard-codes "Aula F75 Max 2.4G"). Heuristic: the paired keyboard's name comes only from Bluetooth/OS side.

## B. "AA-frame" family (AULA F87 Pro "Sonix" firmware; PMO Wave 75; probably more)

### Identifiers (V: open-aula `web/models/f87pro.js`, PROTOCOL.md "AA protocol")
* Wired: `38A6:2710/2711/2712/2713`, `0C45:8803`.
* Dongles: `0C45:FEFC` (usage page 0xFF80), `0C45:FEFE` (usage page 0xFF60).
* Interface = vendor usage-page collection; report ID 0; payload `packLen` = 32 bytes, or 64 on usage page 0xFF68 (wired).
* PMO (PMOKEY) Wave 75 dongle: `36B0:3002`, Windows in/out 33 bytes, report ID 0 (V: vigil-watt `PmoCodec.cs` docs, hardware-verified).

### Framing
`AA cmd len offLo offHi o5 o6 o7 data[packLen-8]`; every packet is answered by an input report echoing `cmd`; next packet only after reply; reads: data starts at byte 8 of the reply. (V - F87: "untested on hardware, built from the vendor's own WebHID app").
PMO's docs describe the same leading `0xAA` + `cmd` but place **offset at [2..3] and length at [4]** (`AA 30 00 00 18 ...`). The two descriptions disagree on field order; request bytes for PMO battery are therefore taken from the PMO doc only (**I** for cross-compat).

### Commands relevant to us
| Cmd | Meaning | Detail | Conf |
|---|---|---|---|
| 0x10 | device info (32 B) | PMO: `0x12`=get device info, payload[1..2]=VID LE (use it to self-check alignment); `0x10`/`0x11` = start/stop comm session (official driver brackets operations) | V (PMO) |
| **0x30** (PMO) | **battery** | request `AA 30` + zeros, length byte 24 -> reply payload at data idx 8: **[8] = battery %**, **[9] = state 0 discharging / 1 charging / 2 full**; sample `AA 30 00 00 18 00 00 55 40 00` = 64 % discharging. Percent 0 or >100 = invalid/not ready | V (hardware-verified on Wave 75, 2.4 GHz) |
| 0x11 (read, F87) / 0x21 (write) | settings, 16 B | `[3]` sleep minutes, `[4]` response time 1-5, **`[5]` polling-rate code: 3=1000, 4=2000, 5=4000, 6=8000 Hz**, `[6]` OS mode, `[7]` clock format | V (F87 Pro, from vendor JS; **not** hardware-tested) |
| 0x13/0x14/0x15/0x12/0x16 | lighting/colours/macros/keymaps | not needed | - |

* F87 Pro has **no documented battery command** (open-aula `battery()` returns null for `aa` models) - U.
* The polling table here (3=1K ... 6=8K) differs from MonsGeek's (0=8K ... 6=125). Keep family-specific tables. Whether code 0..2 mean 125/250/500 is **I** (not listed in the source).

## C. SinoWealth "BYCOMBO4 / G5KB" (AULA F75 wired, VID 0x258A PID 0x010C)
520-byte feature reports with report ID 6 on usage page 0xFF00/1; packet `06 cmd sub 00 total idx sizeLo sizeHi` + data; reads use `cmd|0x80`, data at byte 8; config block has debounce, hardware effect, sleep; **no battery, no polling-rate** fields documented (V open-aula). The 258A:010C *receiver* exposes vendor collections COL03/05/06 and was exhaustively probed by a tray tool: **no battery channel reachable** (FerrisPulse 1.9, V). Treat Sinowealth 0x258A (Glorious/Redragon/Royal Kludge wired boards) as "no battery".

## Practical rules
1. Match dongles by (0x05AC,0x024F) + usagePage 0xFF60; send the 32/33-byte `0x20 0x01` query, validate checksum + echo, then read percent at idx 3 (or 4 with ID prefix).
2. If product string is not "2.4G Wireless Receiver" and VID 0x05AC, it is a real Apple device - skip.
3. For 0C45:FEFC/FEFE or 36B0:3002 use the AA/0x30 query; verify with the `0x12` VID echo first (cheap self-check).

## Sources
* https://github.com/paulcoding810/open-aula (web/PROTOCOL.md, web/protocols/aula04.js, aa.js, g5kb.js, web/models/*.js; REVERSING.md)
* https://github.com/VitalyArt/Aula-F75-Max-Driver (Sources/AulaCore/AulaCore.swift, Sources/AulaLinuxHID/LinuxHIDBackend.swift)
* https://github.com/Mightling/vigil-watt (docs/hardware/protocols.md, src/VigilWatt.Core/Plugins/Pmo/PmoCodec.cs; MIT)
* https://github.com/Ferris-echo/FerrisPulse/blob/main/docs/HARDWARE_PROTOCOLS.md (items 1.9, 2.11, community batch #6)
* https://github.com/ZGMFX01A/Wireless-Device-Battery-Monitor (variant names only; protocols not published)
