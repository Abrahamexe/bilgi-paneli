# VAXEE Control Center protocol (VID 0x3057)

Confidence: V verified in source (OpenMouse transcription of VAXEE Control Center v3.6), HW = not stated; treat as V only.

* Models: XE Wireless 1003, NP-01S 1004, AX 1005, NP-01 1006, XE-S 1007, XE-S-L 1008, VAXEE x Ninjutso Sora 1009, E1 1010, NP-01S V2 1011, XE V2 1012, NP-01S Ergo 1013, NP-01S V3 1014, NP-01 Ergo 1015 (these are the **mouse** product ids reported inside cmd 0x0A). Host-facing PIDs: 1001, 1002, 2001, 2002, 0005 are receivers; 1003-1015 are the wired mouse. Full list: 1001-1013, 0005, 2001, 1014, 2002, 1015.
* Collection: usagePage 0xFF05, usage 0x01, **feature report ID 0x0E, 63-byte data**.
* Request (feature, ID 0x0E): `[0xA5, cmd, 1(read)|2(write), len, values...]` zero padded to 63. Wait 100 ms (vendor panel pause), then GET_FEATURE. Reply envelope: `[0xA5, cmd, 3, 1, len, data...]`, data from offset 5 (WebHID, report ID stripped).
* Commands: 0x01 firmware (u16 LE hex), 0x02 DPI stage (1-4), 0x04 DPI value (u16 LE), 0x07 polling, 0x08 tracking mode, 0x09 LOD, 0x0A mouse PID (u16 LE at data[0..1]; real model name), **0x0B battery**, **0x10 charging**, 0x13 trajectory, 0x14 profile.
* **Battery**: cmd 0x0B, len 1: reply data[0] = steps 0..20; percent = steps*5 (reject >20). Request bytes: `0E A5 0B 01 01 00 ...`.
* **Charging**: cmd 0x10, len 1: data[0] nonzero = charging.
* **Polling**: cmd 0x07 data[0] code 1..5 = 500, 1000, 2000, 4000, 8000 Hz (code 0 invalid). 2K/4K/8K only when tracking mode (cmd 0x08) is not "standard". Rates supported: receivers 1002/2001/2002 up to 4000; others 500/1000.
* Quirks: commands are read-only here (0x0B, 0x10, 0x07, 0x0A all safe); keep a queue and 100 ms spacing; poll every 30 s (OpenMouse interval).
* Sources: https://unpkg.com/@openmouse/protocol@0.60.0/dist/vaxee/index.js and drivers/vaxee/hid.js
