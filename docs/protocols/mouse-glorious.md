# Glorious mice (Model O / O- / D / D- Wireless, Model O Pro, D 2 Pro, Model O 2 / I 2 Wireless)

Scope: battery %, charging, polling rate. Research date 2026-11-24. Tags: **V** verified in source, **I** inferred, **U** unknown, **HW** third-party hardware report.
Licenses: korkje/mxw (MIT), danexpo/o2ctl (MIT), tvollstaedt/glorious-i2-battery (GPL-2; facts only), Rana-Faraz/glorious-mouse-battery-system-tray (license not stated; facts only),
HaloBattery (MIT). Nothing copied.

Three unrelated protocol families under one brand:

| Family | Mice | VID : PIDs | Battery readable? |
|---|---|---|---|
| **A. SinoWealth "Core 1" wireless** | Model O Wireless, O- Wireless, D Wireless, D- Wireless, Model O PRO Wireless, Model D 2 PRO Wireless | VID `0x258A`. O: wired `2011` / dongle `2022`. D: `2012` / `2023`. O-: `2013` / `2024`. D-: `2014` / `2025`. O PRO: `2027` (dongle). D 2 PRO: `2034` (dongle) | **Yes** (query) |
| **B. "Core 2" Model O 2 Wireless** | Model O 2 Wireless | VID `0x093A`: dongle `822D`, cable `822A`, Bluetooth `822B` | **Yes** (query + 65 s events) |
| **C. Model I 2 Wireless** | Model I 2 Wireless | VID `0x093A`: dongle `821D`, cable `821A` | **No query**; passive events only |

Wired-only mice (Model O/O-/D/D- wired `258A:0036` etc., Model O Eternal `3794:A000`): no battery; configuration via libratbag sinowealth driver (polling readable from the settings report on cable, U for offsets). V HaloBattery-adjacent perihub list; libratbag driver-sinowealth.c.
V (A, PIDs): mxw src/util/devices.rs L74-L95; Rana-Faraz battery.rs L9-L36 (2027, 2034). V (B/C): o2ctl docs/PROTOCOL.md L18-L24; glorious-i2-battery README.

## 1. Family A (SinoWealth wireless) - battery

### Collection
* Open **USB interface 2** (hidapi `interface_number == 2`) of VID `258A`, PID wired or wireless. V mxw devices.rs L99-L110, Rana battery.rs L7 (`FEATURE_INTERFACE = 0x02`).
  HaloBattery: the receiver publishes vendor collections `0xFFA0` and `0xFFFF`; use the one whose **feature report is 65 bytes** (ID + 64); try `0xFFA0` first on ties. HW.
* Transport = **feature report, report ID 0, 65 bytes** (`HidD_SetFeature` then `HidD_GetFeature`).

### Request (65-byte feature report; byte 0 = report ID 0)
```
b[0]=00  b[1]=00  b[2]=00  b[3]=02  b[4]=02  b[5]=00  b[6]=83  b[7..64]=00
```
(`b[3]=0x02` DevSel = wireless side... `b[4]=0x02` CmdType = battery, `b[6]=0x83` function). V mxw src/report/battery.rs L5-L9: bytes at 65-byte buffer indexes 3, 4, 6; Rana battery.rs L10 `(3,0x02,4,0x02,6,0x83)`. HW (mxw, GloriousBatteryMonitor-Go via HaloBattery, confirmed by user #281 on a Model O Wireless).
Firmware-version query for reference: `b[4]=0x03, b[6]=0x81`.

### Reply
Wait **~50-100 ms** after the SetFeature (mxw 50 ms, HaloBattery ~100 ms RF round trip), then `GetFeature` the same 65-byte report ID 0:

| Byte (index in the 65-byte buffer) | Meaning |
|---|---|
| 1 | status: `A1` awake/ok, `A4` asleep, `A2` waking/unknown, `A0`/`A3` other (mxw maps A1->0 ok, A4->asleep, A0->"waking up"; HaloBattery calls A2 "waking") |
| 6 | echo `0x83` - if not `0x83` the reply is stale/not ours (mxw treats it as status 2) |
| 7 | charging: 0 = no, non-zero = on charger (HaloBattery) |
| **8** | **battery percent**; 0 is shown as 1 by mxw |

V mxw battery.rs L11-L32, Rana battery.rs. HW HaloBattery #281 ("percentage matched Glorious Core").
* **Charging**: HaloBattery found the reply's charging byte unreliable while the mouse is on the cable: the mouse leaves the 2.4 GHz link, the dongle goes quiet. Their rule: charging = *wired PID (`2011` etc.) appears*, keep the last wireless level. mxw prints percent for wired mode too (query the **wired PID's** interface 2 the same way). I/HW.
* Asleep mouse: status `A4` (or no answer): show last level greyed. V mxw ("(asleep)"), HW.

## 2. Family B (Model O 2 Wireless, `093A:822D/822A/822B`)

From o2ctl docs/PROTOCOL.md (derived from reading Glorious CORE 2.1.21; "verified" items were checked on a real mouse). V/HW.
* **USB interface 1**, vendor usage page `0xFF00`, usage 1, **feature report ID 3** (63 bytes + ID = 64 total). Input report 6 on the same interface (events).
* Battery query: SetFeature `03 08 <route> 14` where route = **`FB` for the dongle, `FA` for the cable**; then GetFeature report 3: reply (ID removed) `08 14 <level%> <charging>`; charging 1 = charging. Verified over the cable: `08 14 64 01` = 100 %, charging. Through the dongle while the mouse is on the cable the dongle replies from stale data (`08 14 40 00`).
* Events: input report 6: `06 FB <level> <charging>` about every 65 s over the dongle; none over the cable; none after the first "charging" report until unplugged.
* Bluetooth `822B`: feature report **5** (20 bytes + ID), packets without the route byte: `05 08 14` -> `08 14 <level> <charging>`.
* Wait ~30 ms after every packet; the dongle relays asynchronously (a read straight after write may return old data).
* Polling: **read U** ("reads of settings time out on the dongle"). Setter: performance block byte 4 = 1 125 Hz, 2 250, 3 500, 4 1000.

## 3. Family C (Model I 2 Wireless, `093A:821D` / `821A`)

* **The mouse cannot be asked.** Every GET_REPORT times out (5 s). Battery arrives only as **input report 6**: `06 FB <percent 0..100> <toggle> 00 00 00 00` on interface 1, vendor collection (usage page `0xFF00`, usage `0xFF00`), roughly every 20-40 s while in use, only over the 2.4 GHz receiver. The `toggle` byte flips each frame; it is **not** a charging flag. V tvollstaedt/glorious-i2-battery README ("How the mouse reports its battery").
* Implementation: open the vendor collection read-only and wait for `06 FB`. Cable present (PID `821A` visible) = charging (inferred from the cable PID; no frames while plugged).
* Polling: U.

## 4. Polling rate (all Glorious)

* Family A: **U** (no read; mxw sets it with `b[3]=2,b[4]=1,b[5]=1,b[7]=ms` where ms is the interval 1..8; V mxw polling_rate.rs). Wired-mode settings report is readable via libratbag's SinoWealth driver (config report, 520-byte on Windows - U).
* Family B/C: not readable over wireless. Show n/a, or measure (see README).
* Glorious wireless mice are limited to 1000 Hz except Model O Pro/D 2 Pro 4K-8K variants (U).

## 5. Model name

PID table above. The dongle's USB product string is the generic "Glorious Model O Wireless" family name (I). `258A` is also used by many other SinoWealth OEM devices (Rana/mxw filter strictly on interface 2 + PID).

## 6. Quirks
* Send nothing but the battery report (`83`/`08 14`): other function codes (DPI, LED, `04` performance, profile) are writes.
* Close Glorious CORE first? mxw/Halo say nothing about conflicts; CORE uses the same reports (U).
* SetFeature to a *sleeping* mouse returns OK (it is the dongle) but the answer carries `A4`/old data.
* Family A wired PID answers the same query but status bytes differ; use only the percent byte there.

## 7. Sources
* https://github.com/korkje/mxw (src/report/battery.rs, src/util/devices.rs, src/config/polling_rate.rs)
* https://github.com/Rana-Faraz/glorious-mouse-battery-system-tray/blob/master/src/battery.rs
* https://github.com/danexpo/o2ctl/blob/main/docs/PROTOCOL.md
* https://github.com/tvollstaedt/glorious-i2-battery (README)
* https://github.com/HeyOkay/HaloBattery/blob/main/docs/protocols.md ("Glorious Model O Wireless")
