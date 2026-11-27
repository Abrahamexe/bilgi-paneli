# Razer mice (wired, HyperSpeed / Wireless dongles, HyperPolling 4K/8K dongle)

Scope: battery %, charging, polling rate (Hz), read-only, Windows raw HID.
Research date: 2026-11-24. Tags: **V** verified in source (link), **I** inferred, **U** unknown, **HW** third-party hardware report.

License: OpenRazer (GPL-2) read for protocol facts only, byte layouts restated in our words, no code copied. The Windows-side
behaviour was cross-checked against two permissively licensed Windows apps: xzeldon/razer-battery-report (MIT, Rust, hidapi) and
HeyOkay/HaloBattery (MIT, Python).

## 1. Device identification

* VID = `0x1532` (Razer). V: openrazer driver/razercommon.h L19.
* One physical mouse shows up as several PIDs: wired (cable), wireless (mouse itself on the dongle) and *receiver/dongle*. The full list OpenRazer
  can query is in section 8 (generated from `razermouse_driver.h` / `razermouse_driver.c`, with the transaction ID per PID).
* Battery-capable mice (all PIDs whose "Battery txn" column in section 8 is not `-`): Mamba (2012/Chroma/Elite wireless), Lancehead (+TE wireless), DeathAdder V2 Pro,
  Viper Ultimate, Viper Mini SE, Viper V2 Pro, Viper V3 Pro/SE, Viper V4 Pro, DeathAdder V3 Pro/HyperSpeed, V4 Pro, Basilisk Ultimate, Basilisk V3 Pro, V3 Pro 35K,
  Basilisk V3 X HyperSpeed, V4 Pro, Naga Pro, Naga V2 Pro, Pro Click, Pro Click Mini, Pro Click V2 (+Vertical), Cobra Pro, Cobra HyperSpeed, Orochi V2, Atheris (AA, battery only),
  Basilisk Mobile, HyperPolling dongle (`00B3`), etc.
* AA/AAA mice (Atheris `0062`, Orochi V2 `0094/0095/00E0`, DeathAdder V2 X HyperSpeed `009C`, Basilisk X HyperSpeed `0083`, Naga V2 HyperSpeed `00B4`, Viper V3 HyperSpeed `00B8`,
  Basilisk V3 X HyperSpeed `00B9`, Basilisk Mobile `00D3/00D4`) answer battery but their *charging query returns a constant 0* in OpenRazer (no charging concept). V: razermouse_driver.c L1730-L1830.
* Extra PIDs not in the OpenRazer header but present in razer-battery-report's catalog (tested by users): Basilisk V4 Pro `00E2` (wired) / `00E3` (dongle), Naga V3 Pro `00E7` (wired) / `00E8` (dongle) - both need
  **USB interface 3** (Naga V3 Pro dongle wait 99.9 ms), Viper V4 Pro `00E5/00E6` interface 3. V/HW: https://github.com/xzeldon/razer-battery-report/blob/master/crates/razer-hid/src/catalog.rs L149-L157.

### Which HID interface / collection to open

* The command channel is a **HID feature report, report ID 0, 90 bytes of payload** (Windows hidapi: 91 bytes = `00` + 90). V: razercommon.h L121-L135 (struct razer_report = 90 bytes),
  razercommon.c L20-L45 (SET_REPORT value `0x300` = feature report ID 0), razer-battery-report transport.rs L100-L115.
* USB interface number: **0** for nearly every mouse; **3** for Viper V4 Pro (`00E5/00E6`), Basilisk V4 Pro (`00E2/00E3`), Naga V3 Pro dongle (`00E8`), Naga X (`0096`), Basilisk V3 (`0099`), Basilisk V3 35K (`00CB`).
  V: openrazer razermouse_driver.c L100-L120 (`index = 0x03`), razer-battery-report catalog.rs.
* Inside the interface, Windows splits collections. razer-battery-report tries the **generic mouse collection (usage page `0x0001`, usage `0x0002`) first** - "that collection answers on Windows" - then falls back to other
  collections of the same interface (discovery.rs L10-L12, L155, device.rs "exchange"). HW. Opening the mouse collection with 0 desired access (no read/write) is enough for `HidD_SetFeature/GetFeature`.
* The mouse with the dongle plugged AND the cable plugged shows up under two PIDs (e.g. `00A5` + `00A6`); the mouse then answers on the cable PID. Query both, prefer the one that answers.

## 2. Report layout (90 bytes, byte 0 = first byte after the report ID)

| Byte | Field | Notes |
|---|---|---|
| 0 | status | request: `0x00`. Response: `0x00` new, `0x01` busy, `0x02` OK, `0x03` failure, `0x04` timeout/no response (mouse asleep or off), `0x05` not supported |
| 1 | transaction ID | per device: `0x1F`, `0x3F`, or `0xFF` (see table); must not be 0 |
| 2-3 | remaining packets | big-endian, `0x0000` |
| 4 | protocol type | `0x00` |
| 5 | data size | number of argument bytes used |
| 6 | command class | |
| 7 | command ID | high bit set = "get" (device -> host) |
| 8..87 | arguments (80 bytes) | |
| 88 | CRC | XOR of bytes 2..87 inclusive |
| 89 | reserved | `0x00` |

V: razercommon.h L57-L139, razercommon.c L111-L125 (CRC loop covers byte index 2 up to 87), razerchromacommon.c (`get_razer_report`).

### 2.1 Exchange procedure (V: razercommon.c L47-L90, razermouse_driver.c L127-L175)

1. Build the 90-byte request, compute CRC.
2. `HidD_SetFeature` (91 bytes, ID 0).
3. **Wait** (see wait table below), then `HidD_GetFeature` (91 bytes, ID 0) on the same handle.
4. Validate: response `remaining_packets`, `command_class`, `command_id` equal the request's (otherwise another application's answer is in the buffer: retry), `status == 0x02`.
   OpenRazer treats `0x01 (busy)` as success; razer-battery-report retries on busy instead (a busy read carries no payload and can look like 0 %). Recommended: retry.
5. Retry up to 5 times, 10 ms apart (OpenRazer). On status `0x04` after retries: mouse asleep -> keep last reading.

Wait between Set and Get (OpenRazer constants `razermouse_driver.h` L138-L141): **600 us** wired/old, **31 ms** wireless receivers/new mice, **59.9 ms** Viper/V2 Pro/HyperPolling class, **400 ms** Atheris/Orochi V2 receivers
(per-PID values are in section 8; razer-battery-report adds 99.9 ms for the Naga V3 Pro dongle).

## 3. Battery request

Request bytes (after the ID byte), `T` = transaction id of the PID:

```
00 T 00 00 00 02 07 80 00 ... 00 [byte88 = 85] 00      (class 0x07, id 0x80, data size 2; CRC = 0x02^0x07^0x80 = 0x85)
```

Response: same header, `status=02`, data size 2, **arguments[1] = battery 0..255** (arguments[0] = 0). Percent = `round(arg1 * 100 / 255)`.
Real capture (DeathAdder V4 Pro dongle `00BF`): `02 1f 00 00 00 02 07 80 00 ab ...` -> 0xAB = 171 -> 67 %. HW: HaloBattery docs/protocols.md "Razer DeathAdder V4 Pro".
V: razerchromacommon.c L1051-L1057 ("0->255 is in arg[1]"), razermouse_driver.c L1622-L1660.

## 4. Charging request

```
00 T 00 00 00 02 07 84 00 ... 00 [byte88 = 81] 00      (class 0x07, id 0x84; CRC = 0x02^0x07^0x84 = 0x81)
```

Response `arguments[1]`: `0` = not charging, `1` = charging (V razerchromacommon.c L1061-L1067, razermouse_driver.c L1730-L1830). U: whether `arguments[1]` stays 1 when full on the cable differs per model (not documented); treat "charging && 100 %" as "fully charged on cable".
Charging status is not meaningful when the *dongle's* PID answers for a mouse that is currently on the cable and powering off the RF link; querying the cable PID is more reliable. I.
PIDs in the "-" or "n/a" charging column: do not send; AA mice have no charging.

## 5. Polling rate (Hz)

Two generations (V: razerchromacommon.c L1079-L1180, razermouse_driver.c L1942-L2150):

### 5.1 Legacy: class `0x00`, id `0x85`, data size 1

```
00 T 00 00 00 01 00 85 00 ... [byte88 = 84] 00           (CRC = 0x01^0x00^0x85 = 0x84)
```
Response `arguments[0]`: `0x01` = 1000 Hz, `0x02` = 500 Hz, `0x08` = 125 Hz. (Unknown other values: U. DeathAdder 3.5G uses its own table 1/2/3 - irrelevant here.)
Used for most wired + older wireless mice and for Viper V3 Pro *wired* `00C0` in OpenRazer.

### 5.2 Extended (up to 8 kHz): class `0x00`, id `0xC0`, data size 1

```
00 T 00 00 00 01 00 C0 00 ... [byte88 = C1] 00           (CRC = 0x01^0xC0 = 0xC1)
```
Response **`arguments[1]`**: `0x01` = 8000, `0x02` = 4000, `0x04` = 2000, `0x08` = 1000, `0x10` = 500, `0x20` = 250 (set-only in OpenRazer, read table omits it), `0x40` = 125.
(`arguments[0]` = 0 / varstore flag in the setter.) V: razerchromacommon.c L1119-L1147, razermouse_driver.c L2048-L2074.
OpenRazer's PID list for 0xC0: Viper 8K `0091`, Viper Mini SE `009E/009F`, **HyperPolling Wireless Dongle `00B3`**, DeathAdder V3 `00B2`, DeathAdder V3 Pro (wireless alt) `00C3`, Viper V3 Pro wireless `00C1`,
DeathAdder V4 Pro `00BE/00BF`, Viper V4 Pro `00E5/00E6`. I: newer models not yet in OpenRazer (Basilisk V4 Pro, Naga V3 Pro, Viper V3 Pro SE ...) probably also use 0xC0; procedure: send 0xC0 first; if the response status is `0x05`
(not supported) or the code is not in the table, send 0x85.
Note: for some PIDs OpenRazer uses 0x85 on the wired PID and 0xC0 on the wireless PID of the same mouse (e.g. Viper V3 Pro `00C0` wired = 0x85, `00C1` wireless = 0xC0): probe both.
U: request data size for 0xC0 is 1 in OpenRazer while its setter uses 2; not seen failing in OpenRazer, but no independent hardware confirmation of the *get* from us.

The 8K/4K *HyperPolling dongle* (`00B3`) is a standalone receiver for several mice (Viper V2 Pro, DeathAdder V3 Pro, ...); it answers battery + polling for the paired mouse using txn `0x1F`, wait 59.9 ms. V: OpenRazer lists.

## 6. Real model name

Razer exposes no "get name" in this protocol. The HID/USB product string is e.g. "Razer Viper V2 Pro" for the mouse PID, but a *receiver* PID (`0088`, `0077`, ...) is named after the receiver ("Razer Basilisk Ultimate Dongle"
style). Use a PID -> name table (section 8, same names as OpenRazer / razer-battery-report). The serial can be read with class 0x00 id 0x82 data size 0x16 (22 ASCII chars) - used for de-duplicating the cable and dongle copies
of one mouse. V: razer-battery-report report.rs L76-L80 (class 0, id 0x82, size 0x16), openrazer razermouse_driver.c L1595-L1610. U: some receiver PIDs reject it (txn 0x3F for Lancehead receiver serial, see its catalog).

## 7. Quirks

* **Mouse asleep**: dongle answers `status 0x04` (timeout) or garbage; HaloBattery/razer-battery-report keep the last level greyed. Moving the mouse wakes it. HW.
* Synapse running at the same time is fine (feature reports are request/response). Do not hammer: >1 request per second is pointless; use >= 10 ms between retries.
* Pairing dongles can answer garbage (`status` not 0/1/2/3/4/5, bad CRC) - validate CRC? OpenRazer does not verify the response CRC; razer-battery-report does check "bad checksum" and retries. Recommended: check.
* I: a 0 % reading with status 0x02 from a dongle is more likely "no data" than a truly empty battery; HaloBattery/razer-battery-report treat idle/asleep as a separate state - suppress low-battery alerts on a single 0.
* Transaction ID 0 is rejected (OpenRazer forces 0xFF). Keyboards additionally use 0x9F (not mice).
* A global mutex per physical device: do not interleave your own two requests on different threads (the response buffer is shared). V razermouse_driver.c L130-L135 (mutex).
* Do **not** send any SET command (anything with command ID bit7 = 0): they change the mouse state/profile.

## 8. PID table (generated from OpenRazer `razermouse_driver.c` 2026-11-24; V)

"Battery txn" = transaction ID OpenRazer uses for charge level; "Charging query" = `yes`/`n/a`(constant 0)/`-`(no entry); "Hz query" = `0x85` legacy or `0xC0` extended. Wait values are the per-PID
exchange wait in microseconds (the 3 constants above; `600(default)` if the PID is not in the special lists).
Wired-only mice (no battery) are included because they still answer the polling-rate query; their battery column is `-`.

| PID | OpenRazer name | Battery/charge txn id | Charging query | Hz query | wait us (OpenRazer) | USB if. (OpenRazer) |
|---|---|---|---|---|---|---|
| 0x0013 | Orochi 2011 | - | - | 0x85 | 600(default) | 0 |
| 0x0015 | Naga | - | - | 0x85 | 600(default) | 0 |
| 0x0016 | Deathadder 3 5G | - | - | 0x85 | 600(default) | 0 |
| 0x001F | Naga Epic | 0xFF | yes | 0x85 | 600(default) | 0 |
| 0x0020 | Abyssus 1800 | - | - | 0x85 | 600(default) | 0 |
| 0x0024 | Mamba 2012 Wired | 0xFF | yes | 0x85 | 600(default) | 0 |
| 0x0025 | Mamba 2012 Wireless | 0xFF | yes | 0x85 | 600(default) | 0 |
| 0x0029 | Deathadder 3 5G Black | - | - | 0x85 | 600(default) | 0 |
| 0x002E | Naga 2012 | - | - | 0x85 | 600(default) | 0 |
| 0x002F | Imperator | - | - | 0x85 | 600(default) | 0 |
| 0x0032 | Ouroboros | 0xFF | yes | 0x85 | 600(default) | 0 |
| 0x0034 | Taipan | - | - | 0x85 | 600(default) | 0 |
| 0x0036 | Naga Hex Red | - | - | 0x85 | 600(default) | 0 |
| 0x0037 | Deathadder 2013 | - | - | 0x85 | 600(default) | 0 |
| 0x0038 | Deathadder 1800 | - | - | 0x85 | 600(default) | 0 |
| 0x0039 | Orochi 2013 | - | - | 0x85 | 600(default) | 0 |
| 0x003E | Naga Epic Chroma | 0xFF | yes | 0x85 | 600(default) | 0 |
| 0x003F | Naga Epic Chroma Dock | 0xFF | yes | 0x85 | 600(default) | 0 |
| 0x0040 | Naga 2014 | - | - | 0x85 | 600(default) | 0 |
| 0x0041 | Naga Hex | - | - | 0x85 | 600(default) | 0 |
| 0x0042 | Abyssus | - | - | 0x85 | 600(default) | 0 |
| 0x0043 | Deathadder Chroma | - | - | 0x85 | 600(default) | 0 |
| 0x0044 | Mamba Wired | 0xFF | yes | 0x85 | 600(default) | 0 |
| 0x0045 | Mamba Wireless | 0xFF | yes | 0x85 | 600(default) | 0 |
| 0x0048 | Orochi Chroma | - | - | 0x85 | 600(default) | 0 |
| 0x004F | Deathadder 2000 | - | - | 0x85 | 600(default) | 0 |
| 0x0050 | Naga Hex V2 | - | - | 0x85 | 600(default) | 0 |
| 0x0053 | Naga Chroma | - | - | 0x85 | 600(default) | 0 |
| 0x0054 | Deathadder 3500 | - | - | 0x85 | 600(default) | 0 |
| 0x0059 | Lancehead Wired | 0x3f | yes | 0x85 | 600(default) | 0 |
| 0x005A | Lancehead Wireless | 0x3f | yes | 0x85 | 600(default) | 0 |
| 0x005B | Abyssus V2 | - | - | 0x85 | 600(default) | 0 |
| 0x005C | Deathadder Elite | - | - | 0x85 | 600(default) | 0 |
| 0x005E | Abyssus 2000 | - | - | 0x85 | 600(default) | 0 |
| 0x005F | Deathadder 2000 Alt | - | - | 0x85 | 600(default) | 0 |
| 0x0060 | Lancehead Te Wired | - | - | 0x85 | 600(default) | 0 |
| 0x0062 | Atheris Receiver | 0x1f | n/a (const 0) | 0x85 | 400000 | 0 |
| 0x0064 | Basilisk | - | - | 0x85 | 600(default) | 0 |
| 0x0065 | Basilisk Essential | - | - | 0x85 | 600(default) | 0 |
| 0x0067 | Naga Trinity | - | - | 0x85 | 600(default) | 0 |
| 0x006A | Abyssus Elite Dva Edition | - | - | 0x85 | 600(default) | 0 |
| 0x006B | Abyssus Essential | - | - | 0x85 | 600(default) | 0 |
| 0x006C | Mamba Elite | - | - | 0x85 | 600(default) | 0 |
| 0x006E | Deathadder Essential | - | - | 0x85 | 600(default) | 0 |
| 0x006F | Lancehead Wireless Receiver | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x0070 | Lancehead Wireless Wired | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x0071 | Deathadder Essential White Edition | - | - | 0x85 | 600(default) | 0 |
| 0x0072 | Mamba Wireless Receiver | 0x3f | yes | 0x85 | 31000 | 0 |
| 0x0073 | Mamba Wireless Wired | 0x3f | yes | 0x85 | 31000 | 0 |
| 0x0077 | Pro Click Receiver | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x0078 | Viper | - | - | 0x85 | 600(default) | 0 |
| 0x007A | Viper Ultimate Wired | 0xFF | yes | 0x85 | 59900 | 0 |
| 0x007B | Viper Ultimate Wireless | 0xFF | yes | 0x85 | 59900 | 0 |
| 0x007C | Deathadder V2 Pro Wired | 0x3f | yes | 0x85 | 59900 | 0 |
| 0x007D | Deathadder V2 Pro Wireless | 0x3f | yes | 0x85 | 59900 | 0 |
| 0x0080 | Pro Click Wired | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x0083 | Basilisk X Hyperspeed | 0xFF | n/a (const 0) | 0x85 | 31000 | 0 |
| 0x0084 | Deathadder V2 | - | - | 0x85 | 600(default) | 0 |
| 0x0085 | Basilisk V2 | - | - | 0x85 | 600(default) | 0 |
| 0x0086 | Basilisk Ultimate Wired | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x0088 | Basilisk Ultimate Receiver | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x008A | Viper Mini | - | - | 0x85 | 600(default) | 0 |
| 0x008C | Deathadder V2 Mini | - | - | 0x85 | 600(default) | 0 |
| 0x008D | Naga Left Handed 2020 | - | - | 0x85 | 600(default) | 0 |
| 0x008F | Naga Pro Wired | 0x1f | yes | 0x85 | 600(default) | 0 |
| 0x0090 | Naga Pro Wireless | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x0091 | Viper 8K | - | - | 0xC0 | 600(default) | 0 |
| 0x0094 | Orochi V2 Receiver | 0x1f | n/a (const 0) | 0x85 | 400000 | 0 |
| 0x0095 | Orochi V2 Bluetooth | 0x1f | n/a (const 0) | 0x85 | 400000 | 0 |
| 0x0096 | Naga X | - | - | 0x85 | 600 | 3 |
| 0x0098 | Deathadder Essential 2021 | - | - | 0x85 | 600(default) | 0 |
| 0x0099 | Basilisk V3 | - | - | 0x85 | 600 | 3 |
| 0x009A | Pro Click Mini Receiver | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x009C | Deathadder V2 X Hyperspeed | 0x1f | n/a (const 0) | 0x85 | 31000 | 0 |
| 0x009E | Viper Mini Se Wired | 0x1f | yes | 0xC0 | 59900 | 0 |
| 0x009F | Viper Mini Se Wireless | 0x1f | yes | 0xC0 | 59900 | 0 |
| 0x00A1 | Deathadder V2 Lite | - | - | 0x85 | 600(default) | 0 |
| 0x00A3 | Cobra | - | - | 0x85 | 600(default) | 0 |
| 0x00A5 | Viper V2 Pro Wired | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00A6 | Viper V2 Pro Wireless | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00A7 | Naga V2 Pro Wired | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00A8 | Naga V2 Pro Wireless | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00AA | Basilisk V3 Pro Wired | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00AB | Basilisk V3 Pro Wireless | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00AF | Cobra Pro Wired | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00B0 | Cobra Pro Wireless | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00B2 | Deathadder V3 | - | - | 0xC0 | 600(default) | 0 |
| 0x00B3 | Hyperpolling Wireless Dongle | 0x1f | yes | 0xC0 | 59900 | 0 |
| 0x00B4 | Naga V2 Hyperspeed Receiver | 0x1f | n/a (const 0) | 0x85 | 31000 | 0 |
| 0x00B6 | Deathadder V3 Pro Wired | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00B7 | Deathadder V3 Pro Wireless | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00B8 | Viper V3 Hyperspeed | 0x1f | n/a (const 0) | 0x85 | 59900 | 0 |
| 0x00B9 | Basilisk V3 X Hyperspeed | 0x1f | n/a (const 0) | 0x85 | 31000 | 0 |
| 0x00BE | Deathadder V4 Pro Wired | 0x1f | yes | 0xC0 | 31000 | 0 |
| 0x00BF | Deathadder V4 Pro Wireless | 0x1f | yes | 0xC0 | 31000 | 0 |
| 0x00C0 | Viper V3 Pro Wired | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00C1 | Viper V3 Pro Wireless | 0x1f | yes | 0xC0 | 59900 | 0 |
| 0x00C2 | Deathadder V3 Pro Wired Alt | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00C3 | Deathadder V3 Pro Wireless Alt | 0x1f | yes | 0xC0 | 31000 | 0 |
| 0x00C4 | Deathadder V3 Hyperspeed Wired | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00C5 | Deathadder V3 Hyperspeed Wireless | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00C7 | Pro Click V2 Vertical Edition Wired | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00C8 | Pro Click V2 Vertical Edition Wireless | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00CB | Basilisk V3 35K | 0x1f | - | 0x85 | 600 | 3 |
| 0x00CC | Basilisk V3 Pro 35K Wired | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00CD | Basilisk V3 Pro 35K Wireless | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00D0 | Pro Click V2 Wired | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00D1 | Pro Click V2 Wireless | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00D3 | Basilisk Mobile Wired | 0x1f | n/a (const 0) | 0x85 | 31000 | 0 |
| 0x00D4 | Basilisk Mobile Receiver | 0x1f | n/a (const 0) | 0x85 | 31000 | 0 |
| 0x00D6 | Basilisk V3 Pro 35K Phantom Green Edition Wired | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00D7 | Basilisk V3 Pro 35K Phantom Green Edition Wireless | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00DA | Cobra Hyperspeed Wired | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00DB | Cobra Hyperspeed Wireless | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00DE | Viper V3 Pro Se Wired | 0x1f | yes | 0x85 | 31000 | 0 |
| 0x00DF | Viper V3 Pro Se Wireless | 0x1f | yes | 0x85 | 59900 | 0 |
| 0x00E0 | Orochi V2 2026 Receiver | 0x1f | n/a (const 0) | 0x85 | 400000 | 0 |
| 0x00E5 | Viper V4 Pro Wired | 0x1f | yes | 0xC0 | 59900 | 3 |
| 0x00E6 | Viper V4 Pro Wireless | 0x1f | yes | 0xC0 | 59900 | 3 |

## 9. Sources

* OpenRazer (GPL-2): https://github.com/openrazer/openrazer/tree/master/driver - `razercommon.h`, `razercommon.c`, `razerchromacommon.c` (L1051-L1180), `razermouse_driver.c` (battery L1622, charging L1730, polling L1942, per-PID waits L36-L120), `razermouse_driver.h` (PIDs, wait constants L138-L141)
* razer-battery-report (MIT): https://github.com/xzeldon/razer-battery-report/tree/master/crates/razer-hid/src (catalog.rs, discovery.rs, device.rs, transport.rs, report.rs)
* HaloBattery (MIT): https://github.com/HeyOkay/HaloBattery/blob/main/providers/razer.py and docs/protocols.md (Basilisk V3 Pro/Ultimate, DeathAdder V4 Pro, Viper V4 Pro confirmed on hardware)
