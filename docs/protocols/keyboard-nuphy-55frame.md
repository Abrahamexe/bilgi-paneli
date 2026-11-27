# Keyboard family: "0x55 / 0xAA chunk frame" (NuPhy Console boards - Air60/Air75/Halo75 V1, WH80; same frame as MCHOSE "CZ" channel and AULA SC620)

Legend: **V** verified in third-party source (URL), **I** inferred, **U** unknown.

## 1. Who
| Brand/model | VID:PID | Conf |
|---|---|---|
| NuPhy WH80 | VID **0x19F5**; wired **0xA011**; 2.4 GHz dongle **0x8F01** (product string contains "Dongle") | V (FerrisPulse `NuphyDeviceHelper.cs`) |
| NuPhy Air75/Air60/Halo75 V1 (Console-managed) | VID 0x19F5, PIDs not listed here | I (nudelta Readme says V1 boards use the NuPhy Console proprietary USB protocol) |
| Names the tray tool maps for dongle PIDs: 0x8F02 Air75 V2, 0x8F03 Air96 V2, 0x8F04 Air60 V2, 0x8F05 Halo75 V2, 0x8F06 Halo96 V2, 0x8F07 Field75 | | **U/weak**: Air75 V2 etc. are QMK/VIA boards per NuPhy docs and nudelta README; the name table is a guess by that tool. Probe before trusting. |
| Same frame family elsewhere | MCHOSE "CZ" keyboards (G87 V2/K99 V3 generation, `[55, cmd, 0, chk, size, offLo, offHi, 0, payload]`), AULA SC620 mouse (`55 cmd flag chk len off...`, chk = sum bytes 4..63) | V |
VID 0x19F5 is also the VID of a DrunkDeer gen-2 OEM variant (A75 Pro `19F5:FB5C`) - different protocol (see keyboard-wired-analog-misc.md); do not match on VID alone.

## 2. Endpoint
HID collection with **input and output report length both 65** (report ID 0 + 64) - prefer the one on `MI_01`. A different collection without that shape is the keyboard/consumer. V (tray tool enumerates `caps.InputReportByteLength == 65 && OutputReportByteLength == 65`). Open with read/write + overlapped I/O; send with WriteFile and read the reply with an already-pending ReadFile (the reply can arrive immediately). Wait up to ~300 ms.

## 3. Frame (V)
65-byte Windows buffer:
* `[0]` = 0x00 (report ID)
* `[1]` = 0x55 (request marker; reply marker is 0xAA, and some firmware echoes 0x55)
* `[2]` = command (0x04 / 0x05 read..., see below)
* `[3]` = 0
* `[4]` = **checksum = sum of buffer bytes [5..64] mod 256**
* `[5]` = length of data to read/write
* `[6..7]` = offset, little-endian
* `[8]` = 0
* `[9..]` = payload (writes) / reply data (reads)
Reply: same layout, `[1]` in {0xAA, 0x55}, `[2]` echoes cmd, **data starts at [9]**; need >= 10 bytes read.

## 4. Read commands used (all non-mutating)
| cmd | meaning | request | reply |
|---|---|---|---|
| 0x04 | get current connection mode | offset 0, len 16 | `[9]` = mode 0..3 (0 = wired USB, 1 = 2.4 GHz, 2/3 = Bluetooth slots - **I** for 2/3) |
| 0x05 | read config RAM | offset `mode*64`, len 54 | config block (per-mode 64-byte blocks) |
| **0x05 @ `mode*64 + 32`, len 1** | **battery** | as above | **`[9] & 0x7F` = battery percent (0..100)** (accept only `[2]==0x05` and `[1]` in {AA,55}) |
Charging: **no flag in this block.** The tray tool assumes "charging" when the PID is the wired one (0xA011) or percent >= 100 - an inference, not a protocol field (**I**).
Config block fields (for polling rate): relative to reply data start (`[9]`): `+4` = rate byte, **low nibble = polling code: 1 = 125, 2 = 250, 3 = 500, 4 = 1000 Hz** (comment in source; higher nibble reserved), `+5` sleep (bit7 enable, bits0-6 minutes), `+6` Win-lock bit0, `+9` backlight brightness, `+25` side-light brightness. Offsets of the block start by current mode (mode 0 = wired profile; sleep is stored on the wireless profile at offset 64). Polling >1000 on 8K models: **U** (code table beyond 4 not shown). V for 1..4, I for the general shape.
Write commands (cmd 0x03/0x06...) exist (backlight, sleep, Win-lock) - **never send**.

## 5. Model name
USB product string (dongle strips the word "Dongle": e.g. "NuPhy Air75"), else PID table above. Behind a dongle shared by several keyboards: **U**.

## 6. Quirks
* The dongle answers even when the keyboard is on wired/Bluetooth, so "dongle present" != online; decide "online" from battery reply validity.
* Use a per-query lock; the official NuPhy Console must be closed.
* V1 boards via nudelta: protocol is similar but keymap-centric; battery not documented there.
* NuPhy IO web (nuphy.io, Air75 HE/Node) is a newer proprietary protocol: **U**.

## Sources
* https://github.com/Ferris-echo/FerrisPulse/blob/main/src/Hardware/NuphyDeviceHelper.cs (GPL-3, facts only) and docs/HARDWARE_PROTOCOLS.md 2.1
* https://github.com/donn/nudelta (Readme)
* https://github.com/Mightling/vigil-watt/blob/main/docs/hardware/protocols.md section 2.3 (CZ channel frame, same layout)
* https://github.com/paulcoding810/open-aula/blob/main/web/PROTOCOL.md (SC620 0x55 frame)
