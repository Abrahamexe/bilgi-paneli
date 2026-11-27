# Logitech G headsets (G533, G535, G633/G635/G733/G933/G935, G PRO, G PRO X, G PRO X 2, G PRO X 3, G522)

VID **0x046d** for all. Tags: **[V]** verified in source (URL), **[I]** inferred, **[U]** unknown.
Source (facts only, GPL-3.0): https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/ - `logitech_g533.hpp`, `logitech_g535.hpp`, `logitech_g633_g933_935.hpp`, `logitech_gpro.hpp`, `logitech_gpro_x2_lightspeed.hpp`, `logitech_gpro_x3_lightspeed.hpp`, `logitech_g522_lightspeed.hpp`, `protocols/hidpp_protocol.hpp`, `protocols/logitech_calibrations.hpp`, `protocols/logitech_centurion_protocol.hpp`. HID++ framing reference cited by HC: https://lekensteyn.nl/files/logitech

Three protocol generations:
1. **HID++ 2.0 over the receiver, battery as voltage (mV)** - G533, G535, G633/G635/G733/G933/G935, G PRO, G PRO X, (G PRO X 2 first-gen path).
2. **Centurion** (HID++-like framing with a "bridge" to the headset; battery as direct percent) - G PRO X 2 LIGHTSPEED (also has a "legacy" vendor path), G PRO X 3, G522.
3. G930 (old feature-report protocol), G431/G432/G433 (wired, no battery), Zone, G435 (no known battery interface).

## 1. HID++ voltage family

### 1.1 Frame **[V]**
Long HID++ message, report ID **0x11**, 20 bytes: `11 FF <featureIndex> <function|swId> <params...>`. Device index 0xFF (= receiver/"headset behind receiver" in HC). HC pads the request to 20 bytes and reads **7 bytes** of the reply.
Reply layout: `[0]=11 [1]=FF [2]=featureIndex [3]=function/swId echo [4..5]=voltage mV big-endian [6]=state`.
* **state byte[6]: 0x03 = charging, 0x01 = idle/discharging.** **[V]**
* **byte[2] == 0xFF in the reply = HID++ error / headset off or not connected.** **[V]** (HC treats it as offline).
* Voltage below the lowest calibration point -> reading unavailable (HC returns "unavailable", level unknown). **[V]**

### 1.2 Per-model constants **[V]**

| Model | PID(s) | Request bytes after `11 FF` | HID collection hint (HC) |
|---|---|---|---|
| G533 | 0x0a66 | `07 01` | usage page 0xFF43, usage 0x0202, interface 3 |
| G535 | 0x0ac4 | `05 0d` | usage page 0x000C, usage 0x01, interface 3 |
| G633 / G635 / G733 / G933 / G935 | 0x0a5c / 0x0a89 / 0x0a5b / 0x0a87 / G733: 0x0ab5, 0x0afe, 0x0b1f | `08 0a` | usage page 0xFF43, usage 0x0202 |
| G PRO (wireless), G PRO X, G PRO X 2 (HID++ variant) | 0x0aa7 (G PRO), 0x0aaa & 0x0aba (PRO X), 0x0afb & 0x0afc (PRO X 2 variants) | `06 0d` | usage page 0xFF43, usage 0x0202 |

* Feature indexes `07/05/08/06` are firmware-specific table positions **[V]** (hard-coded in HC). **[I]** They very probably point at HID++ 2.0 feature **0x1001 "Battery Voltage"** or **0x1F20 "ADC Measurement"** (kernel bugzilla 216483 documents 0x1F20 reporting battery on Logitech devices: https://bugzilla.kernel.org/show_bug.cgi?id=216483). A robust implementation resolves the index at runtime with IRoot (`11 FF 00 0x<featId hi><lo>`: feature index 0, function 0, params = 16-bit feature ID; reply byte[4] = feature index) instead of hard-coding. **[I]**
* Windows collection choice: HID++ exposes separate top-level collections for short (0x10, 7 B), long (0x11, 20 B) and very-long (0x12) reports; pick the one whose output report 0x11 exists (usage page 0xFF43/0xFF00 on most, 0x000C on G535 per HC). **[I]**

### 1.3 Voltage -> percent calibration (HC `logitech_calibrations.hpp`) **[V]**
HC interpolates with a spline through these (percent: voltage mV) points; piecewise-linear is an acceptable approximation. Values below the last (lowest) voltage are treated as "unavailable".

| Family | 100 % | 80 | 60 | 50 | 40 | 30 | 20 | 10 | 5 | 0 |
|---|---|---|---|---|---|---|---|---|---|---|
| G533 | 4200 | | | 3850 | | 3790 | 3750 | | 3680 | 3330 |
| G535 | 4175 | | | 3817 | | 3766 | 3730 | | 3664 | 3310 |
| G633/G933/G935/G733 (HC default family) | 4100 | 3950 | 3850 | | 3750 | | 3650 | 3500 | 3300 | 3150 |
| G PRO / PRO X / PRO X 2 (HID++) | 4150 | | | 3830 | | 3780 | 3740 | | 3670 | 3320 |
| G430/G432 (wired; for reference) | 4100 | | | 3800 | | 3750 | 3700 | | 3600 | 3300 |
| Default (unknown Logitech) | 4100 | | | 3850 | | 3780 | 3730 | | 3650 | 3300 |

(The G633 row is 100/80/60/40/20/10/5/0 -> 4100/3950/3850/3750/3650/3500/3300/3150.) Confidence on exactness of curves: HC comments call them "conservative values" (G633) or condensed from a 48-point manual curve (G535); **[I]** error is likely +/- 10 % in the flat region 3.7-3.85 V.

### 1.4 Quirks **[V]/[I]**
* G PRO X 2 has two firmware personalities: HID++ (PIDs 0x0afb/0x0afc listed above) and the Centurion/vendor protocol below (PID 0x0af7).
* Voltage rises while charging (on the charger the reading is higher than the open-circuit curve), so while `state==0x03` show "charging" and either hide the % or clamp - HyperX Flight uses the same trick (see `headset-hyperx.md`). **[I]**
* Timeout 1-2 s; the receiver answers with 0xFF at byte[2] quickly when the headset is off. **[V]**

## 2. G PRO X 2 LIGHTSPEED (PID **0x0af7**) - Centurion + legacy fallback **[V]**

* Collection: **usage page 0xFFA0, usage 0x0001, interface 3**, 64-byte reports, report ID **0x51**.
* Frame (direct to dongle): `51 <len> <flags=00> <featureIndex> <function|swId(0x01)> <params...>` where `len = payload bytes + 1` (HC sends the full 64-byte payload length for X2; "exact length" mode for X3). Reply frames use the same `51 <len> <flags>` header; match `payload[0] == featureIndex` and `payload[1] == function|swId` where relevant.
* Headset is behind a **bridge feature (0x0003)**: sub-message `00 <subFeatureIndex> <function|swId> <params>` is wrapped as `<bridgeIdx> <0x10|swId> <len hi nibble> <len lo> ...`. Replies: ACK with function nibble 0x1 / swId 0x1, then the response with swId 0 and `payload[4]==0x00` (sub-CPL), sub-feature at `payload[5]`, data from `payload[7]`.
* Discovery: Root (feature 0x0000) `getFeature(featureId)` returns index; FeatureSet (0x0001) enumerates; **Centurion Battery SoC = feature 0x0104**.
* **Battery**: send function 0x00 of feature 0x0104 through the bridge. Reply data: **byte[0] = percent (0..100, reject >100)**, **byte[2] = charge state: 1 or 2 = charging**, else discharging. **[V]** (`parseCenturionBatteryResponse`).
* **Headset off**: bridge status (bridge function 0) params `03 e8 00 00 01` = headset off, `03 e8 00 01 01` = on; unsolicited bridge event `[bridgeIdx, 00, 00, connected]` (connected=0 -> off). A bridge rejection (`FF <bridgeIdx> 0x11 ...`) can also mean off. **[V]**
* **Legacy vendor path** (used if feature 0x0104 returns "not supported"): request (64 B) `51 08 00 03 1a 00 03 00 04 0a ...`; reply must be `byte[0]==0x51, byte[1]==0x0b, byte[8]==0x04`; **battery = byte[10]** (0..100), **charging if byte[12]==0x02**. Off packets: `51 05 .. .. .. .. 00` (byte[6]==0) = powered off; `51 03` = ack; other `51 05` = power events to skip. **[V]**
* Rated life 50 h; G HUB reported 47 h at 94 % (HC linear estimate). **[V]** Not needed for the tray.

## 3. G PRO X 3 LIGHTSPEED (receiver PID **0x0b45**) and G522 LIGHTSPEED (PID **0x0b18**) **[V]**
* Same Centurion engine, **frame prefix `50 23`** instead of `51` (so frame = `50 23 <len> <flags> ...`), usage page 0xFFA0 / usage 0x0001 / interface 3.
* G PRO X 3 is "verified on USB revision 1.02"; feature indexes looked up per feature with Root.getFeature; connection events + offline probing enabled.
* Battery request/decode identical to section 2 (feature 0x0104, byte[0] = %, byte[2] in {1,2} = charging).
* Sources: `logitech_gpro_x3_lightspeed.hpp`, `logitech_g522_lightspeed.hpp`.

## 4. G930 (PID 0x0a1f) **[V]**
Feature-report based; send a fixed 64-byte feature report (`FF 09 00 FD F4 10 05 B1 BF A0 04 ...`), then read the feature report three times with 100 ms gaps (protocol requirement). Usage page 0xFF00 / usage 0x01. Decode not captured here **[U]**. Very old model; skip unless needed.

## 5. No battery over HID
* G431/G432/G433 (0x0a9b/0x0a9c/0x0a6d) wired, HC supports sidetone only. **[V]**
* G435 (dongle PID 0x0acb): vendor collections exist (report 6 feature/input, report 0x34) but no battery command documented in the public teardown https://dev.to/endes/usb-hid-down-the-rabbit-hole-logitech-g435-dongle-33if **[U]**
* Zone Wired: wired. **[V]**

## 6. Sources
* HeadsetControl files listed at top.
* Linux kernel patch adding PRO X 2 battery over the vendor HID collection (not re-read): https://lwn.net/Articles/1093805/
* Kernel bugzilla on Logitech ADC measurement battery feature: https://bugzilla.kernel.org/show_bug.cgi?id=216483
