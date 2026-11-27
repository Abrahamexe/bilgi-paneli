# Family C: Pulsar "Sonix / Fusion2" 64-byte feature protocol (VID 0x3710) + Pulsar Pro (0x5405)

Used by Pulsar's wired/Fusion2 line and its 1K/8K wireless dongles. Pulsar's Nordic-generation mice (0x3554, CrazyLight on 0x3710:5406) use Family A instead (see mouse-family-compx-report8.md). Ids on the same VID 0x3710 therefore split by PID.

Confidence: V verified in source, HW hardware-confirmed per source, I inferred, U unknown.

## 1. Devices (VID 0x3710)

| PID | Device | Protocol |
|---|---|---|
| 1401 / 1402 / 1403 / 1404 | Xlite wired, X2 wired, X2H wired, X2A wired | Sonix 64 B (this file) |
| 3401 / 3404 / 3409 / 3410 | Xlite v4, X3, X3 LHD wired | this file |
| 5402 | 1K wireless dongle (Fusion2 line, X3, Xlite v4 ...) | this file (+ power category) |
| 5403 | 8K wireless dongle | this file, rates to 8000 |
| 5404 | Feinmann 8K / FO1 dongle | this file (same framing) |
| 5405 | Pulsar Pro receiver | **different** (section 6) |
| 5406, 3414, 3527, 5504, 7507 | CrazyLight / Noctua | Family A (Nordic 17-byte) |

Interface 3, feature report ID 0, 64 bytes. (OpenMouse XS-1 driver matches the same PIDs by shape: unnumbered 64-byte feature report.) V, HW (X3, X3 LHD, Xlite v4, Feinmann)

## 2. Frame

```
[0]  direction: 0x00 host->device, reply 0x01 done, 0x02 pending, 0x05 dongle has no link to the mouse, 0x00 request not taken
[1]  category
[2]  register (bit7 set = read)
[3]  sub-register
[4-5] 0
[6]  profile (0 = currently active / global; reply echoes it for per-profile requests)
[7..61] payload
[62..63] checksum = uint16 little-endian of sum(bytes[0..61])
```
Send via SET_FEATURE (control, wValue 0x0300, report ID 0), then poll GET_FEATURE of the same report every 10-30 ms until byte[0]==0x01 **and** category/register echo the request. Do not resend while 0x02 (a second SET while pending is dropped and the late reply is handed to the next read, which once produced a battery of "1%"). If byte[0]==0x05 with the request echoed, the mouse is asleep/absent: stop. Reads typically answer in ~11 ms (cable) after 2-3 polls; give up after ~1 s. V, HW

## 3. Battery and charging (read-only)

| What | cat | reg (read) | sub | profile | Reply |
|---|---|---|---|---|---|
| percent | 0x08 | 0x81 | 0x01 | 0 | byte[6] = percent 0..100; 0xFF = no mouse behind dongle |
| charging | 0x08 | 0x83 | 0x01 | 0 | byte[6] = 1 charging (on cable), 0 not |

Request example (percent): bytes `00 08 81 01 00 00 00 ...` and checksum at [62..63] = 0x8A 0x00 (0x08+0x81+0x01 = 0x8A). Charging: sum 0x8C 0x00 -> `8C 00`.
No millivolt reading found. Confirmed on X3 via 1K dongle and cable, and X3 LHD via 8K dongle; Feinmann 8K same registers (Feinmann charging never observed non-zero). V, HW

Passive pushes on the dongle's event interface (separate HID interface): `05 07 <charging> <percent> 01` (battery, plain hex), `05 08 <01 up | 02 gone> 01` link, `05 0D <value>` link quality once per second. V, HW

## 4. Polling rate

cat 0x01, reg 0x89 (0x09|0x80), sub 0x02, profile 0 -> reply byte[7].

| Code | 0x01 | 0x02 | 0x04 | 0x08 | 0x10 | 0x20 | 0x40 |
|---|---|---|---|---|---|---|---|
| Hz (Fusion2, 1K dongle, Feinmann, 8K dongle) | 125 | 250 | 500 | 1000 | 2000 | 4000 | 8000 |

Reverse to Family A. 1K devices only go to 1000; 8K dongle measured 999/1999/3988/7200 reports/s for 0x08/0x10/0x20/0x40. V, HW
**Warning:** OpenMouse's pulsar-xs1-hid.js reads "polling" from cat 0x08 reg 0x85 sub 0x03 (the register that pulsar-mouse-linux shows is the **sleep timer**, uint16 LE seconds at [7:9]) and marks it unverified. Use cat 0x01 / 0x89.

## 5. Model identification

* Paired mouse behind a dongle: cat 0x01, reg 0x84, sub 0x04, profile 0 -> reply [6..7] = VID LE, [8..9] = PID LE of the **wired** PID of the mouse (e.g. 3710:3409 = X3, 3410 = X3 LHD, 3401 = Xlite v4). The 8K dongle answers this under the previous request's header (match on payload VID 0x3710, not on echo).
* Firmware: cat 0x01 reg 0x87 sub 0x04 -> [6:10] little-endian version.
* Sleep timer: cat 0x08 reg 0x85 sub 0x03 -> uint16 LE seconds at [7:9] (30/60/120/180/300 seen).

## 6. Pulsar Pro receiver 0x3710:0x5405 (OpenMouse, U on hardware)

Collection usagePage 0x01 usage 0 with input and output report ID 0, 63-byte data. Request `[cmd, param0=0 ...]` via output report; reply input report whose byte[0] echoes cmd (0xA6 = rejected, 0xA7 = busy, retry after 50 ms).
* Battery cmd 0xB4: reply `[0]=0xB4, [1]=percent, [6]=charging(1)`.
* Polling cmd 0xB2: reply uint16 LE at [2] = Hz directly.
* Dongle RSSI 0xA4, versions 0xA0/0xBF, profile 0xC6.
Source flags: vendor-supplied model-name only via product string. Confidence: V (source) / U (hardware).

## 7. Quirks

* Poll no faster than once a minute (Pulsar tool 60 s); a battery poll neither wakes nor delays mouse sleep. Link drops when the sleep timer fires (e.g. 30 s idle); then reads return 0x05.
* Requests can be lost even when back-to-back (216 of 5155 on an X3); resend the identical frame once after the loss, but only when the buffer shows an answer for another request. Pending (0x02) must never be resent.
* After a cable<->dongle swap the first reads may be dropped for ~0.2-1.2 s; allow up to 6 resends spread over ~1.5 s at open.
* Fusion (Pulsar's app) and this driver both use interface 3 via control transfers, so no exclusive conflict is described; on Linux the tool detaches the kernel driver (not needed on Windows with HidD_*).
* Pulsar web configurator: https://bbb.pulsar.gg (named in OpenMouse source comments, not dumped).

## 8. Sources

* https://github.com/packerlschupfer/pulsar-mouse-linux (drivers/x2a.py, xlite_v4_wireless.py, fusion2_8k_dongle.py, feinmann8k.py, nordic.py)
* OpenMouse: https://unpkg.com/@openmouse/protocol@0.60.0/dist/pulsar/index.js and drivers/pulsar/pulsar-pro-hid.js, pulsar-xs1-hid.js
