# Keyboard family: Corsair "Bragi" protocol (VID 0x1B1C) - K70 Pro Mini Wireless, K100 Air, K65 Plus Wireless, MAKR 75, Vanguard 99 Air, K70 Core TKL Wireless, ...

Legend: **V** verified in third-party source (URL), **I** inferred, **U** unknown. Same protocol as headset-corsair.md (Bragi) - one driver covers both.
Older Corsair keyboards (K70 RGB MK.2, K95 Platinum, K55 ...) use the legacy "CUE" protocol and are wired-only (no battery) - not covered.

## 1. Identifiers (V: OpenLinkHub `src/devices/devices.go`, PIDs there are decimal)
| Model | USB PID (hex) | Interface used | Link |
|---|---|---|---|
| K70 PRO MINI Wireless (USB) | 0x1BB6 | 1 | wired/cabled |
| K100 AIR (USB) | 0x1BAB | 1 | cabled |
| K65 PLUS Wireless (USB) | 0x2B10 / 0x2B11 / 0x2B23 | 1 | cabled |
| K65 PLUS Wireless **dongle (Slipstream)** | 0x2B07 | 1 | paired keyboards appear as endpoint 0x09 |
| MAKR 75 | 0x2B04 | 1 | |
| VANGUARD 99 AIR (USB) | 0x2B21 | 2 | |
Other "W/WU" folders in the repo (K70 Core TKL Wireless, K57 RGB Wireless, K100 Air wireless via Slipstream) exist; their PIDs were not extracted here (**U**). The Slipstream dongle also carries mice/headsets; map by paired-device list (VID/PID in the list).
Interface number comes from the table above; OpenLinkHub can alternatively match a usage page (`UsagePage`), value 0 = by interface number. Usage page of the Bragi collection: 65346 (0xFF42) for headsets (**I** same for keyboards).

## 2. Frame (V)
* **Write**: 65-byte buffer = `[0]=0x00 (report ID)`, `[1]=endpoint byte`, `[2..]=command bytes`, then data. **Read**: one 64-byte input report right after the write (blocking read).
* Endpoint byte: **0x08** = the device directly addressed on its own USB connection (wired keyboard, or the dongle's own control endpoint); **0x09** = first device paired to a Slipstream dongle (dongle lists paired devices through its own query; each gets an endpoint number). V (k70pmWU `bufferW[1]=0x08`; k65plusWdongle `Endpoint: 0x09`).
* No checksum.
* Keep-alive: devices in "software mode" need a heartbeat; for read-only queries do **not** switch modes (do not send `01 03 00 06` software-mode or `0x0d` open-endpoint commands).

## 3. Battery  (V)
| Query | command bytes | Reply (64-byte read buffer, no report ID) |
|---|---|---|
| **Battery level** | `02 0F` (get property 0x0F) | `reply[3..4]` = u16 little-endian, **tenths of a percent -> percent = value/10** (0..1000). Ignore 0. |
| **Charging status** | `02 10` (get property 0x10) | `reply[3]` = 0 not charging, 1 charging, 2 not charging(unplugged state variant), 3 full (OpenLinkHub label table) |
| Firmware | `02 13` | (not needed) |
Dongle: send a heartbeat to the dongle (`0x12` on endpoint 0x08), then `02 0F` to endpoint 0x09 (paired device), parse `reply[3..4]`/10. The dongle also **pushes** battery events: input report with `data[2]==0x0F` and `data[4..5]` LE u16 (/10) (V k65plusWdongle listener); `data[1]==0x01 && data[2]==0x36` = device connection status change (`data[4]` value).
Corsair HS80 MAX / M75 AIR (FerrisPulse) confirm 65-byte Bragi with VID 0x1B1C PIDs 0x0A97/0x0A96 (headset) and 0x1BDC/0x1BF2 (M75 AIR mouse), read-only battery supported. V (FerrisPulse "community batch" #4).

## 4. Polling rate
Only the *set* is public: command `01 01 00 <code>` (OpenLinkHub `UpdatePollingRate`). Code table used by the UI: **1=125, 2=250, 3=500, 4=1000, 5=2000, 6=4000, 7=8000 Hz** (0 = not set). V (K100 AIR / K70 PM wireless / K100 definitions).
**Read** of the polling property is not implemented in OpenLinkHub; by symmetry with the property-get `02 <prop>` the get would be `02 01` - **I/U, unverified**. Keyboard's USB-interval fallback is safer.

## 5. Model name
USB product string and PID. Behind a Slipstream dongle: dongle reports a paired-device table (vendor/product LE at offset 6..9 of a list entry, serial length at 10, serial from 11) via its device-list query on endpoint 0x08 (query command bytes not extracted: **U**; see k65plusWdongle.go lines ~270-300).

## 6. Quirks
* Only one client at a time on the Bragi interface; iCUE running concurrently will fight (read replies get stolen). Close iCUE or accept intermittent failures.
* Wireless keyboards report 0 % while asleep; show last value.
* Charging value 2 vs 0 both "not charging".

## Sources
* https://github.com/jurkovic-nikola/OpenLinkHub (GPL-3 - facts only): src/devices/k70pmW, k70pmWU, k100airWU, k65plusWdongle, devices.go, stats/
* https://github.com/Ferris-echo/FerrisPulse/blob/main/docs/HARDWARE_PROTOCOLS.md (community batch #4)
