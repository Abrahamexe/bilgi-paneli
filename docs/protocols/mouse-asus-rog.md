# ASUS ROG / TUF mice (Keris, Gladius, Chakram, Harpe, Spatha, Strix Impact, OMNI receiver)

Scope: battery %, charging, polling rate. Research date 2026-11-24. Tags: **V** verified in source, **I** inferred, **U** unknown, **HW** third-party hardware report.
Licenses: G-Helper (GPL-3) and libratbag (MIT) read for protocol facts only, restated; HaloBattery (MIT) cross-check.

## 1. Device identification

VID = `0x0B05` (ASUSTeK). One command protocol (64-byte reports, 2-byte command prefix) shared by the family.

| Mouse | USB (cable) PID | 2.4 GHz dongle PID | Source |
|---|---|---|---|
| ROG Chakram | `18E3` | `18E5` | V libratbag asus-rog-chakram.device |
| ROG Chakram Core | `1958` | - | V |
| ROG Chakram X | `1A18` | `1A1A` | V |
| ROG Gladius II (Origin) | `1845` / `1877` | - | V |
| ROG Gladius II Wireless | - | `18A0` (G-Helper; interface `mi_02`) | V G-Helper GladiusIIWireless.cs |
| ROG Gladius III Wireless | `197D` | `197F` | V G-Helper GladiusIIIWireless.cs |
| ROG Gladius III Wireless AimPoint | cable `1A70`? U | `1A72` | V (dongle) G-Helper GladiusIIIAimpoint.cs; HaloBattery cites `0B05:1A72` |
| ROG Keris | `195C` | - | V libratbag |
| ROG Keris Wireless | `195E` | `1960` | V libratbag asus-rog-keris-wireless.device |
| ROG Keris Wireless AimPoint | `1A66` | `1A68` | V |
| ROG Keris II Ace / Origin | cable `1B16` / `1C0C` | via OMNI receiver `1ACE` | V G-Helper |
| ROG Harpe Ace AimLab | `1A92` | `1A94` | V G-Helper HarpeAceAimLab.cs |
| ROG Harpe Ace Mini | `1B63` | OMNI `1ACE` | V |
| ROG Harpe II Ace / Extreme | `1C69` | own receiver `1AD0` ("ROG SPEEDNOVA 8K RECEIVER") | V G-Helper HarpeIIAce.cs; HW HaloBattery (Harpe II Ace confirmed) |
| ROG Spatha X | `1977` | `1979` | V |
| ROG Strix Impact II Wireless | `1947` | `1949` | V libratbag |
| ROG Strix Impact III Wireless | `1A88`(wired Impact III) | `1AD7` | V libratbag |
| ROG Strix Carry | - | `18B4` (interface `mi_01`) | V |
| ROG Pugio / Pugio II | `1846` (mi_02) / `1906` | `1908` | V |
| TUF Gaming M4 Air | `1A03` | - | V libratbag |
| TUF Gaming Mini WL (Miku) | `1C56` | `1C57` | V |
| MD200 | - | `1A24` (mi_02) | V G-Helper |
| ROG OMNI receiver (shared by several mice + keyboards) | - | `1ACE` | V |

## 2. HID interface / collection (Windows)

* Standard ROG mice: **USB interface 0 (`mi_00`)**, the vendor collection, **report ID 0x00**, 64-byte payload (G-Helper `USBPacketSize() = 65` = ID + 64). Exceptions in G-Helper: Gladius II (Origin/Wireless), Pugio, MD200, ... `mi_02`; Strix Carry `mi_01`; MD301 `mi_03`.
  V G-Helper AsusMouse.cs L560-L572, per-model constructors.
* **OMNI receiver `1ACE` / Harpe II Ace receiver `1AD0`**: collection `mi_02&col03`, **report ID 0x03** (64-byte, the ID is sent as first byte); keyboard channel is `mi_02&col02`. V G-Helper models (`base(..., "mi_02&col03", 0x03)`), HW HaloBattery.
* I/O type: **output report (WriteFile / HidD_SetOutputReport style `Write`) + input report read**, not feature reports. V G-Helper WindowsUsbProvider.cs (`HidStream.Write` / `Read`), AsusMouse.cs L614-L700. Timeout 300 ms; 3 retries.
* Flush stale input before each request (G-Helper `Drain`) and match the reply by the first 3 bytes (`reportId`, cmd, sub) because the mouse also emits unsolicited reports (`12 01 ...` button bitmap, profile change). V AsusMouse.cs L630-L700.

## 3. Battery + charging

Request (65 bytes, ID byte first): `<rid> 12 07 00 ... 00` (rid = `00` normally, `03` on OMNI/Harpe II receivers).
Reply `<rid> 12 07 ...`:

| Reply byte (ID at 0) | Meaning |
|---|---|
| 1-2 | `12 07` echo |
| **5** | **battery**: percent 0..100 on newer mice; on older models (Chakram, Keris Wireless, Strix Impact II ...) a **level 0..4** (ASUS' own indicator steps of 20 %; TechPowerUp notes Armoury Crate also shows 20 % steps) |
| 6 | auto-power-off setting |
| 7 | low-battery warning threshold % |
| 8-9 | looks like cell voltage in mV little-endian (`FD 0E` = 3837 mV in a HW capture) - unused |
| **10** | **charging: non-zero = charging** |

V G-Helper AsusMouse.cs L782-L822 (`GetBatteryReportPacket`, `ParseBattery`, `ParseChargingState`). HW: Harpe II Ace on its receiver: `03 12 07 00 00 35 02 14 fd 0e 00 ...` = 53 %, not charging (HaloBattery docs). The other models: "Unverified" per HaloBattery.
* Per-model: some models have no battery (`HasBattery()` false in G-Helper for Chakram, Chakram X, Spatha X; they use other opcode layouts: U, probably the same 12 07 returns level 0..4) - treat "response ff aa" as unsupported.
* **Standby**: when the mouse is asleep the reply is all zeros or percent 0 with no charging; 0 without charging = standby, not empty (G-Helper: `SetDeviceReady(Battery > 0)`; HaloBattery same). Error packet `ff aa` in bytes 1-2 = "command not known" (G-Helper `IsMouseError`).

## 4. Polling rate

Request `<rid> 12 04 00 ...` (cmd `12 04`, sub `00`). Reply `<rid> 12 04 00 ...`; **byte 13 = polling code**:

| Raw (byte 13 & 0x07) | Hz | Models |
|---|---|---|
| 0 | 125 | all |
| 1 | 250 | all |
| 2 | 500 | all |
| 3 | 1000 | all |
| 4 | 2000 | 8K "Booster" mice only |
| 5 | 4000 | " |
| 6 | 8000 | " |

V G-Helper AsusMouse.cs L31-L40 (enum), L1063-L1100 (`GetReadPollingRatePacket`, `ParsePollingRate`). On "Booster" (ROG 8K / OMNI) receivers the code is in the **high nibble** (`(byte13 >> 4) & 0xF`, if non-zero) else the low 3 bits. "Booster" is detected once by sending `03 7D 20 02` to the OMNI collection and checking reply byte 5 == 1 (V G-Helper PeripheralsProvider.cs L698-L712). 1K-only models: only 125-1000 valid (G-Helper falls back to the max supported when the value is out of range).
libratbag's older layout (`12 04` settings block; polling index 0..3 at byte 8 or `raw[12] & 7` for "SETTINGS_V2" devices) agrees for 125-1000. V libratbag asus.c L330-L410, asus.h quirks.

## 5. Model name

* The cable PID gives the model directly (table above). Through the **OMNI receiver `1ACE`** the mouse model is unknown until asked: send `03 12 12 02` (report ID 3, libratbag) - reply bytes 5..16 = 12-byte ASCII serial whose prefix identifies the model (libratbag matches `OmniSignature` in `.device` files); G-Helper instead reads a pair list: `01 A0 00 00` -> reply bytes from 5, **four bytes per slot, first two bytes = paired device PID little-endian**. V G-Helper PeripheralsProvider.cs L655-L690, libratbag asus.c L584-L611.
* Dongle USB string is generic ("ROG KERIS WIRELESS RECEIVER" style or "ROG SPEEDNOVA 8K RECEIVER"). Use PID table.

## 6. Quirks

* U: coexistence with Armoury Crate is not documented by either project; open the collection shared (non-exclusive), as G-Helper does (`OpenOption.Exclusive = false`, PeripheralsProvider.cs L653-L656).
* Never send the `51 xx` family (setters: `51 31` DPI/polling/etc., `51 37` energy, `51 35` lift-off) or `50 xx` profile switch.
* Unsolicited input reports `12 01 ...` (raw button bitmap) and profile-change events share the input pipe - filter by reply header.
* Sleeping cable-less mouse: dongle returns zero packet (battery 0, no charging) -> "asleep". With cable: battery reads while charging.
* Report length: always 65 bytes written (ID + 64), including OMNI (`03` as ID, 64-byte total in HaloBattery wording "a 64-byte packet `03 12 07`" - U whether the ID is counted in the 64; HaloBattery says hidapi keeps report ID 3 in front of the reply).

## 7. Sources
* G-Helper (GPL-3): https://github.com/seerge/g-helper/tree/main/app/Peripherals/Mouse (AsusMouse.cs, Models/*.cs), app/Peripherals/PeripheralsProvider.cs, app/AnimeMatrix/Communication/Platform/WindowsUsbProvider.cs
* libratbag (MIT): https://github.com/libratbag/libratbag/blob/master/src/driver-asus.c, src/asus.c, src/asus.h, data/devices/asus-*.device
* HaloBattery (MIT): https://github.com/HeyOkay/HaloBattery/blob/main/docs/protocols.md ("ASUS ROG Gladius III Aimpoint...", "Harpe II Ace", "OMNI receiver"), providers/asus.py
