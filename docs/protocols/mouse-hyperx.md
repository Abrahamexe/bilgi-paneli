# HyperX mice (Pulsefire Dart, Pulsefire Haste 2 Wireless / NGENUITY "universal" protocol)

Scope: battery %, charging, polling rate. Research date 2026-11-24. Tags: **V** verified in source, **I** inferred, **U** unknown, **HW** third-party hardware report.
Licenses: santeri3700/hyperx_pulsefire_dart_reverse_engineering (CC0), PMarinov1994/ngenuity-linux-cli (MIT), HaloBattery (MIT), OpenRGB (GPL-2, PIDs only). Nothing copied.

## 1. Two protocol generations

| Generation | Mice | VID : PID | Source |
|---|---|---|---|
| **Kingston-era "NGenuity2" (Dart)** | Pulsefire Dart | wireless dongle `0951:16E1` (HP-era `03F0:068E`), wired `0951:16E2` (HP-era `03F0:088E`) | V OpenRGB HyperXMouseControllerDetect.cpp L25-L38, Dart RE protocol/index.md L1-L5 |
| **NGENUITY 3 "universal" protocol** | Pulsefire Haste 2 Wireless (dongle `03F0:0F98`, also `03F0:0D97`), Haste 2, Haste 2 Pro, Haste 3 (+Pro/Wireless), Saga (+Pro) | VID `0x03F0` (HP) | V (tested Haste 2 Wireless) ngenuity-linux-cli docs/devices/README.md; PIDs V HaloBattery providers/hyperx_ngenuity.py L41-L46 |
| Older wired (no battery) | Surge `0951:16D3`/`03F0:0490`, FPS Pro `0951:16D7`, Core `0951:16DE`/`03F0:0D8F`, Raid `0951:16E4`, Haste (wired) `0951:1727`/`03F0:0F8F` | | V OpenRGB detect L27-L38 |

U: Pulsefire Haste Wireless (1st gen), Haste 3 Wireless, Saga Pro, Surge Wireless PIDs/protocol: not published; assumed (I) to use the NGENUITY 3 protocol because NGENUITY 3.1.0 lists them under it (ngenuity-linux-cli devices page marks them "untested").

## 2. Pulsefire Dart (`0951:16E1` / `16E2`)

Transport: 64-byte interrupt reports, **no report ID** (descriptor: vendor page `0xFF00`, usage 1; input usage 2 x 64 bytes, output usage 3 x 64 bytes, feature usage). V Dart RE misc/usb_hid_report_descriptors_0951_16e1_wireless.txt. Windows: report ID 0 + 64 bytes.
* Wireless dongle: **USB interface 2, usage page `0xFF00`**. Wired mouse: **interface 1, usage page `0xFF13`**. V protocol/index.md L3-L4; OpenRGB detect L135-L138.

Byte 0 = command; replies come as input reports of 64 bytes whose byte 0 is the same command number.

### 2.1 Battery + charging: "get heartbeat"

Request (output report, 64 bytes): `51 00 00 00 ...` (byte 0 = `0x51`). V protocol/index.md "Get heartbeat".
Reply (input report):

| Byte | Meaning |
|---|---|
| 0 | `0x51` |
| 3 | `0x03` (payload length) |
| **4** | **battery percent 0..100** |
| **5** | **charging: 0 = not charging, 1 = charging** |
| 6 | unknown (`0x0F` seen) |
| 7-8 | changes with battery level (probably voltage, unconfirmed) |

V protocol/index.md "Report: Heartbeat". HW (author's unit, capture `ngenuity_battery_heartbeats_57_to_100`). Also an unsolicited event `FF 03 <profile> <cable> <sleep> ...`: byte 3 = cable (1 = connected/charging), byte 4 = 0 going to sleep / 1 wake.
Connectivity report (sent by the device, not requested): `46 00 00 <01 wireless | 02 wired> <state>`; for wireless, byte 4 = 1 when the mouse is connected via the dongle. V protocol/index.md "Mouse connectivity status".

### 2.2 Polling rate

* Set: `D0 00 00 01 <code>`; code 0 = 125 Hz, 1 = 250, 2 = 500, 3 = 1000. V protocol/index.md "Set polling rate".
* **Read: U** (no documented getter; NGenuity never reads settings back - "The official software does not read settings from the mouse"). Show n/a (max 1000 Hz).

### 2.3 Name: hardware-information request `50 ...` -> reply `50 00 00 1D <PID lo> <PID hi> <VID lo> <VID hi> <fw build> <rev> <minor> <major> "HyperX Pulsefire Dart"` (ASCII at byte 12). V protocol/index.md "Mouse hardware information report".

## 3. NGENUITY 3 universal protocol (Haste 2 Wireless and family)

Source: https://github.com/PMarinov1994/ngenuity-linux-cli/blob/main/docs/protocol/universal.md (MIT; reverse engineered from NGENUITY 3.1.0, **tested on a Pulsefire Haste 2 Wireless**; HaloBattery re-implements it, "unverified on this app's side").

* Transport (V universal.md L24-L33): the **dongle's USB interface 2**, vendor usage page `0xFF00`, usage `0x0001` (HaloBattery picks by `ff00:0001`, interface 2, 64-byte packets). Every packet is 64 bytes: **byte 0 = report ID = command**, byte 1 = sub-command, rest payload, zero padded. Declared output/feature report IDs: `10 12 14 20 22 30 32 36 40 42 44 50 52 60 62 F0 FA FC`; replies are **input reports with ID = command + 1** (`11 13 15 21 23 31 33 37 41 43 45 51 53 61 F1 FB FD FF`). Send with `HidD_SetOutputReport`/`WriteFile` (output) - the descriptor also declares each ID as feature; ngenuity-linux-cli uses hidraw writes = output reports. I for Windows API choice: use WriteFile with the exact report ID and 64 bytes total (63 payload after the ID? see below).
  Note on lengths: the ID is byte 0 and "63 bytes" follow. V.
* Pace >= 10 ms between packets (what NGENUITY does). V.

### 3.1 Battery + charging

Request: report `50`, sub `02`: `50 02 00 ... (64 bytes total)`. Reply: input report `51 02 ...`:

| Byte | Meaning |
|---|---|
| 0-1 | `51 02` |
| **2** | **battery level % 0..100** (the mouse's own gauge) |
| **3** | **charge status: 0 not charging, 1 charging, 2 full, 3 charger error, 0xFF unknown** |
| 4-5 | temperature degC, i16 little-endian |
| 6-7 | voltage mV, u16 little-endian |
| 8 / 9 | fuel-gauge chip type / charger chip type |

V universal.md L184-L186, HaloBattery providers/hyperx_ngenuity.py L2-L14. The mouse also sends `51 02` by itself right after waking (universal.md L267).
Link state: `52 01` -> `53 01 <connected>` (byte 2: 1 = mouse connected, 0 = asleep/off). V.
A sleeping mouse: no answer -> keep last value (HaloBattery greys it for 5 min). HW (Haste 2 Wireless).

### 3.2 Polling rate

Request `32 02 <profile>` ("DPISettings information request", profile 0 = live) -> reply `33 02 ...` **without the two profile bytes**; payload layout (V universal.md L162-L170):

```
reply byte 2   polling rate code
reply byte 3   enabled-stage bitmask
reply byte 4   active stage
reply byte 5+5i  stage i: u16 LE (DPI/50 - 1), R, G, B
```

Polling code: `01` = 8000 Hz, `02` = 4000, `04` = 2000, `08` = 1000, `10` = 500, `20` = 250, `40` = 125. (V universal.md L164; the setter table uses the same codes.)
Ambiguity: the doc's write form `32 01` carries profile at [2..3] and polling at [4]; the read form drops the two profile bytes, so polling is at reply index 2. I (derived from "reads it back as `33 02` without the two profile bytes"); confirm on hardware: the byte equal to one of the 7 codes right after `33 02`.
Max rate per model: Haste 2 Wireless / Haste 2 Pro 4K / Haste 3 Pro 8K (devices page notes). V.

### 3.3 Name

`10 01` DeviceInformation -> reply `11 01 ...` (see device notes) contains product IDs (`0x0D97` appears) and a name string. U for exact offsets (read ngenuity-linux-cli docs/devices/pulsefire-haste-2-wireless.md). Fallback: PID table (section 1) or the USB product string of the dongle (it is the mouse name for HP-era dongles, I).

### 3.4 Quirks

* Never send `12 01` (factory reset), `F0/FA/FC` (firmware), or any `xx 01`/`xx 02` setter: many sub 01/02 commands are writes. Read-only set: `50 02`, `52 01`, `32 02`, `10 01`, `14 02`.
* Wake-up: the dongle answers `52 01` even if the mouse is asleep. Check link first.
* NGENUITY must be closed? Not required for reads (HaloBattery did not note a conflict); the dongle may serve both. U.
* Reports are exchanged via the vendor collection only; the pointer/keyboard/consumer collections on interfaces 0/1 must not be touched.

## 4. Sources
* https://github.com/santeri3700/hyperx_pulsefire_dart_reverse_engineering (protocol/index.md, README.md, misc/usb_hid_report_descriptors_*)
* https://github.com/PMarinov1994/ngenuity-linux-cli (docs/protocol/universal.md, docs/devices/README.md)
* https://github.com/HeyOkay/HaloBattery/blob/main/providers/hyperx_ngenuity.py and docs/protocols.md ("HyperX Pulsefire Haste 2 Wireless")
* OpenRGB PIDs: https://github.com/CalcProgrammer1/OpenRGB/blob/master/Controllers/HyperXMouseController/HyperXMouseControllerDetect.cpp
