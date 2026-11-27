# Keyboard family: Logitech HID++ 2.0 (VID 0x046D) - G915/G913/G Pro, MX Keys/Mechanical, K-series, via Unifying / Bolt / LIGHTSPEED / Bluetooth / USB

Legend: **V** verified in third-party source (URL), **I** inferred (from spec knowledge, not re-read in source), **U** unknown. This is the **best documented** family; same framing as headset-logitech.md (G435/PRO X use the same feature IDs), so a single HID++ driver covers keyboards + mice + headsets.

## 1. Identifiers
| Item | Value | Conf |
|---|---|---|
| VID | 0x046D | V |
| Receivers | Unifying `C52B`, `C532`; Bolt `C548`; LIGHTSPEED `C539`, `C53A`, `C53D`?, `C545`, `C547`, `C54D` | V (C52B/C532/C548/C547/C54D in vigil-watt and FerrisPulse), I (the rest) |
| Wired / direct | keyboard USB PID itself speaks HID++ with device index `0xFF` | V (FerrisPulse: wired direct address FF) |
| Bluetooth keyboards (MX Keys/K380/K780...) | HID++ over a vendor HID collection (usage page 0xFF43 for BLE direct devices, 20-byte long reports) ; plus standard GATT BAS | V (FF43 20-byte channel seen for G PRO X Wireless, FerrisPulse), I for keyboards |
| Receiver collections | short `0xFF00/0x0001`, 7-byte reports (ID 0x10); long `0xFF00/0x0002`, 20-byte reports (ID 0x11). Bolt and LIGHTSPEED **answer only on the long collection**; Unifying often only on the short one | V (vigil-watt, FerrisPulse) |

Writing a 0x10 short frame to a long-only collection fails with `ERROR_INVALID_FUNCTION`; read the collection's report ID from HID caps instead of hard-coding. V (vigil-watt section 3).

## 2. Frame (V)
* Short (7 B): `[0x10, devIdx, featIdx, (func<<4)|swId, p0, p1, p2]`
* Long (20 B): `[0x11, devIdx, featIdx, (func<<4)|swId, p0..p15]`  (64-byte "very long" = report 0x12, rarely needed)
* `devIdx`: receiver slot 1..6 (Bolt/Unifying/LIGHTSPEED), `0xFF` for the device itself over USB/BT. `swId` = any 1..15 chosen by the app (vigil uses 0x0B, FerrisPulse uses 8); **0 is reserved for device-initiated notifications**, so an app can recognise unsolicited charge notifications by `(byte3 & 0x0F)==0`.
* Error reply: featIdx byte = `0x8F` (HID++1.0) or `0xFF` (2.0), bytes 1..2 echo, byte 3.. error code.
* Feature index discovery: **IRoot (index 0x00), function 0 `GetFeature(featureID hi, featureID lo)`** -> reply `[featureIndex, type, version]`; index 0 = feature absent. The first request after wake can take ~1 s; use >=1.5 s timeout for the ping, 500 ms afterwards (V: vigil-watt, FerrisPulse). A protocol ping = IRoot function 1 (`txPing[3] = 0x18` with swId 8 in FerrisPulse).
* Note on indexes in some bindings: the reply buffer returned *after* a feature request may start at the feature byte; params then sit at index 2 (V vigil section 3). With raw HID buffers (report ID at 0) params start at index 4.

## 3. Battery  (V: Solaar `lib/logitech_receiver/hidpp20*.py`, vigil-watt, FerrisPulse)
Try in this order (vigil-watt's cascade): `0x1004` UNIFIED_BATTERY -> `0x1000` BATTERY_STATUS -> `0x1001` BATTERY_VOLTAGE -> `0x1F20` ADC_MEASUREMENT -> HID++1.0 registers 0x07/0x0D (old Unifying).

| Feature | Call | Reply layout (params only) |
|---|---|---|
| **0x1004** Unified Battery | function **1** (`GetStatus`; function 0 = GetCapabilities - **do not** read battery from it; build request byte as `(featIdx, (1<<4)|swId)`) | `p0` = state of charge % (0 => unknown, use level), `p1` = level bit (8 full, 4 good, 2 low, 1 critical), `p2` = charge status, `p3` = ext. Example (G PRO X SUPERLIGHT 2): `11 01 06 19 59 08 00 00` = 89 % discharging; wired `11 FF 06 19 3C 04 01 01` = 60 % charging (p2=1). |
| **0x1000** Battery Status | function 0 | `p0` = % now, `p1` = % at next level, `p2` = status |
| **0x1001** Battery Voltage | function 0 | `p0..p1` = millivolts (big endian), `p2` flags: bit7 = charging, bits1:0 = charge state (1 = full), bit3 fast charge, bit4 slow, bit5 critical. Percent must be estimated from a Li-ion table (Solaar interpolates: 4186 mV=100, 4067=90, 3989=80, 3922=70, 3859=60, 3811=50, 3778=40, 3751=30, 3717=20, 3671=10, 3646=5, 3579=2, 3500=0) |
| **0x1F20** ADC Measurement | function 0 | mV (BE) + flags `01` discharging / `03` charging / `07` full; used by G PRO X Wireless headset and some keyboards |
Charge status code meaning (0x1000 / 0x1004): `0` discharging, `1` recharging, `2` almost full, `3` full, `4` slow recharge, `5+` fault. (Sample data above supports 0/1; vigil-watt treats 1/2/4 as charging, 3 as full; FerrisPulse prose lists a different mapping - ignore it, rely on samples.) 
Real-time: when the cable is plugged, the device sends an unsolicited notification with `swId==0` and the same payload as GetStatus - a passive reader (no writes) can update instantly (V vigil-watt).

## 4. Polling rate (keyboards that have it: G915/G915 TKL/G Pro X TKL/G Pro Keyboard 2.0 ...)
* **0x8060 REPORT_RATE** (classic, 1-8 ms): function 0 = list -> `p0` bitmask, bit i set => (i+1) ms supported; read = function **1** (Solaar uses `read_fnid 0x10`) -> `p0` = interval in ms (1..8) ; write = function 2. Hz = 1000/ms. V (Solaar settings_templates `ReportRate`).
* **0x8061 EXTENDED_ADJUSTABLE_REPORT_RATE** (up to 8 kHz): list = function 1 (`0x10`) -> 16-bit mask over indexes 0..6; read = **function 2** (`0x20`) with param `0x00`=wired / `0x01`=wireless (observed on PRO X SUPERLIGHT 2: param 01 -> index 03); write = function 3 (`0x30`). Index -> interval: 0=8 ms (125 Hz), 1=4 ms (250), 2=2 ms (500), 3=1 ms (1000), 4=500 us (2000), 5=250 us (4000), 6=125 us (8000). V (Solaar + FerrisPulse sample).
* Only one of the two features exists per device; if neither is present: U (fall back to USB interval). Onboard profiles (0x8100) may override host settings.

## 5. Model name / receiver slot
* Feature **0x0005 DEVICE_NAME**: function 0 = name length (`p0`), function 1(`charIndex`) = next ASCII chunk (text starts at params[0], i.e. index 4 in a raw 20-byte frame). V.
* Receiver pairing registers (HID++1.0, register 0xB5, "pairing info"): Unifying `0x2N` slot -> `[3..4]` = WPID (BE), `[7]` = device kind; Bolt `0x5N` -> `[1]` = kind, `[2..3]` = WPID (LE, swap). Device kinds identify "keyboard". V (vigil-watt).
* Wired devices: the USB PID/product string is enough; for LIGHTSPEED receivers shared by a keyboard and a mouse enumerate slots 1..2 (device index) and call IRoot ping for each.

## 6. Quirks
* G915 TKL: enabling host mode breaks Fn keys; read-only queries are fine.
* The HID++ long channel coexists with Logitech G HUB / Options+ (they hold the interface open but sharing is allowed on Windows for HID collections without exclusive access); heavy parallel traffic can cause "busy" errors.
* Voltage-only keyboards (0x1001) give mV only: percent is an estimate; label as such.
* LIGHTSPEED receiver shared with other devices: map reply `devIdx`.

## Sources
* https://github.com/pwr-Solaar/Solaar (lib/logitech_receiver/hidpp20.py, hidpp20_constants.py, settings_templates.py) - GPL, facts only
* https://github.com/Mightling/vigil-watt/blob/main/docs/hardware/protocols.md (MIT; sections "Logitech")
* https://github.com/Ferris-echo/FerrisPulse/blob/main/docs/HARDWARE_PROTOCOLS.md sections 1.2 + MR0097 log
* https://github.com/Ap0LoG1zE/EZ2.4G/blob/main/c/PROTOCOLS.md (Logitech 0x1004/0x1000 offsets with report ID)
