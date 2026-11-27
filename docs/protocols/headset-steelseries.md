# SteelSeries Arctis (1 / 7 / 7+ / 9 / Pro Wireless / Nova 3P / Nova 5 / Nova 7 / Nova Pro Wireless / GameBuds)

Scope: battery percentage, charging state, headset-off/not-connected state, read from the 2.4 GHz base station / dongle over HID (Windows, raw HID, no vendor software required).
Vendor ID for everything here: **0x1038** (SteelSeries ApS).

Confidence tags: **[V]** verified in source (URL given), **[I]** inferred, **[U]** unknown.
All protocol facts below were extracted from HeadsetControl (GPL-3.0; facts only, no code copied) and re-expressed in my own words. Source folder: `https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/` plus the file named in each row.

## 0. Conventions used in this file

* "Request" = bytes written with HID output report semantics. HeadsetControl passes buffers through hidapi, where **byte 0 of the buffer is the report ID**; a leading `0x00` means "unnumbered report" and is NOT sent on the wire. Where a request in HeadsetControl starts with another value (`06 xx`, `40 AA`), that first byte is either a real report ID or, on unnumbered-report interfaces, simply payload byte 0 on the wire. **[I]** Which of the two applies per model is not stated in source. On Windows read the interface's HID descriptor (`HidP_GetCaps`, `HidP_GetValueCaps`) at runtime: if the output report has a Report ID, byte 0 of your WriteFile buffer is that ID; if not, byte 0 must be 0x00 and the payload follows (buffer length = `OutputReportByteLength`).
* "Reply" offsets are offsets into the buffer returned by a read, again in hidapi convention (report ID, if numbered, is byte 0).
* Pad every request with zeros to the report length (31 bytes for the legacy family, 64 for the Nova family).
* Every query is "write request, then read one input report with a ~1 s timeout". The dongle answers even when the headset is off (it reports that fact in the reply), except where noted.

## 1. Model table

| Family (HC file) | Models | PID(s) (VID 1038) | Interface / usage page : usage | Battery source |
|---|---|---|---|---|
| `steelseries_arctis_1.hpp` | Arctis 1 Wireless, Arctis 1 Xbox, Arctis 7X, Arctis 7P | 0x12b3 (1), 0x12b6 (1 Xbox), 0x12d7 (7X), 0x12d5 (7P) | iface 3, 0xFF43:0x0202 | direct % |
| `steelseries_arctis_7.hpp` | Arctis 7, Arctis 7 (2019), Arctis Pro (2019), Arctis Pro GameDAC | 0x1260, 0x12ad, 0x1252, 0x1280 | iface 5, 0xFF43:0x0202 (iface 5 has several collections on Windows; pick this one) | direct % (see quirk) |
| `steelseries_arctis_9.hpp` | Arctis 9 | 0x12c2, 0x12c4 (later rev, "reported, not verified") | iface 0 (no usage hint) | scaled byte |
| `steelseries_arctis_pro_wireless.hpp` | Arctis Pro Wireless | 0x1290 | iface 0 | 5 steps |
| `steelseries_arctis_7_plus.hpp` | 7+, 7+ PS5, 7+ Xbox, 7+ Destiny | 0x220e, 0x2212, 0x2216, 0x2236 | iface 3, 0xFFC0:0x0001 | 5 steps |
| `steelseries_arctis_nova_3p_wireless.hpp` | Nova 3P Wireless, Nova 3X Wireless | 0x2269, 0x226d | iface 3, 0xFFC0:0x0001 | direct % |
| `steelseries_arctis_nova_5.hpp` | Nova 5 base station, Nova 5X base station | 0x2232, 0x2253 | iface 3, 0xFFC0:0x0001 | direct % |
| `steelseries_arctis_nova_7.hpp`, `..._7p.hpp` | Nova 7, 7X, 7P, Diablo IV / WoW editions, "Gen 2", "v2" | see section 3.3 | iface 3, 0xFFC0:0x0001 | 5 steps or direct % by PID |
| `steelseries_arctis_nova_pro_wireless.hpp` | Nova Pro Wireless, Nova Pro Wireless X | 0x12e0, 0x12e5 | iface 4 (no usage hint) | 9 steps |
| `steelseries_arctis_gamebuds.hpp` | Arctis GameBuds | 0x230a | iface 3, 0xFFC0:0x0001 | direct %, per bud |
| `steelseries_arctis_nova_3.hpp` | Arctis Nova 3 (wired) | 0x12ec | iface 4, 0xFFC0:0x0001 | **no battery** (wired) |

All rows **[V]** from the file named in the first column (PIDs, interfaces, usages). HC uses the Arctis 1 protocol for 7X/7P.

## 2. Legacy family (Arctis 1 / 7 / 9 / Pro Wireless): 31-byte reports

### 2.1 Arctis 1 / 7X / 7P **[V]**
* Request: `06 12` (zero pad to 31 bytes).
* Reply: read 8 bytes. **byte[3] = battery percent 0..100** (used as-is). **byte[2] = status; 0x01 means headset off / not connected.** No charging flag is decoded.
* Source: https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/steelseries_arctis_1.hpp (shared logic: `protocols/steelseries_protocol.hpp`).

### 2.2 Arctis 7 / 7 (2019) / Pro / Pro GameDAC **[V]** with quirks
* Request: `06 18`.
* Reply: 8 bytes, **byte[2] = battery percent 0..100**. No offline byte, no charging byte decoded.
* Quirk **[V]**: HeadsetControl issue #347 reports the 2019 Arctis 7 (1038:12ad) returning a constant 0x38 (56 %) in byte[2] regardless of real charge; a Jan-2026 Linux kernel thread notes the mainline driver's Arctis 7 / 7 (2019) battery path was never implemented and just reports 100 %. Treat these models' battery as **[I] unreliable** until tested on hardware. URLs: https://www.github.com/Sapd/HeadsetControl/issues/347 and https://lkml.iu.edu/2601.2/01826.html
* Source: https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/steelseries_arctis_7.hpp

### 2.3 Arctis 9 **[V]**
* Request: `00 20` (31-byte buffer).
* Reply: 12 bytes. **byte[3] = raw battery**, linearly scaled **0x64 -> 0 %, 0x9A -> 100 %** (`(raw-100)*100/54`, clamp 0..100). **byte[4] == 0x01 -> charging**; HC also treats byte[4]==0xFF as "offline" (its own comment calls that value "unused", so **[I]** the offline meaning of 0xFF is weakly supported).
* Source: https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/steelseries_arctis_9.hpp . A blog walkthrough of the same device (not re-read in full, **[I]** consistent with HC): https://qsypoq.github.io/reverse-engineering-arctis-9-battery-tray/ . Mainline Linux `hid-steelseries.c` also implements Arctis 1 and 9 battery (not re-read).

### 2.4 Arctis Pro Wireless (0x1290) **[V]**
* Two steps:
  1. Request `41 AA`, read 2 bytes. **byte[0] == 0x02 -> headset off / not connected** (0x04 = online).
  2. Request `40 AA`, read 1 byte. **byte[0] = 0..4 -> 0/25/50/75/100 %**.
* No charging flag. Source: https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/steelseries_arctis_pro_wireless.hpp

## 3. Nova family: 64-byte reports, request `00 B0`

Shared helper **[V]**: write a 64-byte buffer `00 B0 00 ...` (report ID 0 + command 0xB0), then read up to 128 bytes. The reply's first byte is `0xB0` (command echo). Status reports (0xB0) can also arrive asynchronously, and other readers on the same HID interface (e.g. SteelSeries GG) receive the same reports; match on leading 0xB0 and loop a few reads.

Offsets below are **into the reply buffer (byte[0] = 0xB0)**.

### 3.1 Arctis 7+ (0x220e / 0x2212 / 0x2216 / 0x2236) **[V]**
* byte[1] == 0x01 -> headset not connected.
* byte[2] = battery level 0..4 -> 0/25/50/75/100 %.
* byte[3] == 0x01 -> charging.
* Source: https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/steelseries_arctis_7_plus.hpp
* Quirk **[V]**: the dongle keeps returning plausible chat-mix data when the headset is disconnected, so always test the offline byte first.

### 3.2 Nova 3P/3X Wireless and Nova 5/5X **[V]**
* byte[1] == 0x02 -> headset offline.
* **byte[3] = battery percent 0..100 (direct)**.
* byte[4] == 0x01 -> charging.
* Nova 5 requires a reply >= 16 bytes. Sources: https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/steelseries_arctis_nova_3p_wireless.hpp and `steelseries_arctis_nova_5.hpp`

### 3.3 Nova 7 family **[V]**
* **byte[3] == 0x00 -> headset not connected.** (Different offset than 3.1/3.2.)
* **byte[3] == 0x01 or 0x02 -> charging.** (Other non-zero values mean normal use; their exact values are **[U]**.)
* **byte[2] = battery.** Two encodings, chosen by product ID:
  * "discrete": 0..4 -> 0/25/50/75/100 %.
  * "percentage": 0..100 direct.
* Several PIDs moved from discrete to percentage after a **January 2026 firmware update** (comments in source). **[I]** One PID can therefore be seen with two firmware generations. Heuristic: for PIDs marked "discrete / pre-Jan-2026" accept byte[2] <= 4 as discrete steps, but if you see byte[2] > 4 treat it as percent.
* PID table (VID 0x1038) **[V]**:

| PID | Model (per source comment) | battery encoding |
|---|---|---|
| 0x2202 | Arctis Nova 7 | discrete |
| 0x22A1 | Arctis Nova 7 (Jan 2026 update) | percent |
| 0x227e | Nova 7 Wireless Gen 2 | percent |
| 0x2206 | Nova 7x | discrete |
| 0x2258, 0x229e, 0x22ad | Nova 7x v2 | percent |
| 0x223a | Nova 7 Diablo IV (before Jan 2026) | discrete |
| 0x22a9 | Nova 7 Diablo IV (after Jan 2026) | percent |
| 0x227a | Nova 7 WoW Edition | discrete |
| 0x22a4 | Nova 7X | discrete |
| 0x22a5 | Nova 7X | percent |
| 0x220a | Nova 7P (before Jan 2026) | discrete |
| 0x22a7, 0x2298 | Nova 7P V2 | percent |

* Sources: https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/steelseries_arctis_nova_7.hpp , `steelseries_arctis_nova_7p.hpp`. A Linux kernel series tested Nova 7 (0x2202) and 0x22a1 with the same offsets: https://lkml.iu.edu/2602.3/08373.html

### 3.4 Arctis Nova Pro Wireless (base 0x12e0, Pro Wireless X 0x12e5) **[V]**
* Request: `06 B0` (31-byte padded), read up to 128 bytes, require >= 16.
* byte[15] (status): **0x01 = headset off / not connected; 0x02 = charging via cable; 0x08 = online**.
* byte[6] = battery 0..8 -> `value * 100 / 8` (0/12/25/.../100 %).
* The base station has a swappable second battery; HC does not read it (**[U]**).
* Source: https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/steelseries_arctis_nova_pro_wireless.hpp

### 3.5 Arctis GameBuds (0x230a) **[V]**
* Request `00 B0`. byte[3] / byte[4] = left / right bud status (**0x03 = out of case / active, 0x02 = docked / off**); byte[5] / byte[6] = left / right battery 0..100. HC reports the lower of the active buds; both docked -> offline. No charging state (docking powers a bud off).
* Source: https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/steelseries_arctis_gamebuds.hpp

## 4. Quirks / timing
* **Dongle present, headset off** is normal and reported in-band (offline bytes above); the HID device stays enumerated. **[V]**
* Other software (GG / Sonar) shares the collection: reads are broadcast to every open handle, so another app can swallow your reply. Retry up to ~8 reads and match the leading command byte. **[V]** for the settings read in HC, **[I]** for battery.
* Legacy request first-byte ambiguity (report ID vs payload): see section 0. **[I]**
* Nova 7 / 5 / Pro can also be Bluetooth devices simultaneously; use `headset-bluetooth-generic.md` for the BT path. **[I]**

## 5. Sources
* HeadsetControl (GPL-3.0), facts only: https://github.com/Sapd/HeadsetControl (files named above and `lib/devices/protocols/steelseries_protocol.hpp`).
* Linux kernel series "HID: steelseries: Add support for Arctis headset lineup" (Jan 2026; derived from HeadsetControl): https://patchew.org/linux/20260112041941.40531-1-srimanachanta@gmail.com , https://lkml.iu.edu/2602.3/08370.html
* Arctis 9 blog: https://qsypoq.github.io/reverse-engineering-arctis-9-battery-tray/
