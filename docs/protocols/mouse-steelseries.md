# SteelSeries mice (Aerox 3/5/9 Wireless, Prime Wireless, Rival 3 Wireless, Rival 650/700)

Scope: battery %, charging, polling rate. Research date 2026-11-24. Tags: **V** verified in source, **I** inferred, **U** unknown, **HW** third-party hardware report.

Source licenses: rivalcfg (flozz, WTFPL), libratbag (MIT), HaloBattery (MIT) - facts below are restated, nothing copied.

## 1. Device identification

VID = `0x1038` (SteelSeries ApS). Interface numbers below are USB interface numbers (rivalcfg opens by `interface_number`, "endpoint" in its profiles).

| Mouse | PID(s) | Link | USB interface (battery) | Source |
|---|---|---|---|---|
| Aerox 3 Wireless | `1838` | 2.4 GHz dongle | 3 | V rivalcfg aerox3_wireless_wireless.py L23-L27 |
| Aerox 3 Wireless (CS2 Dragon Lore) | `1878` | dongle | 3 | V same L29-L33 |
| Aerox 3 Wireless | `183A`; `187A` (CS2) | cable | 3 | V aerox3_wireless_wired.py L11-L21 |
| Aerox 5 Wireless | `1852`, `185C`, `1860` | dongle | 3 | V aerox5_wireless_wireless.py L23-L39 |
| Aerox 5 Wireless | `1854`, `185E`, `1862` | cable | 3 | V aerox5_wireless_wired.py |
| Aerox 9 Wireless | `1858`, `1874` | dongle | 3 | V aerox9_wireless_wireless.py |
| Aerox 9 Wireless | `185A`, `1876` | cable | 3 | V aerox9_wireless_wired.py |
| Prime Wireless / Prime Mini Wireless | `1840` / `1848` | dongle | 3 | V prime_wireless_wireless.py L23-L33 |
| Prime Wireless / Prime Mini Wireless | `1842` / `184A` | cable | 3 | V prime_wireless_wired.py L11-L21 |
| Rival 3 Wireless | `1830` | dongle (Bluetooth mode: no HID battery, Windows BT battery instead) | 3 | V rival3_wireless.py L8-L14 |
| Rival 3 Wireless Gen 2 | `1872` | dongle | 3 | V rival3_wireless_gen2.py |
| Rival 650 Wireless | `1726` (dongle), `172B` (cable) | | 0 | V rival650.py L7-L19 |
| Wired-only (no battery, rate set-only): Aerox 3 `1836`, Aerox 5 `1850`, Prime `182E/182A/1856`, Prime Mini `184D`, Prime+ `182C`, Rival 3 `1824/184C`, Rival 3 Gen2 `1870`, Rival 5 `183C/183E`, Rival 600 `1724/172E`, Rival 700 `1700/1730`, Sensei Ten `1832/1834` | | cable | see rivalcfg | V |

Not in rivalcfg (U): newer SteelSeries (Aerox 3/5/9 Wireless 2nd-gen "2022 refresh", Rival 5 Wireless, Prime Wireless 2025 4K/8K variants). Same family, but PIDs and quirks unknown.

Windows collection: SteelSeries exposes the config channel as a vendor collection. HaloBattery picks it by **usage page `0xFFC0`** rather than by interface number, because the Rival 650 has it on interface 0 (rivalcfg issue #202). I (for Aerox/Prime: the interface-3 collection is the `0xFFC0` one; rivalcfg only gives the interface number).

## 2. Transport

* Output report (hidapi `write`), **report ID 0**, then the command bytes, padded to the output report length (64 payload bytes -> 65 with the ID on Windows). V: rivalcfg mouse.py L264-L315 (`_hid_write` -> `hid.write([0x00, cmd...])`), HaloBattery docs ("a 64-byte `00 d2 ...`").
* The answer is an **input report** read with `hid.read` (no report ID in the returned data), timeout 200 ms in rivalcfg (V mouse.py L159-L205). Delay between commands: rivalcfg's default 50 ms.
* No checksum.

## 3. Battery

### 3.1 Aerox 3/5/9 Wireless, Prime Wireless (command `0x92`)

* **Cable mode** request payload: `92`; response length 2. **Dongle (wireless) mode**: the command byte is OR-ed with `0x40` -> `D2`; response length 64 (read 64 bytes, only the first bytes matter).
  V: rivalcfg aerox3_wireless_wireless.py L1-L18 (`_WIRELESS_FLAG = 0b01000000`, readback 64), aerox3_wireless_wired.py L165-L171.
* Response: `data[0]` = echo of the command (`92` / `D2`), `data[1]` = battery byte.
  * charging = `data[1] & 0x80`
  * level % = `((data[1] & 0x7F) - 1) * 5` (so 0x01..0x15 -> 0..100 %, steps of 5). V: aerox3_wireless_wired.py L169-L170. HW: Aerox 9 Wireless reply `D2 04 ...` = 15 % matches SteelSeries GG (HaloBattery docs #79).
  * `data[1] & 0x7F == 0` -> mouse off / asleep, not empty (HaloBattery, from yurtemre7/steel-mouse). HaloBattery also accepts raw values above 21 as a direct percentage (I, not verified by us).
* Charging bit not yet confirmed on hardware in the dongle path (HaloBattery docs: "the charging bit is not tested yet"). I.

### 3.2 Rival 3 Wireless (+Gen 2), Rival 650 Wireless (command `AA 01`)

* Request payload `AA 01`, response length 3 (rivalcfg). V: rivalcfg rival3_wireless.py L71-L76, rival650.py L118-L124.
  * level % = `data[0]` (0..100)
  * charging = `data[2]` non-zero.
* HaloBattery reads two shapes because the references disagree: reply echoing `AA` (level in byte 1, charging in byte 3) or the bare 3-byte reply. Not hardware-confirmed (HaloBattery docs: "neither has been on hardware here"). I.

## 4. Polling rate

* **Not readable** in any open source. rivalcfg only *sets* it (Aerox/Prime wireless: command `2B`, value 125 Hz `03`, 250 `02`, 500 `01`, 1000 `00`; Rival 3 Wireless: command `17`, same value map; V rivalcfg aerox3_wireless_wired.py L38-L53, rival3_wireless.py L29-L45).
  libratbag's steelseries driver writes report-rate and has no read. V libratbag driver-steelseries.c L308 (`report_rates[] = {125,250,500,1000}`), L498-L525.
* The same code table (0 -> 1000 Hz, 1 -> 500, 2 -> 250, 3 -> 125) is the likely read-back format, but no `get` command is documented. **U**.
* Practical: show "n/a" for SteelSeries wireless, or measure with Raw Input (see README-mouse-major.md). The Aerox 3/5/9 wireless mice cannot exceed 1000 Hz (Prime Wireless 2025 / Aerox 4K variants: U).

## 5. Real model name

* The dongle's USB product string is generic ("SteelSeries Aerox 3 Wireless" per PID in practice), HID descriptors do not carry the mouse model. Use the PID -> name table above. For sub-variants (CS2 Dragon Lore) the PID differs, so the table is enough. V (rivalcfg model lists). Firmware: command `90 00` (response 2 bytes) on Rival 3/650 (V rival3_wireless.py L81-L85) - not needed.

## 6. Quirks

* **Asleep**: the 2.4 GHz link sleeps when the mouse is idle; the dongle answers with level 0 (or a zero-filled 64-byte report) until the mouse moves. Keep the last reading. HW: HaloBattery docs for Aerox 3 Wireless.
* rivalcfg's own docs: `battery` returns level `None` when "the mouse is turned off" (V mouse.py L159-L168).
* SteelSeries GG can be running: HaloBattery states it "Works alongside SteelSeries GG". HW (Aerox 9).
* Do not send unknown opcodes: commands like `0x21`, `0x23`, `0x2B`, `0x2D`, `0x11` in these profiles are **setters** (DPI, LEDs, save to flash). Only `92`/`D2` and `AA 01` are read-only.
* Wireless flag trick: the dongle ignores the plain wired opcode, so `92` is silent on the dongle and `D2` is silent on the cable. Send the right one per PID (dongle PIDs = wireless flag).
* Report ID 0 + 64 bytes on Windows; the reply may need a second `read` if G HUB-like software queued unrelated input. Use a timeout of 200-500 ms.

## 7. Sources

* rivalcfg (WTFPL): https://github.com/flozz/rivalcfg/tree/master/rivalcfg/devices (aerox3_wireless_wired.py, aerox3_wireless_wireless.py, aerox5_*, aerox9_*, prime_wireless_*, rival3_wireless*.py, rival650.py) and mouse.py (`battery`, `_hid_write`)
* libratbag (MIT): https://github.com/libratbag/libratbag/blob/master/src/driver-steelseries.c
* HaloBattery (MIT): https://github.com/HeyOkay/HaloBattery/blob/main/docs/protocols.md ("SteelSeries Aerox 3 Wireless", "SteelSeries Rival 3 Wireless") and providers/steelseries.py
* Referenced there: alloyctl, yurtemre7/steel-mouse, gort818/aerox3-wireless (not read by us)
