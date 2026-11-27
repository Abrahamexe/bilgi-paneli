# HyperX Cloud wireless family (Kingston -> HP vendor-ID change)

Tags: **[V]** verified in source (URL), **[I]** inferred, **[U]** unknown.

Sources (facts only): HeadsetControl (GPL-3.0) `lib/devices/hyperx_*.hpp`; HyperHeadset (MIT, Rust, has the broadest HyperX coverage incl. Cloud III / III S / II Core / Flight S): https://github.com/LennardKittner/HyperHeadset (`src/devices/*.rs`, `old_cloud_2_doc/Headset_protocol.md`, derived from NGENUITY 2 captures).

## 0. The vendor-ID / PID mess (important)

Early HyperX products enumerate under **Kingston VID 0x0951**. After HP bought HyperX, new products and re-spun dongles enumerate under **HP VID 0x03F0**, with different PIDs and **often a different protocol**. You must match on (VID, PID) pairs, never on PID alone (0x1718 appears under both vendors in the sources).

| Product | VID | PID(s) | Protocol "dialect" | Source |
|---|---|---|---|---|
| Cloud II Wireless (Kingston, original) | 0x0951 | 0x1718 (HC); HyperHeadset: 0x1718, 0x0b92, 0x16EA, 0x16EB (0x16EA/0x16EB are commented "Cloud Flight S") | A: 62-byte, report 0x06 / reply 0x0B | HC `hyperx_cloud_2_wireless_kingston.hpp`; HH `cloud_ii_wireless.rs` **[V]** |
| Cloud II Wireless (HP) | 0x03F0 | 0x0696, 0x018B (HC); HH "DTS" file: 0x1718, 0x018B, 0x0D93, 0x0696 | B: 20-byte `06 FF BB cmd` | HC `hyperx_cloud_2_wireless.hpp`; HH `cloud_ii_wireless_dts.rs` **[V]** |
| Cloud Flight Wireless | 0x0951 | 0x16C4 (old), 0x1723 (new); HH also lists 0x0e90, 0x1749 under VID 0x03F0 (HP) | C: 20/64-byte `21 FF 05` voltage | HC `hyperx_cloud_flight.hpp`; HH `cloud_flight_wireless.rs` **[V]** |
| Cloud Flight S | 0x0951 (probable) | 0x16EA / 0x16EB **[I]** (comment-only in HH) | A or C **[U]** | HH comment **[I]** |
| Cloud Alpha Wireless | 0x03F0 | 0x098D (HC); HH adds 0x1743, 0x1765 | D: 31/64-byte `21 BB cmd` | HC `hyperx_cloud_alpha_wireless.hpp`; HH `cloud_alpha_wireless.rs` **[V]** |
| Cloud Alpha 2 Wireless | 0x03F0 | 0x08BE (control MCU; 0x0ABE is audio) | E: 64-byte `50 02` | HC `hyperx_cloud_alpha_2_wireless.hpp` **[V]** |
| Cloud II Core Wireless | 0x03F0 | 0x069F, 0x0995 | F: 64-byte `66 cmd` | HH `cloud_ii_core_wireless.rs` **[V]** |
| Cloud III Wireless | 0x03F0 | 0x05B7, 0x0C9D | F: 62-byte `66 cmd` | HH `cloud_iii_wireless.rs` **[V]** |
| Cloud III S Wireless | 0x03F0 | 0x06BE, 0x02CC | G: 64-byte `0C 02 03 01 00 cmd` | HH `cloud_iii_s_wireless.rs` **[V]** (HH notes dongle can go "deaf", issue #36) |
| Cloud Stinger 2 Wireless | 0x03F0 (probable) | **[U]** - HH README lists it as supported but no PID appears in its device tables; the DTS table (0x0D93) is the likely carrier **[I]** | probably B **[I]** | https://github.com/LennardKittner/HyperHeadset (README) |
| Cloud Stinger (original) Wireless / Stinger Core Wireless DTS (0x03F0:0x0A8E) / Cloud Mix 2 / Core Wireless | - | open HH issues #57, #75, #54 | **[U]** (unsupported) | https://github.com/LennardKittner/HyperHeadset/issues/75 |
| Cloud 3 (wired), Alpha S (wired) | 0x03f0:0x089d / 0x03cc; 0x0951:0x16ed | n/a | **no battery** | HC **[V]** |

Interface/usage selection (Windows): HeadsetControl uses usage page **0xFF90, usage 0x0303, interface 0** for Cloud II Wireless (HP) and no hint for the others; the Alpha 2 uses usage page **0xFF13, usage 0xFF00, interface 2** **[V]**. HyperHeadset on Windows simply tries each HID interface of the matching (VID,PID) with the first query until one both accepts the write and answers (and warns that hammering all interfaces can destabilise the Cloud III S dongle) **[V]** `src/devices/mod.rs`. If a device refuses output reports (Windows error 1, "Incorrect function"), HH retries as a **feature report** (HidD_SetFeature) **[V]**.

## 1. Dialect A - Cloud II Wireless (Kingston 0x0951:0x1718, 62-byte)

* Request (62 bytes, byte[0] = report ID 0x06): `06 00 02 00 9A 00 00 68 4A 8E 0A 00 00 00 BB <cmd> <arg> 00...` (fixed header bytes; byte[14] = 0xBB; byte[15] = command; byte[16] = argument). **[V]** (HC builds exactly these bytes; HH has the same constants in comments).
* Commands: `0x02` battery level; `0x03` charging state. **[V]**
* Wait ~100 ms after the write, then read 64 bytes. Reply header: byte[0]=0x0B (report ID 11), byte[2]=0xBB, byte[3]=echo of command. **[V]**
* Battery: **byte[7] = percent 0..100**. Charging: command 0x03 -> **byte[4]: 1 = charging**; HH maps 0 not charging, 1 charging, 2 fully charged, other = error **[V]** (HH `ChargingStatus`).
* HC also issues a short `GET_INPUT_REPORT(0x06)` "prepare" read before each command **[V]** (`prepareDevice`); this uses `HidD_GetInputReport` on Windows.
* Voltage: not available in this dialect (`voltage_mv` None). **[V]**

## 2. Dialect B - Cloud II Wireless HP (0x03F0:0x0696 / 0x018B / 0x0D93)

* Request 20 bytes: `06 FF BB <cmd> 00 ...` (HH: `[0..2] = 06 FF BB`, `[3] = cmd`). **[V]**
* Commands: `0x02` battery, `0x03` charging, `0x08` mic connected, `0x07` auto-shutdown. **[V]** (DTS file). `0x01` "wireless status" works but resets headset state (unmutes), so do not poll it **[V]** HH comment.
* Reply (20 bytes, HC requires exactly 20): `06 FF BB <cmd> <b4> <b5> <b6> <b7> ...`; header check `byte[0..3]`. HC: **battery = byte[7]**, **voltage = (byte[5]<<8)|byte[6] mV**, **charging command 0x03 -> byte[4] == 1**. **[V]** (HC `hyperx_cloud_2_wireless.hpp`: `VOLTAGE_INDEX_1=5, VOLTAGE_INDEX_2=6, BATTERY_LEVEL_INDEX=7, BATTERY_CHARGING_INDEX=4`).
* Wait ~100 ms between write and read; HC reads with 1 s timeout. **[V]**
* Headset off: **[U]** HC has no offline state for this model (it would time out or return stale data); HH has a separate, state-resetting wireless-status query that it deliberately skips.

## 3. Dialect C - Cloud Flight (Wireless) 0x0951:0x16C4 / 0x1723 (and HP 0x0e90 / 0x1749)

* Request 20 (HC) or 64 (HH) bytes: `21 FF 05 00 00 ...` (report ID 0x21, 0xFF, command 0x05 = battery). **[V]**
* Reply length must be 15 or 20 (HC). **voltage_mV = (byte[3]<<8)|byte[4]**. **[V]**
* **Charging** is inferred from voltage: HC: voltage > 0x100B (4107) => charging, level unknown (reports -1). HH: charging if (byte[3]==16 && byte[4]>=20) or byte[3]>=17 (>= 0x1014 = 4116). **[V]** Two sources disagree by ~9 mV; use HH's threshold (stricter) or hysteresis.
* **Percent** from voltage (HH table, MIT; thresholds in mV -> percent, step 5 %): 3328->5, 3584->10, 3674->15, 3704->20, 3732->25, 3744->30, 3754->35, 3764->40, 3774->45, 3784->50, 3794->55, 3804->60, 3824->65, 3840->70, 3860->75, 3890->80, 3910->85, 3940->90, 3960->95, 3970->100. Pick the largest threshold <= voltage; below 3328 -> 5 %. **[V]** (`cloud_flight_wireless.rs`).
* HC's alternative: piecewise - voltage <= 3648: `0.00125*V` percent; > 3975: 100 %; otherwise a quartic polynomial fitted from Logitech G633 data. **[V]** (HC `hyperx_cloud_flight.hpp`; the HH table is simpler and preferable.)
* HH also parses unsolicited reports: byte[0]=0x64 (power; byte[1] 1 or 3 = wireless connected), 0x65 (mute; byte[1]==4 muted). **[V]**
* Headset off: **[U]** (no offline state in HC). Linux kernel / other: not documented.

## 4. Dialect D - Cloud Alpha Wireless (0x03F0:0x098D; also 0x1743, 0x1765)

* Requests (31 bytes HC; 64 HH): `21 BB <cmd> 00...`.
  * `0x03` wireless status: reply byte[3] == 0x01 -> **headset not connected** (HC). (HH labels cmd 3 "wireless status", reply code 36.) **[V]**
  * `0x0C` charging: reply byte[3] == 0x01 -> charging (HC then reports charging with unknown level). HH: `ChargingStatus::from(byte[3])` 0/1/2/else. **[V]**
  * `0x0B` battery: reply **byte[3] = percent**. **[V]**
* HC does 3 sequential round trips (connected -> charging -> battery), each with 2 s timeout. **[V]**
* Unsolicited response codes (HH): 37 battery, 38 charging, 36 wireless, 35 mute, 34 sidetone; the command byte is at reply byte[2], the value at byte[3]. **[V]**

## 5. Dialect E - Cloud Alpha 2 Wireless (0x03F0:0x08BE)

Verified on Windows by the HC author (battery on battery, charging, headset off).
* Interface: vendor collection **usage page 0xFF13 / usage 0xFF00, interface 2**; 64-byte reports laid out `[cmd][sub][payload...]`. Replies use `cmd+1`. **[V]**
* Connection: write `52 01` -> reply `53 01 <connected>`; **byte[2] == 0 -> headset not connected**. **[V]**
* Battery: write `50 02` -> reply `51 02 <level %> <charge state> <temp C, i16 LE> <voltage mV, u16 LE> ...`:
  * byte[2] = percent (reject > 100), byte[3] = charge state (**1 = charging**), bytes[4..5] temperature, **bytes[6..7] voltage mV LE**. **[V]**
* The MCU acknowledges every command (`FF 01`, command echoed at offset 14), rejects with `FF 02`, and pushes unsolicited `FB xx` notifications (e.g. `FB 0A 01/00` on link/unlink). NGENUITY polls the same interface, so loop over reads and match on `byte[0]==cmd+1 && byte[1]==sub`. **[V]**
* First byte of each report is the report ID (0x50, 0x51, 0x52, 0x53, 0xFF, 0xFB, 0x60, 0x61...), i.e. numbered reports. **[I]**
* HC: Linux + Windows only (macOS usage reporting wrong). **[V]**

## 6. Dialect F - Cloud III Wireless (0x03F0:0x05B7 / 0x0C9D) and Cloud II Core (0x069F / 0x0995)

* Packet: byte[0] = **0x66 (102)**, byte[1] = command, remaining zero; length 62 (III) or 64 (Core). **[V]** (HH)
* Commands: `0x89` (137) get battery, `0x8A` (138) get charging, `0x82` (130) wireless status, `0x85` get auto-shutdown. **[V]**
* Reply: byte[0] = 0x66; the same command number (or 0x0D battery / 0x0C charging / 0x0B wireless as unsolicited codes) in byte[1].
  * Battery: `(byte[1], byte[2], byte[3], byte[4])` = (cmd, state1, state2, level): **level = byte[4]**, but only valid if **byte[2] != 0 || byte[3] != 0** (otherwise the reading is ignored = headset not reporting / off). **[V]**
  * Charging: **byte[2]**: 0 not charging, 1 charging, 2 full, else error. **[V]**
  * Wireless: byte[2]==1 connected. **[V]**
* Interface: no usage hint in source; use probe-per-interface as HH does. **[V]/[I]**

## 7. Dialect G - Cloud III S Wireless (0x03F0:0x06BE / 0x02CC)

* Request 64 bytes: `0C 02 03 01 00 <cmd> 00...`; cmd `0x06` battery, `0x48` charge state, `0x02` dongle-connected. Sent as an **Output report 0x0C via WriteFile, fallback SetFeature**. **[V]**
* Reply report 0x0C: **byte[5] = command echo, byte[6] = value** (0xFF = no data). Battery % = byte[6]; charge state = byte[6] (0/1/2/else as above); dongle connected: byte[6]==2. Notifications use report 0x0D with byte[4] = event (1 battery with value at byte[5], 10 charging, 12 wireless, 3 mute, 5 sidetone). **[V]**
* Known issue: dongle can stop answering queries ("deaf mode"), HH issue #36. **[V]**

## 8. Quirks summary
* Kingston vs HP PID reuse (0x1718) - always check VID too. **[V]**
* Cloud II HP dongles: mic mute state is reset by the "wireless status" query - avoid. **[V]**
* Windows: some dongles reject output reports on one interface; try each interface and fall back to feature reports. **[V]**
* NGENUITY running concurrently can interfere (HC README note for Alpha 2). **[V]**
* Wired models (Cloud 3, Alpha S) have no battery. **[V]**

## 9. URLs
* HC: https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/hyperx_cloud_2_wireless.hpp , `hyperx_cloud_2_wireless_kingston.hpp`, `hyperx_cloud_flight.hpp`, `hyperx_cloud_alpha_wireless.hpp`, `hyperx_cloud_alpha_2_wireless.hpp`, `hyperx_cloud_alpha_s.hpp`, `hyperx_cloud_3.hpp`
* HH: https://github.com/LennardKittner/HyperHeadset/tree/main/src/devices and https://github.com/LennardKittner/HyperHeadset/blob/main/old_cloud_2_doc/Headset_protocol.md
