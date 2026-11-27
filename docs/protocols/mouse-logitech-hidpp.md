# Logitech mice: HID++ 1.0 / 2.0 (Lightspeed, Unifying, Bolt, Nano, wired, Bluetooth)

Scope: battery %, charging state, polling rate (Hz), real model name, over Windows raw HID, read-only.
Research date: 2026-11-24. Line numbers refer to the files as fetched that day (branches move).

Confidence tags: **V** = verified in source (link), **I** = inferred, **U** = unknown.
"HW" = a third-party project reports it confirmed on real hardware (we did not test).

License note: Solaar (GPL-2), Linux kernel `hid-logitech-hidpp.c` / `hid-logitech-dj.c` (GPL-2) were read for
protocol facts only; every byte layout below is restated in our own words, no code copied. The voltage curves
are numeric facts; the 100-entry tables are not reproduced, links are given.

## 1. Device identification

VID = `0x046D` (Logitech). Lenovo/other OEM Nano receivers can use other VIDs (e.g. `0x17EF:0x6042`), V: Solaar `base_usb.py` L64-L72.

### 1.1 Receivers (USB, HID++ interface = USB interface 2 unless noted)

| Kind | PIDs | HID++ USB interface | Source |
|---|---|---|---|
| Bolt | `C548` | 2 | V: Solaar base_usb.py L135-L140 |
| Unifying | `C52B`, `C532` | 2 | V: base_usb.py L142-L143 |
| Lightspeed (gaming) | `C539`, `C53A` (PowerPlay), `C53D`, `C53F`, `C541`, `C545`, `C547`, `C54D`; kernel also lists `C543` | 2 | V: base_usb.py L146-L153; kernel hid-logitech-dj.c L2089-L2103 |
| Nano (office, G-series old) | `C518`,`C51A`,`C51B`,`C521`,`C525`,`C526`,`C52E`,`C52F`,`C531`,`C534`(2 slots),`C535`(Dell-branded),`C537` | 1 | V: base_usb.py L127-L141 |
| EX100 27 MHz | `C517` | 1 | V: base_usb.py |

Dell/HP/Lenovo office mice that ship a "Logitech Nano" receiver are therefore HID++ devices (read them exactly like Logitech ones).

### 1.2 Wired / charging-cable USB mice that speak HID++ directly (device index `0xFF`)

From the kernel match table (kernel hid-logitech-hidpp.c L4877-L4912) and Solaar descriptors.py L410-L427:

| PID | Mouse | PID | Mouse |
|---|---|---|---|
| `C07D` | G502 | `C08B` | G502 SE Hero |
| `C07E` | G402 | `C08D` | G502 Lightspeed (cable) |
| `C081` | G900 | `C090` | G703 Hero |
| `C082` | G403 Wireless (cable) | `C091` | G903 Hero |
| `C086` | G903 | `C094` | G Pro X Superlight (cable) |
| `C087` | G703 | `C095` | G502 X Plus (cable) |
| `C088` | G Pro (cable) | `C098` | G502 X Lightspeed (cable) |
| `C08A` | MX Vertical (cable) | `C09A` | G PRO 2 Lightspeed (cable) |
| `C093` | M500S | `C09B` | G Pro X Superlight 2 (cable) |
| `C08E` | MX518 | `C092` | two mice share this PID |

The list is not exhaustive (newer mice appear in Logitech releases constantly). Do NOT rely on a PID table: enumerate every
`VID 046D` device that exposes the HID++ collections (1.3) and identify it by HID++ itself (section 5).

Wireless PIDs (WPID, what the *receiver* reports for a paired mouse, not a USB PID): HID++ 1.0 mice `0x1001-0x1029`, HID++ 2.0
mice `0x4002-0x4101` (e.g. `4093` PRO X Wireless, `4082` MX Master 3). Bluetooth PIDs (`B012` MX Master, `B019` MX Master 2S,
`B020` MX Vertical, `B023` MX Master 3 ...). V: Solaar descriptors.py, base_usb.py header comment L5-L9.

### 1.3 Which HID collection to open (Windows)

Windows enumerates one top-level collection per usage. Open the vendor collections:

| Collection | Report ID | Total length (with ID) | Confidence |
|---|---|---|---|
| usage page `0xFF00`, usage `0x0001` | `0x10` (HID++ short) | 7 bytes | V: kernel hid-logitech-dj.c L556-L568 (descriptor: 1 ID + 6) + HW: HaloBattery logitech.py header |
| usage page `0xFF00`, usage `0x0002` | `0x11` (HID++ long) | 20 bytes | V: same, L569-L580 |
| usage page `0xFF00`, usage `0x0004` | `0x12` (very long) | 64 bytes | I: kernel constants L44-L50 (`REPORT_ID_HIDPP_VERY_LONG`, max 64); not seen as a separate collection in sources |
| usage page `0xFF43`, usage `0x0202` | `0x11` | 20 bytes | V (HW): HaloBattery docs/protocols.md: used by G535 headset and long-only devices. I for mice over Bluetooth LE (Bluetooth HID++ is only exposed this way) |

A mouse on its USB cable exposes the same two `0xFF00` collections, device index `0xFF` (V: HaloBattery logitech.py L9; Solaar uses 0xFF for
direct devices). Some devices implement only the long report (`0x11`); send long for everything (kernel falls back to long when short is not
declared: hid-logitech-hidpp.c L437-L439). Always send HID++ 2.0 requests as **long** (`0x11`, 20 bytes); HID++ 1.0 register reads as short
(`0x10`, 7 bytes) when the short collection exists.

Receiver mi_02 may also carry DJ reports `0x20/0x21` (V: dj.c L582-L600) - ignore them.

Open with share-mode read/write, never exclusive: G HUB / Logi Options+ keep the same handle open. HaloBattery states it "works alongside
G HUB" (HW).

## 2. Frame format

Long HID++ 2.0 request (V: kernel hid-logitech-hidpp.c L395-L416, Solaar base.py L436-L460):

```
byte 0   0x11            report ID (long)
byte 1   device index    receiver slot 1..6 ; 0xFF = the receiver itself (HID++1.0) or a direct wired device ; 0x00 appears in Bluetooth replies
byte 2   feature index   (from ROOT lookup; 0x00 = ROOT)
byte 3   (function << 4) | software-id      (low nibble 1..15; use 0x0A..0x0F, never 0 so replies can be told from notifications)
byte 4.. params (up to 16 bytes), zero padded to 20 bytes total
```

Reply: same header; params start at byte 4. Error replies:

* HID++ 2.0 error: `11 idx FF <feature idx> <fn|sw> <err> ...` (err 1 unknown, 2 invalid argument, 3 out of range, 4 hardware error, 5 Logitech
  internal, 6 invalid feature index, 7 invalid function, 8 busy, 9 unsupported). V: Solaar hidpp20_constants.py L281-L290, base.py L735-L745; kernel L246.
* HID++ 1.0 error: `10 idx 8F <sub-id> <address> <err>` with err 1 invalid sub-id (= "I am a HID++ 1.0 device"), 2 invalid address, 3 invalid value,
  4 connect fail, 5 too many devices, 6 already exists, 7 busy, 8 unknown device (empty slot), 9 resource error (slot paired but device off/asleep),
  0x0A request unavailable. V: kernel L221-L234.

Match a reply to your request by `(byte1 == idx) && (byte2 == feature idx) && (byte3 == fn|sw)`. Notifications from other software (G HUB) arrive on
the same collection; ignore anything that does not match.

Timeouts (V: Solaar base.py L129-L133, HaloBattery logitech.py L57-L59): receiver registers 0.9 s, device requests ~0.6 s, first request to a dozing radio up to ~2 s.

## 3. Protocol detection and feature lookup (HID++ 2.0)

1. Ping / protocol version: `11 idx 00 1S 00 00 5A` (ROOT fn 1, `S` = sw id, param[2] = arbitrary marker). Reply `11 idx 00 1S <major> <minor> <marker>`.
   If the reply is an `10 idx 8F 00 1S 01` error the device is HID++ 1.0. V: kernel L959-L1006, Solaar base.py L804-L860.
2. Feature index lookup: ROOT fn 0 = `11 idx 00 0S <featureID hi> <featureID lo>`. Reply param[0] = feature index (0 = not supported), param[1] = feature flags,
   param[2] = version. V: kernel L946-L968.

Feature IDs used below: `0x0003` device info, `0x0005` device name, `0x1000` battery level status, `0x1001` battery voltage, `0x1004` unified battery,
`0x1F20` ADC measurement, `0x8060` report rate, `0x8061` extended adjustable report rate. V: Solaar hidpp20_constants.py L33-L55, L145-L146.

## 4. Battery (HID++ 2.0)

Try the features in this order and use the first the device has (HaloBattery order 0x1004, 0x1000, 0x1001, then 0x1F20; HW). A device normally exposes just one of them:

### 4.1 `0x1004` UNIFIED_BATTERY (modern mice: G502 X, Superlight 2, MX Master 3S ...)

* Capabilities, fn 0: request `11 idx fi 0S`. Reply param[0] = supported level flags (bit0 critical, bit1 low, bit2 good, bit3 full),
  param[1] flags (bit0 rechargeable, **bit1 = device reports exact state of charge %**). V: kernel L1568-L1620.
* Status, **fn 1**: request `11 idx fi 1S`. Reply:

| Param | Meaning |
|---|---|
| 0 | state of charge, 0..100 (valid only if capabilities.param[1] bit1) |
| 1 | level flags: 8 full, 4 good, 2 low, 1 critical (use when no %) |
| 2 | charging status: 0 discharging, 1 charging, 2 charging slow, 3 charge complete, 4 error, other = not charging |
| 3 | external power status (bit0 = cable present) |

V: kernel L1622-L1690, Solaar hidpp20.py L1900-L1905, L2244-L2265. Charging flag = status in {1,2}; "full" = 3 (report 100 %).

### 4.2 `0x1000` BATTERY_LEVEL_STATUS (G Pro Wireless, G903, older MX)

fn 0: params 0 = % (when discharging), 1 = next-level %, 2 = status: 0 discharging, 1 recharging, 2 charge in final stage, 3 charge complete (then use 100 %),
4 recharging below optimal speed, 5-7 errors. **While charging the % byte is 0 = unknown**: keep the last known value. V: kernel L1186-L1275.
fn 1 (`0x10`) = capability (param0 = number of levels, param1 flags bit1 = mileage/exact %). If levels < 10 or no mileage flag, only coarse levels (critical <11 %,
low <30, normal <81, full) are real. V: kernel L1276-L1310.

### 4.3 `0x1001` BATTERY_VOLTAGE (G502 Lightspeed, G703, many Lightspeed mice, MX Master 2S)

fn 0: params 0..1 = voltage in mV big-endian; param 2 flags: bit7 = charging; (flags & 7) when charging: 0 charging, 1 full, 2 not charging;
bit3 fast charge, bit4 slow/trickle charge, bit5 critical level. V: kernel L1391-L1450, Solaar hidpp20.py L2207-L2241.

Voltage -> % (Li-ion curve, not exact; mouse cells vary): Solaar's 13-point piecewise-linear table (hidpp20.py L2272-L2300), (mV -> %):
4186->100, 4067->90, 3989->80, 3922->70, 3859->60, 3811->50, 3778->40, 3751->30, 3717->20, 3671->10, 3646->5, 3579->2, 3500->0 (linear interpolation between points).
The kernel uses a finer 100-entry table (`hidpp20_map_battery_capacity`, hid-logitech-hidpp.c L1465-L1495) with the same shape (4186 mV = 100 %, 3537 mV = 1 %). I: curve is
approximate for the G502 LS-type cells; when charging the voltage is inflated, so show the last discharging value or just the charging icon.

### 4.4 `0x1F20` ADC_MEASUREMENT (headsets, rare on mice)

fn 0: params 0..1 mV BE, param 2 flags: bit0 = valid/connected, bit1 = charging. Different curve (4030 mV = 100 % ... 3399 mV = 1 %), kernel L1856-L1895. An error reply means "present but inactive".

### 4.5 HID++ 1.0 mice (no feature table: M185-era, MX Revolution, G700, etc.)

Short register read: `10 idx 81 <reg> 00 00 00` (GET_REGISTER = 0x81). Reply `10 idx 81 <reg> p0 p1 p2`. V: kernel L610-L650, L675-L850, Solaar hidpp10_constants.py L188-L190.

* Register `0x0D` (BATTERY_MILEAGE): p0 = percent; p2 >> 6: 0 discharging, 1 charging, 2 full, 3 error. (Percent valid when discharging/full.)
* Register `0x07` (BATTERY_STATUS): p0 = level step 1..7 (1-2 critical, 3-4 low, 5-6 normal, 7 full); p1 = status byte: `0x00` discharging, `0x21` charging, `0x22` charge complete,
  `0x24` fast charging, `0x25` slow charging, `0x26` topping charge, `0x23` error, `0x20` unknown.
* Try 0x0D first, then 0x07 (HaloBattery order, HW on an MK710 keyboard).

## 5. Real model name (receiver says "Receiver")

* HID++ 2.0 device (all modern): feature `0x0005`. fn 0: param0 = name length. fn 1 with param0 = start offset returns up to 16 name characters from that offset; loop until length reached.
  fn 2 = device type (0 keyboard, 1 remote, 2 numpad, 3 mouse, 4 touchpad, 5 trackball, 6 presenter, 7 receiver, 8 headset ...). V: Solaar hidpp20.py L1841-L1870.
* Unit identity for de-duplicating cable + receiver copies of one mouse: feature `0x0003` fn 0 returns entity count + 4-byte unit ID (HaloBattery uses it; I for exact byte offsets, see logitech.py L33-L35).
* Unifying receivers (HID++ 1.0 name, V kernel L856-L890): `10 FF 83 B5 <0x40 + (slot-1)> 00 00` (GET_LONG_REGISTER 0x83, register `0xB5`); reply param[1] = name length, param[2..] = ASCII name.
  Pairing info `0xB5` sub `0x20+(slot-1)`: param[2] = the radio report interval in ms, param[3..4] = WPID, param[7]&0x0F = device kind. V: Solaar receiver.py L124-L126, L268-L283.
  Bolt: register `0xB5`, sub `0x50+slot` (pairing: wpid, serial) and `0x60+slot` with param `0x01` (name; name length at param[2]). V: Solaar receiver.py L492-L505; offsets I.
* USB product string of a receiver is always "USB Receiver"/"Lightspeed Receiver" etc. (V: Solaar names, base_usb.py) so use the HID++ name above.

## 6. Polling rate

**There is no HID++ 1.0 way.** Two HID++ 2.0 features exist (V: Solaar hidpp20_constants.py L145-L146, settings_templates.py L598-L660, hidpp20.py L2125-L2137):

### 6.1 `0x8060` REPORT_RATE (G502, G Pro Wireless, most 1 kHz mice)

* fn 0 (`0x00`) = get list: param0 = bitmask, bit *i* set means (i+1) ms supported (bit0 = 1 ms ... bit7 = 8 ms).
* fn 1 (`0x10`) = get current: param0 = report interval in **ms** (1..8). Hz = 1000 / ms (1 -> 1000, 2 -> 500, 4 -> 250, 8 -> 125). Other values: 3 -> 333, 5 -> 200 ...
* fn 2 (`0x20`) = set (do not use).

### 6.2 `0x8061` EXTENDED_ADJUSTABLE_REPORT_RATE (Superlight 2, G502 X Plus/Lightspeed, G Pro 2 - 2k/4k/8k)

* fn 1 (`0x10`) = get list: params 0..1 = 16-bit bitmask over the code table below.
* fn 2 (`0x20`) = get current: param0 = code. (set = fn 3, `0x30`.)

| Code | Interval | Hz |
|---|---|---|
| 0 | 8 ms | 125 |
| 1 | 4 ms | 250 |
| 2 | 2 ms | 500 |
| 3 | 1 ms | 1000 |
| 4 | 500 us | 2000 |
| 5 | 250 us | 4000 |
| 6 | 125 us | 8000 |

(V Solaar settings_templates.py L631-L650 and hidpp20.py L2132-L2137; the code->label table is the same in both.)
Solaar's note: writes "may need onboard profiles disabled"; reads give the currently active rate. U: whether the value read while onboard profile mode is active equals
the effective radio rate on every model.

Order: look up `0x8061` first (newer firmware may also expose 0x8060), else `0x8060`. If neither exists the mouse's rate is fixed or not exposed -> show "n/a" (or measure, see README).

The receiver's pairing register (4.5/5) also reports a per-device *radio report interval* in ms (`0xB5` sub `0x20+n-1`, param[2]); this is a link property, not guaranteed to equal the current USB report rate. I.

## 7. Quirks

* **Asleep device**: through a receiver, a sleeping mouse gives either no answer (timeout) or HID++1.0 error 9 (resource error) / 8 (unknown device -> slot empty). Treat timeout as "no reading", keep the last value greyed
  (HaloBattery keeps it 300 s). First request after idle may need ~0.5-2 s (V HaloBattery logitech.py L57-L59).
* Use a rotating software ID (0x0A..0x0F) so a late reply to an earlier request is not mistaken for the current one (HaloBattery SWIDS, HW).
* Wired + receiver copies of the same mouse: dedupe via unit ID (feature 0x0003) or name; prefer the reading that says "charging".
* Receiver slots: probe slots 1..6 on receivers (Nano C534 has 2); device index `0xFF` on direct devices; error 8 on a slot = empty.
* Conflict with G HUB / Logi Options+: they also poll; replies can interleave. Match on (idx, feature, fn|sw), and do not set report-mode registers (no writes at all).
* Kernel quirk: `0x1000` capability fn can return RESOURCE_ERROR intermittently - retry (V L1299-L1301).
* Linux-kernel note: when a device only supports long reports (no `0xFF00:1` collection) never send short.
* HID++ error `0xFF` reply with `feature=0`/`err=1` to ping = HID++ 1.0 (use registers 4.5).

## 8. Sources

* Linux kernel: https://github.com/torvalds/linux/blob/master/drivers/hid/hid-logitech-hidpp.c (report IDs L44-L50, errors L221-L246, FAP send L395-L416, root L940-L1006, 0x1000 L1186-L1310, 0x1001 L1391-L1495, 0x1004 L1566-L1740, 0x1F20 L1856-L1895, HID++1.0 registers L610-L850, name L856-L890, USB match table L4877-L4912)
* https://github.com/torvalds/linux/blob/master/drivers/hid/hid-logitech-dj.c (HID++ descriptor L556-L600, receiver PIDs L2063-L2103)
* Solaar (GPL-2): https://github.com/pwr-Solaar/Solaar/tree/master/lib/logitech_receiver (hidpp20.py, hidpp20_constants.py, hidpp10_constants.py, base.py, base_usb.py, receiver.py, settings_templates.py, descriptors.py)
* HaloBattery (MIT, Windows hardware-tested): https://github.com/HeyOkay/HaloBattery/blob/main/providers/logitech.py and docs/protocols.md (G502 LIGHTSPEED / G502 X PLUS confirmed)
