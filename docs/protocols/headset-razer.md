# Razer headsets (BlackShark V2 Pro, Barracuda, Kaira, Nari, Hammerhead ...)

VID **0x1532**. Tags: **[V]** verified in source (URL), **[I]** inferred, **[U]** unknown.

Honest summary: only **one** Razer wireless headset has a documented, working open-source HID battery path: **BlackShark V2 Pro for PlayStation (1532:1014)** in HeadsetControl. Everything else is [U]; for Bluetooth Razer buds/headsets use `headset-bluetooth-generic.md`.

## 1. BlackShark V2 Pro (PlayStation / 2.4 GHz dongle) - 1532:1014 **[V]**

Source: https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/razer_blackshark_v2_pro_ps.hpp (GPL-3.0; facts only). Windows/Linux/macOS supported by HC.

* Dongle is an Airoha-based design speaking the framing of Razer's own firmware updater.
* Collection: **usage page 0xFF00, usage 0x0001, interface 3**; all reports are **64 bytes, report ID 0x02**.
* **Output frame**: `02 80 <LL> <CRC lo> <CRC hi> <payload (LL bytes)>` zero-padded to 64.
  * `LL` = payload length. CRC = **CRC-16/CCITT (poly 0x1021, init 0x0000, no reflection)** computed over all 64 bytes with the CRC field (bytes 3..4) zeroed, stored little-endian.
* **Ack**: after each write, fetch the input report with **GET_REPORT (HidD_GetInputReport) for ID 0x02**; byte[1] = `0x4F` ('O') accepted, `0x46` ('F') rejected. After ~0.5 s idle the dongle may reject the next frame without acting - resend after 20 ms, then 60 ms (up to 3 sends). **[V]**
* **Payload**: `50 41 <type> <seq> <bodyLen> [<bodyLenHi> if type==0x06] <body...>` ("PA" magic). `seq` cycles 1..127; the reply carries `0x80|seq`.
  * type **0x0E** = talks to the dongle (1-byte body length). Body `E6` = read link status; **status bit 0x08 = a headset is linked**. Body `E0` = get mode; `E1 <00|01>` = set mode (0 local, 1 remote).
  * type **0x06** = relayed to the headset *while the dongle is in remote mode* (2-byte body length). Body **`31`** = read battery **voltage**.
  * type **0x08** ("customer") frames: body `<id> 00 00`; replies/notifications: `02 0e 50 49 08 .. .. .. .. .. 04 00 <id> <dir> 01 <value>`, **dir 1 = reply, 2 = notification**; **id 0x21 = battery percent**, **id 0x2a = charge state** (0 no charger, 1 charging, 2 full on charger, 3 unknown). Only state 1 counts as charging.
* **Reply layout (input report)**: `02 <LL> 50 49 ...(8 bytes) <TT> <RR> <SS> <data>`: byte[12]=type, byte[13]=`0x80|seq`, byte[14]=status (0 = OK), data from byte[15]; voltage = uint16 LE millivolts (valid 2500..4500 mV).
* Required sequence for a battery read: (1) read link status (0x0E,0xE6) - if bit 0x08 clear -> headset off; (2) switch dongle to remote mode (0x0E, 0xE1 01) + the updater's "read max length" ioctl; (3) read voltage (0x06,0x31) and the customer percent/charge (0x21 / 0x2a); (4) **always** switch back to local mode (0xE1 00) and verify. **[V]** Rarely poll: HC caches the voltage 5 min, because every remote-mode switch is a window in which a killed process leaves the dongle in remote mode (and the vendor firmware updater uses remote mode too).
* Dongle replies are broadcast to every open handle, so a tray app and CLI collide - HC serialises with a cross-process lock; implement a named mutex. **[V]**
* **Voltage -> percent** (headset firmware 1.0.7 record 5; used when the headset's own percent is unavailable/stale): 0 % = 3304 mV, 10 = 3683, 20 = 3734, 30 = 3766, 40 = 3782, 50 = 3809, 60 = 3869, 70 = 3929, 80 = 3982, 90 = 4074, 100 = 4150 (the 100 % point is an estimate). Voltage reads high on the charger. **[V]**
* Charging: customer id 0x2a state 1. Headset off: link-status bit clear. **[V]**
* Side effect noted by HC: a percent GET while charging starts an unexplained 4-second timer in the headset. **[V]**

Complexity warning: this is the heaviest protocol in the set; budget a dedicated module (CRC, ack via GetInputReport, mode switch with guaranteed restore).

## 2. Other Razer models (no usable open-source protocol found)

| Model | Known IDs | Battery status | Evidence |
|---|---|---|---|
| BlackShark V2 Pro (PC, non-PS) / V2 Pro 2023 / V3 | **[U]** | **[U]** - likely same Airoha dongle family as the PS version **[I]** | A Razer Insider post (about BlackShark V3) says the firmware is "allergic to Linux" - https://insider.razer.com/razer-support-45/spent-3-weeks-reverse-engineering-my-blackshark-v3-just-to-find-out-the-firmware-is-literally-allergic-to-linux-secrets-revealed-88366 (snippet only, not read) |
| Barracuda X (2022) | **[U]** | Razer Insider thread: over Bluetooth the % is correct, but the 2.4 GHz USB receiver on Windows reportedly exposes **voltage and passive charging frames, no valid state-of-charge response** - https://insider.razer.com/razer-support-45/razer-barracuda-x-2022-battery-issue-90816 (from a search snippet; page could not be fetched) **[I]** |
| Barracuda Pro | HyperSpeed dongle 1532:0583 (HID only, no audio interface) per Razer Insider | **[U]** | https://insider.razer.com/razer-support-45/razer-barracuda-pro-dongle-appears-like-a-hid-device-89844 |
| Kaira / Kaira Pro | **[U]** | **[U]** | none found |
| Nari / Nari Ultimate | **[U]** | **[U]** | Hackaday 2025-10: bespoke dongle, pairing protocol reverse engineered, no battery info - https://hackaday.com/2025/10/21/open-source-hack-lets-the-razer-nari-headset-work-with-linux |
| Razer "Chroma"-class HID (mice/keyboards) | 90-byte feature report, `GET_BATTERY_LEVEL` = command class 0x07 / id 0x80 | not headset; OpenRazer wiki - https://github.com/openrazer/openrazer/wiki/Reverse-Engineering-USB-Protocol and https://hyperb1iss.github.io/uchroma/reference/razer-protocol-reference.html | **[I]** do not assume headsets use it |
| Hammerhead TWS / Opus / Barracuda over Bluetooth | - | via HFP/BLE, see Bluetooth doc | - |

## 3. Practical advice for the tray app
* Ship BlackShark V2 Pro PS support behind an "experimental" flag; for all other Razer headsets rely on the Bluetooth generic path or show "not supported over 2.4 GHz".
* To add a model: capture Razer Synapse traffic (USBPcap) while the headset charges/discharges and compare with the framing in section 1 (the `PA` magic + CRC frame is the likely common envelope **[I]**).

## 4. Sources
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/razer_blackshark_v2_pro_ps.hpp
* Razer Insider threads and OpenRazer wiki as listed above.
