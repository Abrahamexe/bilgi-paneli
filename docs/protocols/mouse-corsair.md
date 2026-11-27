# Corsair mice (iCUE / Slipstream): "NXP" legacy protocol and "Bragi" protocol

Scope: battery %, charging, polling rate, read-only, Windows raw HID. Research date 2026-11-24.
Tags: **V** verified in source, **I** inferred, **U** unknown, **HW** third-party hardware report.

License: ckb-next (GPL-2) and OpenRGB (GPL-2) were read for protocol facts only (restated, nothing copied). HaloBattery (MIT) was used as a Windows hardware cross-check.
Important: ckb-next talks raw USB interrupt endpoints (libusb); the byte layouts transfer 1:1 to HID output/input reports on Windows, but the *report framing* (report ID, size) is from OpenRGB/HaloBattery.

VID = `0x1B1C` (Corsair). Two unrelated protocol generations:

| Family | Mice | Where |
|---|---|---|
| **NXP** (64-byte, field GET/SET) | Dark Core RGB (`1B35` wired / `1B64` dongle), Dark Core RGB SE (`1B4B` wired / `1B51` dongle), and all old wired RGB mice (M65, Sabre, Scimitar, Glaive, Harpoon, Ironclaw, Nightsword, Katar) | ckb-next `nxp_proto.h` |
| **Bragi** (a.k.a. "v2", "Slipstream" protocol) | Ironclaw Wireless (`1B4C` USB / `1B66` Slipstream dongle), Harpoon Wireless (`1B5E` / `1B65`), Dark Core RGB Pro (`1B80` / `1B81`), Dark Core RGB Pro SE (`1B7E` / `1B7F`), M55 RGB Pro `1B70`, Katar Pro `1B93`, Katar Pro XT `1BAC`, Katar Pro V2 `1BBA`, M65 RGB Ultra (wired `1B9E`, wireless `1BB5`), M75 / M75 Wireless (`1BF0`/`1BF1`, dongle `1BDC`), Scimitar Elite RGB (Bragi) `1BE3`, MM700 `1B9B` / 3XL `1BC9`, Ironclaw Wireless SE (`2B32`, dongle `2B00`), generic Slipstream dongles `1BA6`, `1B65`, `1B66` | ckb-next `bragi_proto.h`, OpenRGB CorsairPeripheralV2 |

Sources for the PID list: ckb-next src/daemon/usb.h L120-L190, OpenRGB Controllers/CorsairPeripheralV2Controller/CorsairPeripheralV2ControllerDetect.cpp L92-L141 / CorsairPeripheralV2Devices.h. V.
Newer models not in either project (Sabre RGB Pro Wireless, Nightsabre Wireless, Darkstar Wireless, Scimitar RGB Elite Wireless, M75 Air, Katar Elite Wireless, ...): PIDs **U**; expected Bragi. HaloBattery ("Corsair Dark Core RGB Pro SE", "M75 Wireless") confirms the Bragi battery exchange on dongles `1B7F` and `1BDC`. HW.

## 1. Bragi protocol (all current wireless Corsair mice)

### 1.1 HID collection / framing (Windows)

* Interface 1, vendor usage page **`0xFF42`**. V: OpenRGB CorsairPeripheralV2ControllerDetect.cpp L92-L141 (`REGISTER_HID_DETECTOR_IP(..., 1, 0xFF42)`).
* Packets are written with `hid_write` as **report ID `0x00` + payload**. OpenRGB uses a 1025-byte buffer (ID + 1024) for v2 devices (CORSAIR_V2_PACKET_SIZE) and a 65-byte `CORSAIR_V2_WRITE_SIZE` for the short transfers; ckb-next pads to 1024 (`BRAGI_JUMBO_SIZE`) or 64/128 (`MSG_SIZE`, `BRAGI_LARGE_SIZE`). Required length = the output report size in the HID descriptor of that interface (query with `HidP_GetCaps`); zero-pad. V/I.
* Byte 0 of the payload (after ID) = **route byte**: `0x08` = the device you opened (wired mouse, or the dongle itself); **`0x08 | n`** = talk *through the dongle to the child device n* (n = 1..7; `0x09` = first paired mouse).
  OpenRGB: `CORSAIR_V2_WRITE_WIRED_ID = 8`, `CORSAIR_V2_WRITE_WIRELESS_ID = 9`; ckb-next `pkt[0] |= bragi_child_id`. V: CorsairPeripheralV2Controller.h L27-L28, usb_bragi.c L63-L70. HW: HaloBattery docs ("route 0x09 asks the mouse behind the receiver (the receiver itself is 0x08)").
* Wired mouse on its cable: route `0x08`. Dongle: `0x09` for the paired mouse, `0x08` for the dongle itself (the dongle also reports connection bits: notification `0x36`, byte 4 = bitmask of connected children, bit i = child i). V: bragi_proto.h L50-L52, bragi_notification.c L25-L32.

### 1.2 Property get / set

Request (read-only usage):

```
[0] 0x08 (or 0x09 for child 1)     route
[1] 0x02                           GET (BRAGI_GET)
[2] <property>                     
[3] 0x00
rest zero
```

Response: `[0]` route/echo, `[1]` echo of command `0x02`, **`[2]` = error code (0 = OK, `0x05` = not supported)**, **value = little-endian 24-bit at bytes [3],[4],[5]** (`b3 | b4<<8 | b5<<16`).
V: ckb-next bragi_common.c L5-L21. HW: HaloBattery: answer `01 02 00 <lo> <hi>` (leading `01` = route byte observed on the wire in this reply), "the level in tenths of a percent (`26 02` = 550 = 55 %)".
Note the HaloBattery observation that the response echoes the *command*, not the property: another app (iCUE, SignalRGB) can leave an answer for a different property on the channel; HaloBattery asks twice and accepts two equal answers. Do the same.

### 1.3 Properties (ckb-next bragi_proto.h L17-L49)

| ID | Name | Use |
|---|---|---|
| `0x01` | POLLRATE | read: see 1.5 |
| `0x0F` | **BATTERY_LEVEL** | value / 10 = percent (0..1000) |
| `0x10` | **BATTERY_STATUS** | 1 = charging, 2 = discharging, 3 = fully charged (ckb-next enum order UNKNOWN=0, CHARGING=1, DISCHARGING=2, CHARGED=3) |
| `0x11` / `0x12` | VID / PID | PID of the *child* mouse when asking through a dongle (OpenRGB uses it to resolve the model) |
| `0x13` / `0x14` | app / bootloader fw version | |
| `0x15` / `0x16` | radio app / bootloader version | |
| `0x36` | SUBDEVICE_BITFIELD | dongle: connected children |
| `0x96` | MAX_POLLRATE | "untested, only known because CUE complained about it" |

### 1.4 Battery

* `GET 0x0F` -> `percent = value / 10` (V ckb-next device_bragi.c L316-L325: `battery_level = chg / 10`). HW: HaloBattery (Dark Core RGB Pro SE dongle, M75 Wireless dongle `1BDC`, "same level SignalRGB does").
* `GET 0x10` -> status 1 charging / 2 discharging / 3 charged (V device_bragi.c L317-L324, GUI enum batterysystemtrayicon.h L7-L12). **I**: not hardware-confirmed here; HaloBattery reports "No charging state is reported" for the dongle path, i.e. use the status property only on a wired mouse, or if the answer is in 1..3.
* Dongle device itself (route 0x08) has no battery (ckb-next `FEAT_BATTERY` only for non-dongle Bragi, usb.c L443-L447).

### 1.5 Polling rate (Bragi)

* `GET 0x01` (POLLRATE): value 1 = 8 ms (125 Hz), 2 = 4 ms (250 Hz), 3 = 2 ms (500 Hz), **4 = 1 ms (1000 Hz)**. V: bragi_proto.h L59-L62, device_bragi.c L183-L186 (`pollrate = prop - 1`).
* `GET 0x96` MAX_POLLRATE: same coding, so it tells the highest rate the device supports; used by ckb-next as `maxpollrate`. V device_bragi.c L188-L191.
* U: 2000/4000/8000 Hz Corsair mice (M75 Air, Sabre Pro 8K-class, "Slipstream 8K" dongles): codes above 4 are not in any open source. If `GET 0x01` returns a value > 4 treat as "unknown rate (code N)".

### 1.6 Model name

* Through the dongle: `GET 0x12` (PID of child) then PID -> name table (OpenRGB `CorsairPeripheralV2Devices.h`). The dongle's USB string is "CORSAIR SLIPSTREAM WIRELESS USB Receiver". V OpenRGB CorsairPeripheralV2Controller.cpp L36-L48. (Also `0x11` VID = 0x1B1C.)
* On the cable the USB product string is the real model name.

### 1.7 Quirks

* ckb-next's daemon keeps a heartbeat: iCUE "polls devices every 52 seconds" with `08 12` and ckb-next sends `08 12` every 50 s to keep software control. Read-only monitoring does not need it, but a mouse in iCUE "software mode" can be left alone. V device_bragi.c L14-L35.
* Packet pacing: OpenRGB waits up to 50 ms for the response (`CORSAIR_V2_TIMEOUT = 50`), 3 ms for short reads (V CorsairPeripheralV2Controller.h L24-L25); ckb-next waits 2 s max. HaloBattery uses a per-dongle time budget and time window.
* **Conflict with iCUE**: iCUE holds the endpoint; reads from a second process can steal/lose responses (see 1.2). Use two-equal-answers or retry.
* A sleeping wireless mouse behind the dongle returns nothing / error; keep last value. HaloBattery. HW.
* Some Windows hosts expose several `0xFF42` collections; pick the one whose output report length matches (1025 vs 65) - I.

## 2. NXP protocol (Dark Core RGB / SE wireless, legacy)

* ckb-next vtable: `get_battery_info = nxp_get_battery_info` for NXP wireless devices. V device_vtable.c L171, L332.
* Transport: interrupt OUT of 64 bytes on USB interface 1 (usage page `0xFFC2` in OpenRGB's HID registrations for the whole NXP mouse family, `REGISTER_HID_DETECTOR_IP(..., 1, 0xFFC2)`), report ID 0 on Windows. V OpenRGB CorsairPeripheralControllerDetect.cpp L181-L194. I for the exact Windows report length (64 + ID).
* Battery request: 64-byte buffer `0E 50 00 ...` (`CMD_GET = 0x0E`, `FIELD_BATTERY = 0x50`), response 64 bytes:
  * `in[4]` = level step 0..4 -> percent {0, 15, 30, 50, 100}
  * `in[5]` = status: raw 1 = discharging, 2 = charging (ckb-next swaps them into its enum: 2 discharging, 1 charging), 3 = fully charged.
  V: ckb-next device.c L261-L290 (nxp_battery_lut = 0,15,30,50,100; `if in[4] > 4: invalid`). Only 5 coarse steps - show as approximate.
* Polling rate: `CMD_SET 0x07, FIELD_POLLRATE 0x0A` is **write-only** in ckb-next ("Write only", nxp_proto.h L15). **U** for reading; show n/a.
* Only the dongle/wireless PIDs expose battery; wired Dark Core on cable: `1B35`/`1B4B` also reports (IS_WIRELESS includes wired variants, usb.h L283) - I.

## 3. Wired Corsair mice

No battery. Polling rate: NXP write-only; Bragi wired mice (Katar Pro, M55, M65 Ultra wired ...) `GET 0x01` works. Wired NXP mice: n/a.

## 4. Sources

* ckb-next (GPL-2): https://github.com/ckb-next/ckb-next/tree/master/src/daemon - `bragi_proto.h`, `bragi_common.c` (get/set), `device_bragi.c` (L14-L35 heartbeat, L183-L191 pollrate, L316-L325 battery), `usb_bragi.c` (route byte), `bragi_notification.c`, `device.c` (L261-L290 NXP battery), `nxp_proto.h`, `usb.h`, `device_vtable.c`
* OpenRGB (GPL-2): https://github.com/CalcProgrammer1/OpenRGB/tree/master/Controllers/CorsairPeripheralV2Controller (detect: usage page `0xFF42`, interface 1; header constants L21-L36; PID query L31-L48) and CorsairPeripheralController (NXP, `0xFFC2`)
* HaloBattery (MIT): https://github.com/HeyOkay/HaloBattery/blob/main/docs/protocols.md "Corsair Dark Core RGB Pro SE" (+ providers/corsair.py)
