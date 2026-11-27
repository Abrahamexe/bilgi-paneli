# MCHOSE (M HUB) protocols: A7 V2 "inverted feature report" and A7 V3 "report 0x4D"

Confidence: V in OpenMouse (reverse-engineered from MCHOSE's M HUB web bundle); V2 verified on an A7 V2 Ultra+ via 2.4G receiver; V3 **decoders** verified on an A7 V3 Ultra+, encoders untested.

* VIDs: 0x3837 (modern), 0x5253 "RealTek" (older: L7, A7 Ultra; PIDs 0x1020 8K RX, 0x1011 A7 Ultra cable), A5 Gen1 has its own VID (see OpenMouse a5-gen1, not decoded here). Config collection: usagePage 0xFF01 usage 0x0001. MagDock separate usage page (RGB only).
* Wired mouse PID = model id; receivers share one PID, so the real model is read from the battery reply (`productId` inside).

## V2 (A7 V2 Pro 4018, Pro+ 4023, Ultra 4019, Ultra+ 4021, A7 Ultra 1011, L7 Pro 00b0, L7 Pro+ 4015)
* Feature report **0x11** (20-token short) / **0x12** (64-token long). **Every command byte and argument is bit-inverted (XOR 0xFF)**; unspecified tail stays 0. Send `[~cmd, ~arg0, ...]` padded with zeros, wait ~, then `receiveFeatureReport(0x11)`; reply `[reportId, ~cmd, ~payload0, ...]`. Poll repeatedly: replies are stale/empty until the firmware has answered; accept when the un-inverted command echo matches **and** two consecutive answers are identical.
* **Battery**: cmd 0x06 on report 0x11. Request body: `[0xF9, 0xFF x19]`. Decoded payload (after un-inverting): `[0..1]` VID LE, `[2..3]` PID LE (**model id**), `[4..7]` firmware, `[8]` flags (bits 7-5 connect mode, bit 4 connected), `[9]` **percent**, `[10]` **charging (nonzero)**. Needs >=11 payload bytes.
* **Polling**: not a standalone command. Read whole config blob: cmd 0x67 on report 0x12 (64 tokens). Payload `[1]` = wired link byte, `[2]` = wireless link byte; **rate index = high nibble**, DPI stage = low nibble. Index is a plain index into the model's list: max 1000 -> [125,500,1000]; max 4000 -> [125,500,1000,2000,4000]; max 8000 -> [125,500,1000,2000,4000,8000]; Bluetooth hosts limited to 1000. Active link decides which byte applies (wired if PID is a model PID).
* Receiver identity cmd 0x03 (bond, VID/PID, connected), version cmd 0x04 (length-prefixed ASCII).

## V3 (A5 V3 Pro 4035, A5 V3 Ultra+ 4026/4034, K7 V2 4027/4028, A7 V3 4030-4033, K5 4037/4038, R7 Ultra 4036, V7 402a, G3 V3 4029)
* **Output report 0x4D**, 63-byte body: `[0]=0x01 version, [1]=flags (1 = checksum present), [2]=data length, [3..4]=command LE, [5]=biz code, [6]=sequence, [7..]=data, then checksum = XOR of body[1]..body[6+len]`. Replies arrive as input report 0x4D with the same layout.
* **Battery**: command 0x0900 (readDeviceInfo): data `[0..1]` VID, `[2..3]` PID (model), `[4]` profile count, `[10]` connect status, `[11]` charge status, **`[12]` battery percent**, `[13]` game mode. Request body bytes: `01 01 00 00 09 00 00 <chk>` with chk = XOR(01,00,00,09,00,00)=0x08, i.e. `01 01 00 00 09 00 00 08 ...`.
* **Polling**: command 0x0002 readSettings: data `[1]` wired byte, `[2]` wireless byte, rate index in high nibble, but the firmware table has a hidden slot: option = idx-1 when idx>1 (device idx 0 -> option 0, 1 -> 1, 2 -> 1, 3 -> 2, ...). Option list as V2 (max 1000 or 8000).

* Quirks: shared reply buffer between commands (stale answers); the V3 checksum is optional on replies; vendor says old firmware needed for some features.
* Sources: https://unpkg.com/@openmouse/protocol@0.60.0/dist/mchose/index.js, v3.js, drivers/mchose/hid.js
