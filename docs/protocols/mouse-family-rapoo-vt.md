# Rapoo VT / VT9 Pro family (VID 0x24AE)

Confidence: V + HW for VT9 Pro (capture on 2026-09-29, receiver 24ae:1205, cable 24ae:4405) and cross-checked against mousectl's rapoo_vt3pro driver; other Rapoo/VT models: I.

* Command interface: usagePage 0xFF00 usage 0x000E, **output report 0xBA** (31 data bytes: connection byte, command, payload length, 4-byte LE address, data). Windows reports it as 32 bytes (includes ID). Answers come via **GET_REPORT(Input)** (HidD_GetInputReport), not interrupt IN, so WebHID cannot read them; native Win32 can.
* Reply: `[0]` status 0x01 answered, `[1]` 0 for block answers / nonzero for battery answers, `[2]` percent on battery answers, payload from `[4]`. The channel is **lossy** (~40 % of frames dropped); an OK without a preceding busy is the previous command's answer, so retry and match.
* Frame: `[0]` connection (0xA5 receiver, 0xFF wired; not validated by the firmware), `[1]` cmd (0xA4 read, 0xA5 write, **0xAA battery**), `[2]` length, `[3..6]` address LE.
* **Battery (best path, passive)**: the mouse sends unsolicited input report **0xBB** about every 3 s: `b0 51 20 03 01 62` -> byte[5] = percent (0x62 = 98), byte[4] link code (1 wireless, 2 wired = hypothesis). No request needed.
* **Battery (active)**: cmd 0xAA, address 0, length 0: reply marker `data[1]` 1 or 2 (2 = charging), `data[2]` percent. Charging readable only here. Alternative: feature report 0x2A on usagePage 0xFF0B usage 0x0104: percent at report data byte 8 (after the ID).
* **Polling**: read 4 bytes at EEPROM 0x880: `[0]` rate on 2.4 GHz, `[2]` rate on cable. Codes: 0x08=125, 0x04=250, 0x02=500, 0x01=1000, 0x84=2000, 0x82=4000 (8K decoded but not offered). Other addresses: 0x884 sensor, 0x888 DPI X (7x u16 LE + count), 0x898 active stage, 0x8C0 timing (sleep minutes at [2]).
* Quirks: block read length must match the Windows driver's block lengths (the device answers with the block it has); report length 31 data bytes else write refused.
* Sources: https://unpkg.com/@openmouse/protocol@0.60.0/dist/rapoo/index.js and drivers/rapoo/hid.js
