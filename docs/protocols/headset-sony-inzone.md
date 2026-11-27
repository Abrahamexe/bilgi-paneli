# Sony INZONE H5 / H9 II / Buds and PULSE 3D

VID **0x054C**. Tags: **[V]** verified in source (URL), **[I]** inferred, **[U]** unknown.
Source (GPL-3.0, facts only): https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/ - `protocols/sony_inzone_protocol.hpp`, `sony_inzone_h5.hpp`, `sony_inzone_h9_ii.hpp`, `sony_inzone_buds.hpp`, `sony_pulse_3d.hpp`.

## 1. Models

| Model | PID | Collection | Notes |
|---|---|---|---|
| INZONE H5 (WH-G500), 2.4 GHz dongle | 0x0EBF | **usage page 0xFF04, usage 0x0002** (dongle exposes 3 top-level collections) | Hci-over-HID |
| INZONE H9 II, dongle | 0x0FA8 | **usage page 0xFF04, usage 0x0001, interface 5** (Windows "MI_05&COL03") | same protocol |
| INZONE H3 / H7 / H9 (gen 1) | **[U]** | **[U]** | not in HC |
| INZONE Buds (WF-G700N), dongle | 0x0EC2 | report-by-ID stream (see 3) | passive |
| PULSE 3D (CFI-ZWH1), dongle | 0x0D5E | **usage page 0xFF01, usage 0x0020, interface 3** | passive |

All **[V]**.

## 2. INZONE H5 / H9 II: "Sony vendor HCI over HID" **[V]**

* Report ID **0x02**, 64-byte report (63-byte payload). Each report carries a Bluetooth-HCI-shaped packet; host commands use opcode **0xFC00** (bytes `00 FC`), the dongle replies with vendor event code **0xFF**.
* **Command frame** (buffer incl. report ID):
  `[0]=02 [1]=hid_length(12+payloadLen) [2]=01 (HCI command) [3]=00 [4]=FC [5]=param_length (8+payloadLen) [6..7]=96 C3 (Sony key id 0xC396 LE) [8]=address [9]=event id [10]=event type [11..12]=transaction id LE [13..]=payload [13+payloadLen]=checksum`,
  **checksum = sum(bytes[6 .. 12+payloadLen]) & 0xFF**.
  address: **0x41 = PC -> receiver (dongle/headset)**, 0x21 PC -> transmitter.
  event types: GET 0x01, SET 0x02, RET 0x10 (reply to GET), NTFY 0x20 (reply to SET), NTFY_ACTIVE 0xA0 (unsolicited).
  Transaction IDs: use >= 2 (the dongle's unsolicited pushes use TID 1).
* **Battery request = event id 0x04 (BATTERY_INFO), type GET, empty payload.** Worked example (TID 2):
  `02 0C 01 00 FC 08 96 C3 41 04 01 02 00 A1 00...` (checksum 0xA1 = (0x96+0xC3+0x41+0x04+0x01+0x02+0x00) mod 256, computed by me from the rule above **[I]** arithmetic).
* **Reply** (input report 0x02): `[0]=02 [1]=hid_length [2]=04 (HCI event) [3]=FF [4]=param_length [5]=00 [6..7]=96 C3 [8]=address (high nibble must be 1 = PC) [9]=event id [10]=type (0x10) [11..12]=TID LE [13..hid_length]=payload`, checksum = sum(bytes[5 .. hid_length]) & 0xFF == byte[hid_length+1]. Discard frames that fail validation or whose event id/TID do not match (skip NTFY_ACTIVE with TID 1).
* **BATTERY_INFO payload: `[0]` = charger flag (non-zero = charging), `[1]` = percent 0..100; `0xFF` percent = headset offline / no cached value.** **[V]**
* Read timeout 500 ms per attempt, up to 10 attempts. **[V]**
* Related events (not needed): 0x01 2.4 GHz connect status, 0x22 chat-mix.

## 3. INZONE Buds (0EC2) **[V]**
* The dongle **sends unsolicited reports; no request**. Battery report: **byte[1] = 0x12 and byte[2] = 0x04** (report ID byte[0] = 0x02). **byte[14] = right bud %, byte[16] = left bud %, byte[18] = charging case %** (0..100), checksum byte[19] = (byte[14] + byte[16] + 117) & 0xFF. HC reports min(left, right); no charging flag.
* Poll by reading for up to N reports until the 0x12/0x04 report arrives.

## 4. PULSE 3D (0D5E) **[V]**
* Dongle does not answer queries; it emits an **8-byte status report, report ID 0xB0**, on change (power on/off, button press, charge cable plugged, battery step ~hourly).
* **byte[3] = battery 0..100 in steps of 10, or 0x80 while charging; byte[4] bit 0x08 = headset connected (cleared when powered off).** Remaining bytes undecoded.
* Output report 0xB1 exists but writes are rejected, so there is **no way to request a status**; a tray app must keep the handle open and cache the newest report; a one-shot read may wait until the user presses a button.

## 5. Quirks
* Which HID collection carries the protocol differs per dongle; select by usage page/usage on Windows (they share one interface number on some dongles). **[V]**
* INZONE Hub running concurrently also reads the pushes; match by TID. **[I]**
* Wired/Bluetooth modes of H9 II: Bluetooth path is separate (see `headset-bluetooth-generic.md`). **[I]**

## 6. Sources
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/protocols/sony_inzone_protocol.hpp
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/sony_inzone_h5.hpp , `sony_inzone_h9_ii.hpp`, `sony_inzone_buds.hpp`, `sony_pulse_3d.hpp`
