# Major-brand mice: battery / charging / polling-rate summary (read-only HID)

Research date 2026-11-24. Per-brand detail files are linked. Confidence: **V** verified in open-source code (or clean-room capture docs), **HW** additionally reported working on real hardware by a third party (not by us),
**I** inferred, **U** unknown / not publicly documented. "n/a" = nothing to read.

| Brand | Protocol family (file) | Battery % | Charging | Polling rate (Hz) | Notes |
|---|---|---|---|---|---|
| Logitech (Lightspeed / Unifying / Bolt / Nano / wired) | HID++ 1.0 + 2.0, `0xFF00` usages 1/2, report `0x10/0x11` ([mouse-logitech-hidpp.md](mouse-logitech-hidpp.md)) | **V/HW** features `0x1004`, `0x1000`, `0x1001` (+ HID++1.0 regs `0x0D`/`0x07`) | **V/HW** status byte of the same feature | **V** feature `0x8060` (ms) / `0x8061` (125 us..8 ms, up to 8 kHz); not on all mice | Model name via feature `0x0005`. Same code serves Dell/Lenovo Nano-receiver mice |
| Razer (wired, HyperSpeed, HyperPolling 4K/8K) | 90-byte feature report, ID 0, class `0x07` ([mouse-razer.md](mouse-razer.md)) | **V/HW** `07/80` byte arg[1]/255 | **V/HW** `07/84` arg[1] | **V** `00/85` (125/500/1000) or `00/C0` (125..8000); get of 0xC0 only source-verified | Per-PID transaction ID + wait time table; interface 0 (3 on newest) |
| SteelSeries (Aerox/Prime/Rival 3 Wireless, Rival 650) | rivalcfg-style 64-byte output + input ([mouse-steelseries.md](mouse-steelseries.md)) | **V/HW** `92`/`D2` ((b&0x7F-1)*5) ; `AA 01` for Rival 3/650 (V only) | `92` bit7 (HW level only); `AA 01` byte (V) | **U** (set-only) | Dongle uses opcode OR 0x40 |
| Corsair (wireless) | Bragi "slipstream" property GET, route byte 0x08/0x09 ([mouse-corsair.md](mouse-corsair.md)) ; legacy NXP for old Dark Core | **V/HW** property `0x0F` /10 | **V** property `0x10` (not HW-confirmed on dongles) | **V** property `0x01` (125-1000), 2k+ **U** | NXP: `0E 50` 5 coarse steps; usage page `0xFF42` (Bragi) |
| HyperX | Dart: heartbeat `51`; Haste 2 family: NGENUITY 3 `50 02` ([mouse-hyperx.md](mouse-hyperx.md)) | **V/HW** Dart; **V** + HW(third party) Haste 2 Wireless | **V** (Dart byte 5; Haste byte 3) | Haste: **V/I** via `32 02` reply (125-8000); Dart **U** | Haste Wireless (gen 1), Haste 3 wireless PIDs **U** |
| ASUS ROG / TUF | `12 07` / `12 04`, report ID 0 (3 on OMNI), 64 bytes ([mouse-asus-rog.md](mouse-asus-rog.md)) | **V** (HW: Harpe II Ace) byte 5 (% or 0..4 steps) | **V** byte 10 | **V** `12 04` byte 13 (125-8000) | Unsolicited reports share the pipe |
| Glorious | A: SinoWealth feature report `..83` ; B: Model O 2 `03 08 route 14`; C: Model I 2 passive ([mouse-glorious.md](mouse-glorious.md)) | **V/HW** (A) ; **V** (B) ; passive **V** (C) | A: via wired PID (HW) ; B: **V** ; C: cable PID (I) | **U** (set-only) | Model O PRO / D 2 PRO PIDs `2027/2034` reuse A |
| Cooler Master (MM712/MM731) | status `46 81 00` -> `47 81 ...` on interface 3 ([mouse-coolermaster.md](mouse-coolermaster.md)) | **V/HW** byte 10 (MM712) | **V** byte 11 | **V/I** `4A 81 03` reply byte 10 (125-1000) | Code is derived, not quoted from a capture |
| Roccat / Turtle Beach | - ([mouse-misc-roccat-zowie-office.md](mouse-misc-roccat-zowie-office.md)) | **U** | **U** | old wired Kone only (**V**) | No open protocol for the Air line |
| Zowie / BenQ | - (same file) | n/a (wired) / **U** (EC-CW) | n/a | n/a | Rate set by hardware button only |
| Microsoft / HP / Dell office | Bluetooth: Windows PnP property; Nano-receiver Dell = Logitech HID++ (same file) | **V/HW** `DEVPKEY {104EA319-6EE2-4701-BD47-8DDBF425BBE5},2` for BT; **U** for 2.4 GHz non-Logitech | n/a for BT (GATT has none) | n/a | |

## Cross-cutting recommendations

1. **Open collections shared, never exclusive**, and **never send a setter**. Every protocol above mixes read opcodes and write opcodes in the same command space (Razer class/ID with bit 7, Logitech function numbers, SteelSeries opcodes, ASUS `51 xx`, Cooler Master `4x 02/04`, Glorious function codes). Keep a hard allow-list of the exact request bytes documented in each file.
2. **Sleep behaviour is universal**: wireless mice sleep after seconds of idle; the dongle then answers with a timeout (Razer status `0x04`, Logitech HID++ error 9 or no reply, Corsair/HyperX/ASUS no reply or zeros, SteelSeries/Glorious level 0 or status `A4`). Treat it as "no reading": keep the last value greyed for ~5 minutes (HaloBattery uses 300 s; Cooler Master 30 min), never raise a "battery 0 %" alert from a single zero.
3. **Dedupe** one physical mouse seen via cable and dongle (Razer serial, Logitech unit ID `0x0003`, name match); prefer the entry that says charging.
4. **Polling-rate readback exists only for**: Logitech (0x8060/0x8061), Razer (0x85/0xC0), Corsair Bragi (0x01), ASUS (`12 04`), HyperX NGENUITY-3 (`32 02`), Cooler Master (`4A 81 03`). For SteelSeries, Glorious, Dart, Zowie, Roccat Air, Microsoft etc. there is no documented read command. Fallback that works for every mouse: **measure** the report rate from Raw Input (`WM_INPUT` with `RIDEV_INPUTSINK` on the mouse; compute the median inter-report interval over ~1 s of motion; timestamps via `QueryPerformanceCounter`). Needs motion, so show "move mouse to measure" when idle.
5. **Windows HID specifics**: one USB interface = several top-level collections with separate device paths; select by `(vid, pid, interface/mi_xx, usage page, usage)`. Feature-report devices (Razer, Glorious A/B) use `HidD_SetFeature/GetFeature` (open with access 0 or generic read/write, share read|write); output/input-report devices (Logitech, SteelSeries, Corsair, ASUS, HyperX, Cooler Master) use `WriteFile`/`ReadFile` (overlapped, with timeouts). Report ID 0 devices need the leading `0x00` byte and the full report length.
6. **Do not run two pollers at once** against the same dongle (iCUE, G HUB, Synapse, SignalRGB poll as well): replies can interleave. Match replies by header (Logitech sw-id, Razer command class/ID, Corsair property equality, ASUS first 3 bytes, Razer CRC).
7. **Third-party cross-check**: HaloBattery (MIT, https://github.com/HeyOkay/HaloBattery) is a Windows tray app doing exactly this for ~30 brands with a community-tested device list; razer-battery-report (MIT) is a Rust reference for Razer; both are safe to read for behaviour.

## Gaps (not found in any open source as of 2026-11-24)
* Corsair mice above 1 kHz (polling codes), newer Corsair mice PIDs (Sabre Pro Wireless, Darkstar, Nightsabre...).
* SteelSeries polling-rate read; newer SteelSeries wireless PIDs; Rival 3/650 battery reply shape on hardware.
* HyperX Pulsefire Haste (gen 1) Wireless / Haste 3 Wireless PIDs; Dart polling read.
* Roccat/Turtle Beach Kone/Burst "Air" battery; Zowie wireless; Microsoft 2.4 GHz mice.
* Razer: independent hardware confirmation of the `0xC0` polling *read*; per-model quirks of the charging byte.
* Logitech: Bolt receiver name register offsets; confirmation that `0x8061` returns the live rate (not the stored profile rate) while onboard profiles are active.
