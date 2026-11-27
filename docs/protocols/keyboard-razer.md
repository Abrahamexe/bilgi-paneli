# Keyboard family: Razer (VID 0x1532) - 90-byte "Razer report" over HID feature reports

Legend: **V** verified in third-party source (URL), **I** inferred, **U** unknown.
The protocol is shared with Razer mice/headset-dongles; this file only lists keyboard-relevant parts. Compare with headset-razer.md for the 2.4 GHz headset framing.

## 1. Models with battery (V: openrazer `driver/razerkbd_driver.h`/`.c`)
| Model | Wired PID | Wireless/dongle PID | Notes |
|---|---|---|---|
| BlackWidow V3 Pro | 0x025A | 0x025C | |
| BlackWidow V3 Mini HyperSpeed | 0x0258 | 0x0271 | |
| BlackWidow V4 Mini HyperSpeed | 0x02B9 | 0x02BA | |
| BlackWidow V4 Tenkeyless HyperSpeed | 0x02D7 | 0x02D5 | polling-rate capable |
| BlackWidow V4 Low-Profile TKL HyperSpeed | 0x02D4 | 0x02D2 | polling-rate capable |
| DeathStalker V2 Pro | 0x0292 | 0x0290 | |
| DeathStalker V2 Pro TKL | 0x0298 | 0x0296 | |
| Joro (wired form listed with battery handling) | see header | - | |
Wired-only Razer keyboards: no battery. 8 kHz keyboards in openrazer: Huntsman V3 Pro (0x02CF 8KHz, TKL 0x02D0, Mini 0x02D1) etc. (FerrisPulse list) - polling readable only on the ones in section 4.

## 2. Transport (V)
* HID **feature report, report ID 0, 90-byte body** (91 bytes on Windows incl. the ID byte). Send with SetFeature, then wait, then GetFeature on the **same** collection. (openrazer uses USB control transfers `SET_REPORT/GET_REPORT` wValue 0x0300 to a given USB *interface number* `wIndex`; on Windows hidapi/HidD_* reproduces this, per https://github.com/xzeldon/razer-battery-report `razer-hid` crate.)
* Interface that answers (wIndex): **0x02** for BlackWidow V3 Pro (wired 0x025A and wireless 0x025C), V4 TKL HyperSpeed *wireless* (0x02D5), V4 Low-Profile TKL *wireless* (0x02D2), DeathStalker V2 Pro and V2 Pro TKL *wireless*; **0x03** for BlackWidow V3 Mini HyperSpeed (wired and wireless), V4 Mini (wired and wireless), DeathStalker V2 Pro / TKL *wired*, V4 TKL and V4 LP TKL *wired*, BlackWidow V4 / V4 Pro / V4 75 %, Huntsman V2 and V3 Pro lines, Joro; **default 0x01** otherwise. V: `razer_get_report_params()` in razerkbd_driver.c.
  On Windows this is the HID collection with `MI_02`/`MI_03` that has a 91-byte feature report. Which usage page/usage it has on keyboards is **U** (razer-battery-report prefers the mouse usage 0x0001/0x0002 because it is mice-only); safest: enumerate all collections of the matching `MI_xx` and try the feature report on each that advertises FeatureReportByteLength == 91.
* Wait between SetFeature and GetFeature: 600 us (wired), **4900 us for the wireless/dongle forms** (BlackWidow V3 Pro/Mini wireless, DeathStalker V2 wireless). Retry up to 5 times if the reply status is BUSY. V.
* Report layout (offset in the 90-byte body): `0 status | 1 transaction_id | 2..3 remaining_packets (BE) | 4 protocol_type (0) | 5 data_size | 6 command_class | 7 command_id | 8..87 arguments[80] | 88 crc | 89 reserved`. V (razercommon.h).
* **CRC** = XOR of body bytes 2..87 (inclusive) stored at byte 88. V.
* Reply status (byte 0): 0x01 busy, **0x02 success**, 0x03 failure, 0x04 timeout, 0x05 not supported. The reply must echo `remaining_packets`, `command_class`, `command_id`. V.
* **transaction_id** depends on the product and link: **0x1F** (wired HyperSpeed keyboards, Joro, DeathStalker V2 Pro wired, V4 TKL wired); **0x9F** (wireless forms: V3 Pro wireless 0x025C, V3 Mini/V4 Mini wireless, DeathStalker V2 wireless, V4 TKL wireless); **0x3F** (BlackWidow V3 Pro *wired* 0x025A); 0xFF for ordinary wired keyboards (non-battery commands). V (razer_attr_read_charge_level / charge_status / poll_rate switch statements).

## 3. Battery and charging (V)
| Query | class | command | data_size | Reply |
|---|---|---|---|---|
| Battery level | 0x07 | 0x80 | 0x02 | `arguments[1]` = 0..255 raw -> **percent = round(raw*100/255)** |
| Charging | 0x07 | 0x84 | 0x02 | `arguments[1]` = 0 not charging / 1 charging |
(arguments[1] is body byte 9; on a 91-byte Windows buffer it is index 10.) Example request body for battery (tid 0x1F): `00 1F 00 00 00 02 07 80 00...00 | crc=0x02^0x07^0x80=0x85`. Works through the dongle: the dongle PID answers on behalf of the keyboard; the wired PID answers about the *same* keyboard while cabled (keep both if both enumerated; prefer wired for "charging").
Readings are known to be flaky on BlackWidow V3 Pro (stuck at 1-2 % / 100 % in Synapse threads; **I** firmware, not protocol).

## 4. Polling rate (V)
Two generations in openrazer:
* **Old** (mice-era, 125/500/1000): class 0x00 cmd 0x85 size 1 -> arg[0]: 0x01=1000, 0x02=500, 0x08=125. (Not used for keyboards in openrazer.)
* **New "polling_rate2"** (HyperPolling, used by BlackWidow V4 / V4 Pro / V4 75% / V4 TKL HyperSpeed (wired 0x02D7, wireless 0x02D5) / V4 LP TKL (0x02D4/0x02D2) / Huntsman V2 / V2 TKL): **class 0x00, command 0xC0, size 0x01**, transaction id 0x1F (wired) or 0x9F (wireless forms); reply **`arguments[1]`**:
| code | Hz |
|---|---|
| 0x01 | 8000 |
| 0x02 | 4000 |
| 0x04 | 2000 |
| 0x08 | 1000 |
| 0x10 | 500 |
| 0x20 | 250 (listed in the setter) |
| 0x40 | 125 |
Set = class 0x00, cmd 0x40, size 2, `arguments[0]` = 0 (or 1 = store flag, "maybe varstore"), `arguments[1]` = code. Not needed.
Models not in the list above (e.g. BlackWidow V3 Pro): polling readback **U**.

## 5. Model name behind a dongle
The dongle PID tells which keyboard (table in section 1); no extra query needed. Serial: class 0x00 cmd 0x82 (openrazer `razer_chroma_standard_get_serial`) - **I** (not re-read here).

## 6. Quirks
* Do not issue other classes (0x03 LED, 0x0F matrix) - they change state; stay on 0x07/0x80, 0x07/0x84, 0x00/0xC0.
* Airoha-based 8 kHz devices return BUSY (status 0x01) during frame-sync; retry >=3 times with 15 ms backoff (FerrisPulse, V).
* Synapse running in background also polls; races produce BUSY, just retry.

## Sources
* https://github.com/openrazer/openrazer (driver/razerchromacommon.c, razercommon.c, razerkbd_driver.c/h)
* https://github.com/xzeldon/razer-battery-report (Rust Windows tray; crates/razer-hid)
* https://github.com/Ferris-echo/FerrisPulse/blob/main/docs/HARDWARE_PROTOCOLS.md section 1.1 (byte index with report ID, 8 kHz notes)
