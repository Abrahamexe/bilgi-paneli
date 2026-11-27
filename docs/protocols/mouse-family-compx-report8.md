# Family A: CompX "report 8" 0x55-checksum protocol (ATK / VXE / Dragonfly / Pulsar-Nordic / Lamzu Atlantis / Scyrox / VGN / Kysona / G-Wolves XVI:0 / Dareu Jm / Endgame WE)

Largest family. One firmware stack from the ODM CompX (Beken/Nordic MCU). The same 16-byte frame, the same 0x55 sum rule and the same command numbers appear under many brand names and vendor IDs. One driver covers all of them; only VID/PID/usage selection, the polling-code table and the transport variant (output vs feature report) differ.

Confidence legend: **V** = verified in source (URL), **HW** = source says confirmed on real hardware, **I** = inferred, **U** = unknown.

## 1. Brands / models

| Brand | Models (examples) | VID | Evidence |
|---|---|---|---|
| ATK / VXE / Dragonfly | F1 Pro/Max/S, X1, A9, U2, Y9, MAD R, VXE R1/R1 SE/SE+/R1 Pro/Max, V3, X3 (206-entry ATK HUB catalog) | 0x3554, 0x373b | V, HW (MAD R, R1 SE+, R1 Pro Max) |
| Pulsar (Nordic generation) | Xlite/Xlite V3, X2, X2A, X2H, X2V2 (+Mini/Large/Custom), 4K receiver | 0x3554 | V, HW |
| Pulsar CrazyLight / Feinmann Noctua | X2 CrazyLight, X3 LHD CrazyLight, F01 Noctua | 0x3710 | V, HW |
| Pulsar X2 Wireless gen 1 (Areson ids) | X2 / X2 Mini Wireless | 0x25a7 | V, HW |
| Lamzu Atlantis generation | Atlantis (OG V2, Mini, Mini Pro, 4K), Thorn, Maya, Paro (old) | 0x3554 | V, HW (Atlantis Mini 4K) |
| Scyrox | V8, V8S | 0x3554 | V, HW |
| Kysona | M600 | 0x3554 | V (Linux kernel) |
| VGN | Dragonfly F2 Master+ | 0x3554 | V |
| Teevolution (Terra Pro), Gravastar, Noir M1-NEX | - | 0x3554 | V (ids only) |
| G-Wolves "XVI:0" | HTX Ultra (+HTS Plus Pro magnetic) | 0x33e4 | V, HW |
| Dareu Jm family | A950 Pro Mg (TM271F), TM265 dongle | 0x260d | V |
| Endgame Gear WE | OP1we, XM2we | 0x3367 | V (feature-report variant, see 3b) |
| Waizowl | Duckbill/OGM line appears in the ATK HUB catalog as cid "WAIZOWL,1" (373b:1282) | 0x373b | I |
| Hitscan Utility (native) | - | 0x3770 | V (command table only, unverified) |

## 2. IDs

| VID | PIDs | Meaning |
|---|---|---|
| 0x3554 | f507 wired, f508 1K dongle, f509 4K dongle | Pulsar Nordic (kernel patch hid-pulsar) |
| 0x3554 | f57c dongle, f57d wired | Kysona M600 (mainline hid-ids.h v6.13) |
| 0x3554 | f502, f506, f50a | ATK Dragonfly F1 (PID shared; split by CID/MID) |
| 0x3554 | f58a (R1 Pro Max RX), f58c (R1 Pro/Max wired), f58e (R1 SE+ RX), f58f (R1/SE/SE+ wired) | VXE R1 |
| 0x3554 | f50d (1K RX), f50f (wired), f510 (4K RX) | Lamzu Atlantis family (model not distinguishable on wire) |
| 0x3554 | f515 V8S wired, f517 V8S Turbo 8K dongle, f5f7 V8 8K dongle | Scyrox |
| 0x3554 | fb56 RX, fb57 wired | VGN F2 Master+ |
| 0x3554 | f520 f522 f523 f5bb; f54b f575 f548 f577 (RX) f576 f549 (wired); f500 | Teevolution / Gravastar / Noir |
| 0x373b | 1014...1337 (see ATK catalog); 1085 = "Wireless mouse -1k dongle" (R1 SE/SE+, Beken) | ATK/VXE dongles |
| 0x3710 | 5406 dongle, 3414 wired, 3527 X3 LHD CL wired, 5504 Noctua dongle, 7507 wired | Pulsar |
| 0x25a7 | fa7b wired, fa7c receiver | Pulsar X2 gen1 (Areson ids) |
| 0x33e4 | 5618 wired, 3854 wireless RX (HTX Ultra) | G-Wolves |
| 0x260d | 1117 wired, 1114 receiver | Dareu |
| 0x3367 | 1962, 1972 cable; 1961, 1970 RX (OP1we); 1960, 1968, 1982 RX (XM2we) | Endgame Gear WE |

Selecting the right HID top-level collection (the WebHID filter equivalent):

* ATK HUB (v3.2.27, extracted into the OpenMouse catalog): 0x3554 -> usagePage 0xFF04 usage 2; 0x373b -> 0xFF04:2 (most), 0xFF05:1 (NearLink "NK" models), 0xFF10:1 (older), 0xFFA0:2 (A9 Pro Max), 0xFF02:2 (Duckbill Elite). V
* OpenMouse captured the 0x55-framed channel on **0xFF02 usage 0x0002** (interface 1) and believes it coexists with the 0xFF04 one. HW
* Lamzu Atlantis: interface 1, usagePage 0xFF02, usage 2, collection must declare output report 8. HW
* Pulsar Nordic / CrazyLight: interface 1 (266-byte descriptor), report ID 8, IN endpoint 0x82, no OUT endpoint. HW
* VGN: usagePage 0xFF02. Dareu: 0xFF02:0x02 (+ receiver service 0xFF05). Endgame WE: command 0xFF02, notify 0xFF01, identity 0xFF04. V

Practical rule for Win32: enumerate collections (HidP_GetCaps) and take the one that has report ID 0x08 with 16-byte payload (17 with ID) on a vendor page (0xFF02 / 0xFF04). Several vendor collections exist on the same device; only one answers.

## 3. Frame format (V: libatk-rs, madr, lamzu-cfg, Pulsar docs, kernel patch, OpenMouse; all agree)

17 bytes including report ID.

```
[0]  0x08 report ID
[1]  command
[2]  status/error (0 in request; 0 = OK in reply)
[3]  address high   (cmd 0x07/0x08 only)
[4]  address low
[5]  data length (<=10)
[6..15] payload (10 bytes)
[16] checksum = (0x55 - sum(bytes[0..15])) & 0xFF        (report ID IS included in the sum)
```

Replies use the same layout (cmd echoed at [1], payload from [6]), checksum identical. WebHID-style tools strip the report ID, so the same fields appear at indices one lower (battery percent at [5]). Unsolicited events use cmd 0x0A (byte 6: 0x01 DPI button, 0x04 profile change, 0x40 unknown); skip anything whose cmd byte is not the one you sent. Flash values are stored as (value, 0x55-value) pairs.

### 3a. Transport variant A1 (ATK, Pulsar Nordic/CrazyLight, Kysona, Lamzu Atlantis, Scyrox, VGN, G-Wolves, Dareu)
* Host -> device: **output report 0x08**. The device has no interrupt OUT endpoint, so it is a control SET_REPORT(Output). On Windows use `HidD_SetOutputReport` (or `WriteFile`, which hidusb turns into SET_REPORT). The Linux kernel driver states "device listens only to control transfers". V
* Device -> host: input report 0x08 (interrupt IN), 17 bytes. Read with `ReadFile` on a second overlapped handle; keep reading continuously because unsolicited reports interleave.

### 3b. Variant A2 (Endgame Gear WE, Pulsar X2 gen1 on 0x25a7)
* Host -> device: **feature report 0x08** (16-byte payload), `HidD_SetFeature`.
* Device -> host: **input report 0x09** (EGG notify collection 0xFF01 / Areson). V, HW

## 4. Read-only commands (everything the tray app needs)

| Cmd | Name | Request bytes (17, with report ID) | Reply |
|---|---|---|---|
| 0x03 | link/online | `08 03 00*14 4A` | [6]==1 mouse available behind dongle (Kysona/Pulsar kernel: >1 is an error). ATK GetWirelessMouseOnline: [6]=1 online, [7..9] RF id |
| **0x04** | **battery** | `08 04 00 00 00 00 00 00 00 00 00 00 00 00 00 00 49` (byte 16 = 0x49) | [6]=percent 0..100, [7]=charging, [8..9]=millivolts big-endian |
| 0x10 | CID/MID (ATK model id) | `08 10 00*14 3D` | [6]=cid, [7]=mid (e.g. "2,27" = VXE R1 Pro Max) |
| 0x12 | firmware | `08 12 00*14 3B` | [6]=major, [7]=minor (BCD, v1.24 shown as 01 24) |
| 0x08 | read flash | `08 08 00 AH AL LEN ... CS` (LEN<=10) | [6..] data; reply echoes addr/len |
| 0x0E | active profile | cmd only | [6] profile index 0-based |
| 0x01 | identity challenge (Pulsar/Lamzu/Scyrox) | [5]=8, [6..9]=4 random bytes, [10..13]=0 | reply [6..9] transformed nonce, [10]=cid, [11]=mid, [12]=type, [13]=dongle type. Model id = (cid<<8)|mid. Checksum rule as above |

Battery decode (V, HW on Pulsar, VXE MAD R, Lamzu Atlantis Mini 4K, Scyrox V8S):

* percent = byte[6]; discard if >100.
* charging = byte[7]==1 (kernel patch rejects >1; OpenMouse treats nonzero as charging).
* voltage mV = (byte[8]<<8)|byte[9]. The Lamzu note says the reported byte is more accurate than a voltage-derived estimate and Atlantis has no fuel gauge IC (TPU), so the percent is coarse.
* Conflict: libatk-rs documents the older R1 Pro layout as data[2]/10 volts (single byte). Ignore voltage, rely on percent.
* Other brands in the family: Dareu reply [5]=percent, [6]=status (body indices without report ID). G-Wolves/VGN same as above. Endgame WE: reply (feature report 8 then input report 9, body indices) [0]=cmd 4, [1]=0 ok, [5]=percent, [6]=charging.

Real model name:
* ATK: cmd 0x10 -> (cid,mid) and VID:PID -> catalog (ATK HUB table: e.g. 0x3554:f58c + "2,27" = VXE R1 PRO MAX). 4 VID:PID pairs are shared (f506, f50a, f58c, f58f) so CID/MID is mandatory. Some 1K dongles (373b:1085) do not answer a model query.
* Pulsar: cmd 0x01 -> model id table (0x0607 Xlite V3, 0x0608 X2A, 0x060a X2V2, 0x060c X2H ...). Pulsar CrazyLight: cid 0x57 (mid 0x03 = X2 CrazyLight, 0x68 = X3 LHD CrazyLight).
* Lamzu Atlantis / Scyrox: Atlantis models are indistinguishable on the wire (same PID/firmware). Scyrox V8S answers cid 0x3E mid 0x05.
* Kysona/Pulsar kernel patch also maps ATK R1 SE+ model 0x0220.

## 5. Polling rate

Read flash address 0x0000, length >=2: byte0 = rate code, byte1 = 0x55-code. This is the **configured** rate of the active profile, not a measurement. (Reply bytes [6],[7].)

Code table (Pulsar / VGN / G-Wolves / ATK / Scyrox "CompX table"), V + HW:

| Code | 0x08 | 0x04 | 0x02 | 0x01 | 0x10 | 0x20 | 0x40 |
|---|---|---|---|---|---|---|---|
| Hz | 125 | 250 | 500 | 1000 | 2000 | 4000 | 8000 |

Exceptions:
* **Lamzu Atlantis 0xf50f / receiver families** read in the field: 0x10 = 1000 Hz and 0x20/0x40/0x80 = 2000/4000/8000 (Lamzu table). The f510 4K receiver is declared to use the CompX table above. Treat 0x10-0x80 as ambiguous unless PID is known. HW (partial)
* VXE R1 SE/SE+ 1K dongle (373b:1085): live-settings row at flash 0x0070, entries `[selector, value, 0, 0x55-value]`; selector 0x0b polling, 0x03=250, 0x02=500, 0x01=1000. Read path is not documented (U); the row is writable live. V (OpenMouse, OpenVXE)
* Pulsar maximum rate per type from the 0x01 reply byte [11]: 0=1000, 1=4000, 2=1000 (wired), 3=8000 (wired), 4=2000, 5=8000.
* Do not confuse with the Sonix Fusion2 table (family "pulsar-sonix"), where the same hex values mean the reverse (0x01 = 125 Hz).

## 6. Quirks

* Never send: 0x09 (factory reset), 0x07 (flash write), 0x05 (dongle pair), 0x0D (enter USB upgrade), 0x0B (set VID/PID), 0x0F (set profile), 0xF0 (write CID/MID), 0x02 driver status. Safe set: 0x01, 0x03, 0x04, 0x08, 0x0E, 0x10, 0x12, 0x15, 0x17, 0x1D, 0x2B.
* Reply wait: ATK 500 ms timeout; Pulsar kernel 100 ms; match on cmd byte and checksum.
* Polling too often disturbs the mouse's radio sleep: Pulsar tool polls every 60 s, kernel Kysona every 5 s. A battery poll does not keep the mouse awake or defer sleep (observed, Pulsar Fusion2). Use >= 30-60 s.
* Asleep mouse: dongle may not answer; cmd 0x03 first, then 0x04. Mice also push battery by themselves when waking.
* Vendor software (Pulsar Fusion, ATK HUB) can hold the interface; input reports are per-handle so a second reader is generally fine (I).
* Over a cable the same PID family answers for itself; wired A9 still reports battery.
* Write gaps: two config writes ~0.4 s apart through a 2.4G receiver froze a link (not relevant to read-only).

## 7. Sources

* madr (VXE MAD R): https://github.com/dkbednarczyk/madr (madr-lib/src/battery.rs, performance.rs, device.rs) and https://bednarczyk.xyz/blog/reverse-engineering-a-gaming-mouse/
* libatk-rs (ATK command ids, checksum): https://github.com/cyberphantom52/libatk-rs
* lamzu-cfg: https://github.com/LeadSun/lamzu-cfg (src/atlantis/hid.rs)
* pulsar-mouse-linux: https://github.com/packerlschupfer/pulsar-mouse-linux (src/pulsar_mouse/drivers/nordic.py, docs/protocol-x2-crazylight.md)
* Kernel hid-pulsar patch v1: https://patchew.org/linux/20260318205503.64420-1-nikolas@koesling.info ; mainline hid-kysona v6.13: https://github.com/torvalds/linux/blob/v6.13/drivers/hid/hid-kysona.c (request `08 04 ... 49`)
* OpenMouse protocol package (AGPL, facts only): https://unpkg.com/@openmouse/protocol@0.60.0/dist/ (atk/index.js, drivers/atk/hid.js, drivers/atk/device-catalog.generated.js, lamzu/atlantis.js, vgn/index.js, gwolves/index.js, scyrox/index.js, pulsar/index.js, dareu/index.js, endgame-gear/wireless.js)
* OpenVXE (R1 SE+ dongle, unverified battery via cmd 0x17, treated as doubtful): https://github.com/BuSd777/OpenVXE
* ATK HUB: https://v3-hub.atk.store/ (reachable; per-device chunks lazy-loaded, not independently dumped)
