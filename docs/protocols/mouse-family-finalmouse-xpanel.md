# Finalmouse xpanel protocol (VID 0x361D): UltralightX (ULX) and Starlight X (SLX)

Confidence: V in OpenMouse source (decoded from Finalmouse's xpanel web app). ULX decoders hardware-oriented, SLX report ids **inferred, "unverifiable without SLX hardware"**.

* IDs: ULX dongle 0x0100 (usagePage 0xFF00, usage 0x0001), ULX mouse direct 0x0102; SLX dongles 0x0300/0x0301 (usagePage 0xFF00), SLX mouse 0x0310/0x0311.
* Reports: ULX output 0x04 (main), input 0x05; dongle output 0x02, dongle input 0x03. SLX output 0x01, input 0x02 (I). Output body 63 bytes: `[len = 2+payload, 0x80|cmd, payloadLen, payload...]`.
* Read model: the mouse does not answer single queries; sending the "wake all" command (cmd 96 / 0x60) and the dongle-info command (cmd 10) triggers a burst of unsolicited input reports, collected ~1 s plus 75 ms. Input body: `[innerLen, cmd, payloadLen, payload...]`.
* **Battery (ULX)**: cmd 5, u16 LE millivolts, percent computed by piecewise-linear calibration: volts 3.0/3.62/3.66/3.74/3.88/4.17/4.38 -> 0.2/5/10/25/50/75/100 % (>=4.38 V -> 100, <=3.0 V -> 0). **SLX**: cmd 38: payload[0] = state of charge percent, [1..2] mV; cmd 37 = charging state (1 = charging); cmd 36 = link state. ULX charging flag: not found (U).
* **Polling**: cmd 4 (ULX) / cmd 17 (SLX): u16 LE **Hz directly** (500/1000/2000/4000/8000). DPI cmd 3 / 16 u16 LE. Firmware: cmd 12 (mouse), 11 (dongle RF), mouse serial 14, RSSI 13 (signed byte).
* Quirks: close xpanel/Finalmouse software first (OpenMouse error text), reports arrive as a short burst of separate input reports; poll every 10 s at most. Read-only trigger "wake all" is a wake/telemetry request (I: harmless, but it wakes a sleeping mouse).
* Sources: https://unpkg.com/@openmouse/protocol@0.60.0/dist/finalmouse/index.js and drivers/finalmouse/hid.js
