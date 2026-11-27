# Cooler Master mice (MM711/MM712/MM720/MM730/MM731 and the "Cooler Master Mice Dongle")

Scope: battery %, charging, polling rate. Research date 2026-11-24. Tags: **V** verified in source, **I** inferred, **U** unknown, **HW** third-party hardware report.
Sources are clean-room documentation (syntactic-spice/coolermaster-usb-protocol, no license file: facts only), plus three small Windows battery monitors (AallynReed/FoxDen, itsSamBz/OnDeck-Juice; MIT/LICENSE present) that implement it.

## 1. Identification

VID = `0x2516` (Cooler Master). Wireless mice share one dongle:

| Device | PID | Notes |
|---|---|---|
| **Cooler Master Mice Dongle** | `01B7` | 2.4 GHz, 5 HID interfaces (MI_00..MI_04), 32-byte endpoints. V protocol doc L14-L25 |
| MM712 (cable) | `0169` | "MM712 Hybrid Mouse", 64-byte endpoints. V |
| MM731 (cable) | `0167` | V FoxDen coolermaster.rs L6-L9 (I: same family) |
| MM711 / MM720 / MM730 / MM530 (lighting-only research) | `0101` / `0141` / `0165` / `0065` | battery protocol U (older MM7xx have no wireless) |

Model on the shared dongle: status-report byte 2 = `0x50` -> MM712, `0x70` -> MM731 (MM731 seen once; I). V FoxDen/OnDeck `model_name`.

HID collection: **USB interface 3**, vendor usage page `0xFF0A` (usage 1; interface 4 has the same descriptor on the dongle, so select by interface number 3, not by usage). **No report ID**: Windows write = `00` + N bytes where **N = 32 on the dongle, 64 over the cable** (take N from the output report size). V protocol doc L35-L70 + OnDeck comment L1-L9.

## 2. Framing
* Output report, byte 0 = command; input report byte 0 = command + 1. Single request layout `CMD 81 SUB ARG...`, zero padded to N. V protocol doc §2.
* No handshake needed ("the probe tool opens interface 3 and sends commands straight away"). V doc L145-L147.

## 3. Battery + charging: status report `0x47`

Request: `46 81 00` + zeros (cmd `46`, sub 00 = "request 0x47 status report"; OnDeck/FoxDen write `00 46 81 00` including the report ID 0). V doc command table L176, FoxDen coolermaster.rs L45-L50.
Reply (and unsolicited when battery changes): `47 81 <model> ...`:

| Byte | Meaning |
|---|---|
| 0-1 | `47 81` |
| 2 | model byte (`50` MM712, `70` MM731) |
| **10** | **battery %** (1..100; 0 = invalid/unknown) |
| **11** | **charging: 0 no, 1 yes** |
| **12** | link: 1 = wireless dongle, 0 = cable |
| 13 | active profile 0..4 |
| 14 | active DPI stage index |
| 16 | active LED mode |

V protocol doc §4.1 (L200-L218); HW: MM712 on `2516:01B7` verified in OnDeck-Juice (comment: "MM712 verified on hardware"). Note the doc's observation: the percentage reads about 10 points higher while charging (probe: 63 % charging -> 53 % right after unplugging) - show last discharging value or flag as "charging".
Wait up to 500 ms for the reply; discard input reports whose first two bytes are not `47 81`. V OnDeck `QueryStatus`.
Asleep / off mouse: no reply; keep the last value (OnDeck/FoxDen grace 1800 s). With cable + dongle both attached the mouse shows twice: dedupe by name, OR the charging flags.

## 4. Polling rate

* Performance block (`4A`), "short" read of the first 15 bytes: request `4A 81 03` (+zeros). Reply `4B 81 03 <data from byte 3>`; per the doc "blocks 0x4a... have no SLOT, so their data starts at byte 3". Data byte 7 of the block = polling rate: **1 = 125 Hz, 2 = 250, 3 = 500, 4 = 1000** (V doc L233, L252: per-profile, confirmed by captures cap:20-23). So the code is at **reply byte 10** (3 + 7). I (derived from two documented facts; no capture of the reply bytes quoted).
* The value read is the one stored for the **active profile** (committed state); uncommitted host changes read back as written until a profile switch. Fine for monitoring.
* Max 1000 Hz on these mice (no 2k/4k/8k). V.

## 5. Name
PID table + model byte (section 1). Dongle product string is "Cooler Master Mice Dongle", not the mouse model. V.

## 6. Quirks
* Report size differs by link (32 vs 64): choose by PID (dongle vs cable) or output report length.
* Never send `4A 02/04` (write), `44 01` (set profile), `4C 04/08`, `54 01` (commit), `48 ...` writes, `5C 02`, `4E`, `50 05`: all modify the mouse. Reads only: `46 81 00`, `4A 81 03`, `44 81 02`.
* MasterPlus running: not documented to conflict; both just use interface 3 (U).
* Interface 4 (EP 0x85/0x05) may be a firmware-update channel - the research deliberately never probed it; do not open it.

## 7. Sources
* https://github.com/syntactic-spice/coolermaster-usb-protocol/blob/main/docs/PROTOCOL.md
* https://github.com/AallynReed/FoxDen/blob/main/crates/foxden-battery/src/drivers/coolermaster.rs
* https://github.com/itsSamBz/OnDeck-Juice/blob/main/src/drivers/coolermaster.cpp
