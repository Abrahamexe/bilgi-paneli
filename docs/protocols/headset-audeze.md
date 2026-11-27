# Audeze Maxwell / Maxwell 2

VID **0x3329** (HC `VENDOR_AUDEZE`). Tags: **[V]** verified in source (URL), **[I]** inferred, **[U]** unknown.
Source (GPL-3.0, facts only): https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/audeze_maxwell_device.hpp , `audeze_maxwell.hpp`, `audeze_maxwell2.hpp`. HC says the protocol was "recovered from Audeze.exe and a USB capture of the original Maxwell dongle"; the dongle is Airoha-based.

## 1. Models

| Model | PID | Collection |
|---|---|---|
| Maxwell (PlayStation/PC dongle) | 0x4b19 | usage page **0xFF13**, usage 0x01, **interface 0** |
| Maxwell (Xbox dongle) | 0x4b18 | same as above |
| Maxwell 2 (PlayStation/PC) | 0x4b29 | usage page 0xFF13, usage 0x01, **interface 5** |
| Maxwell 2 (Xbox) | 0x4b28 | same as Maxwell 2 |

All **[V]**.

## 2. Transport **[V]**
* HID reports are **62 bytes**: **output report ID 0x06** (written normally) and **input report ID 0x07 which must be read with GET_REPORT / `HidD_GetInputReport`** (HC uses `hid_get_input_report`), not necessarily as an interrupt stream. Poll it.
* Report layout: `<report id> <len> <route> <fragment...>`: `len` = number of valid fragment bytes (0..59); `route` = **0x80 to reach the headset through the dongle**; ignore bytes beyond `len`; `len == 0` means "nothing new yet". A report may contain several messages and a message may span reports.
* Message: `05 <type> <len lo> <len hi> <body...>`; type **5A request, 5B response, 5C notification, 5D asynchronous result**; first two body bytes identify the command.
* Pacing: one request at a time, ~50 ms before each read; never resend writes (HC: not known safe).

## 3. Battery **[V]**
* Request body: `D6 0C 00`, i.e. message `05 5A 03 00 D6 0C 00`, sent as report `06 07 80 05 5A 03 00 D6 0C 00 ...` (my assembly of HC's description: length 7 fragment bytes, route 0x80) **[I]** on the exact padding.
* Replies: first an acknowledgement `05 5B 03 00 D6 0C <status>` (status 00 = ok); then the asynchronous result **`05 5D 05 00 D6 0C 00 00 <percent>`**: in message offsets, **byte[8] = battery percent (cap at 100)**, status offset 6.
* Keep reading until the 5D result (or a failed 5B status) arrives.
* **Headset off**: HC says the headset **does not answer while switched off** - the request simply times out (HC maps timeout to "battery unavailable"). **[V]**
* **Charging**: not exposed by HC for the Maxwell (status always "available"). **[U]**
* Mic-mute (boom) can be read as key 0x22 (`01 09 22 00` read) - 00 = muted. Not needed for battery. **[V]**
* Typical timeout budget: allow 1.5 s (HC EQ timeout constant), repeat the poll no more often than every 30-60 s. **[I]**

## 4. Quirks
* Replies carry no sequence number; match on command identity (`D6 0C`) and skip other queued messages. **[V]**
* Audeze HQ app running concurrently can write settings; reading battery does not conflict. **[I]**
* Maxwell (original) and Maxwell 2 share the protocol but use different HID interfaces (0 vs 5). **[V]**

## 5. Sources
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/audeze_maxwell_device.hpp
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/audeze_maxwell.hpp
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/audeze_maxwell2.hpp
