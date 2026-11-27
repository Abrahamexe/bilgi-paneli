# Other 2.4 GHz headsets: Fifine, MCHOSE, Lenovo, Poly/Plantronics BT600, plus ASUS ROG, JBL Quantum, Cooler Master, Attack Shark (mostly unknown)

Tags: **[V]** verified in source (URL), **[I]** inferred, **[U]** unknown.
Source for all [V]: HeadsetControl (GPL-3.0, facts only) https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/

## 1. FIFINE HT6 Wireless (VID 0x3142, PID 0x00c3) **[V]**
Source: `fifine_ht6.hpp` (protocol recovered from a USB capture of the vendor "SoundLink" app; verified on dongle fw 1.8 / headset fw 4.2).
* Dongle exposes several HID collections; the vendor one is **usage page 0xFF90, usage 0x01**; **report ID 0x55, 64-byte report in both directions**. Windows + Linux (macOS untested).
* Request (state query): `55 65 01 00` + zeros.
* Reply: `55 65 <BB> <SS> <MM> ...`:
  * **byte[2] = battery percent**, **byte[3] = battery status; 0x03 = charging**, byte[4] = sound mode, a further byte = inactive time (0xFF = disabled).
  * **Headset powered off: byte[1] == 0x0A; link lost: byte[3] == 0x1A** (check these before using the battery value).
* The dongle also pushes periodic frames; filter on `byte[0]==0x55 && (byte[1]==0x65 || byte[1]==0x0A || byte[3]==0x1A)`.
* LED engine writes must follow a specific stop/config/save/start sequence - irrelevant for battery.

## 2. MCHOSE X9 (VID 0x3837, PID 0x6045) and V9 Pro (VID 0x291d, PID 0x385d) **[V]**
Source: `mchose_x9.hpp`, `mchose_v9_pro.hpp` (capture of the "M HUB" app; V9 Pro also verified on Linux: reply `55 65 32 02` = 50 %).
* Same vendor collection shape as Fifine: **usage page 0xFF90, usage 0x01, report ID 0x55, 64 bytes** (X9 dongle exposes 6 top-level collections; on Windows select by usage page).
* Request: `55 65 01 00 ...`. Reply: `55 65 <VV> <FF> ...` with **FF (byte[3]) = 0x02 meaning "battery field"; VV (byte[2]) = percent**. Skip other frames (heartbeats). No charging/offline decoding known. **[V]**
* Both chipset families look like C-Media based dongles with the same "55 65" protocol; **[I]** other Fifine/Redragon/Attack-Shark-style dongles built on the same C-Media SoC may answer the same query - probe `55 65 01 00` on any vendor 0xFF90 collection (read-only, low risk).

## 3. Lenovo Wireless VoIP Headset (VID 0x17EF, PID 0xA07D) **[V]**
Source: `lenovo_wireless_voip.hpp`. Collection **usage page 0xFF07, usage 0x0222, interface 3**. 61-byte **feature report** (SetFeature) `24 <cmd> ...`, then read the input report (reply report ID 0x27, must be 61 bytes). Command 0x01 = status: **byte[7] = battery percent**; no charging flag.

## 4. Poly/Plantronics Voyager 8200 UC via BT600 dongle (informational) **[V]**
Source: `plantronics_bt600.hpp`. **usage page 0xFFA2, usage 0x03, interface 3**, report ID 0x07; Poly native get/set protocol (`07 01 01 10 <len> <addr> 00 00 <op> <grp> <item> <value...>`), addr 0x20 = headset via Bluetooth. Requires a session handshake (2-byte output `06 01`, HELLO to dongle then to headset). Battery returns charging state plus remaining talk time. Exact decode in the file. Out of scope for gaming headsets.

## 5. ASUS ROG Delta S / Delta II / Strix Go / Cetra True Wireless
* **No reverse-engineered battery protocol found.** ROG Delta S Wireless: 2.4 GHz + Bluetooth, spec page https://rog.asus.com/headsets-audio/headsets/wireless-headsets/rog-delta-s-wireless-model/spec/ (no HID documentation). ASUS VID is **0x0b05** **[I]** (general knowledge, not from a source here); PIDs **[U]**.
* Battery shown by Armoury Crate; capture it with USBPcap. Cetra True Wireless / Cetra II Core are Bluetooth earbuds -> `headset-bluetooth-generic.md`.

## 6. JBL Quantum (810/910/Q350/Q400...)
* **No public protocol found** (search returned only JBL spec sheets). VID Harman/JBL **[U]**. JBL Engine software shows battery. Quantum models with Bluetooth (810, 910, 350) can be read via the Bluetooth generic path when paired. **[I]**

## 7. Cooler Master (MH670, MH752, MH650 ...)
* **No public protocol found.** Cooler Master VID is **0x2516** **[I]** (general knowledge); MH670's 2.4 GHz dongle and MasterPlus+ battery readout undocumented. **[U]**

## 8. Attack Shark / Edifier Hecate / Redragon / generic OEM headsets
* Nothing documented for Attack Shark headsets. Cheap OEM dongles frequently use **C-Media (VID 0x0d8c)** or **Realtek** audio chips; battery is then either not exposed, or exposed via a vendor HID collection like the 0x55-report family above (Fifine / MCHOSE). **[I]**
* Recommended tactic: enumerate HID collections of the dongle; if any has usage page 0xFF00-0xFFFF with report ID 0x55 and 64-byte reports, try the read-only `55 65 01 00` query and see if a reply with plausible battery appears. Never send writes you cannot reverse.

## 9. Sources
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/fifine_ht6.hpp , `mchose_x9.hpp`, `mchose_v9_pro.hpp`, `lenovo_wireless_voip.hpp`, `plantronics_bt600.hpp`
* https://rog.asus.com/headsets-audio/headsets/wireless-headsets/rog-delta-s-wireless-model/spec/
