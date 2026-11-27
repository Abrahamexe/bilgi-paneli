# ASTRO Gaming A50 (Gen 4 base station, Gen 5 base station)

Tags: **[V]** verified in source (URL), **[I]** inferred, **[U]** unknown.
Sources (facts only, GPL-3.0): https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/logitech_astro_a50_gen4.hpp , `logitech_astro_a50.hpp` (Gen 5), `docs/ASTRO_A50_EXTRA_FEATURES.md`. Gen 4 command codes are credited by HC to the MIT-licensed "eh-fifty" project (not re-read here). Gen 5 protocol credited to HC contributor's G HUB captures.

## 1. Models

| Model | VID | PID | Notes |
|---|---|---|---|
| A50 Gen 4 base (PlayStation/PC edition) | **0x9886** (Astro Gaming legacy VID) | **0x002c** | HC: verified on PS/PC edition only; Xbox/PC edition PID unknown **[U]**. Base mode switch must be on **PC**; in console mode it enumerates as 9886:002b with audio only and **no HID** **[V]**. Linux-verified only; Windows untested **[V]**. |
| A50 Gen 5 base ("A50 X"-era Logitech-era) | **0x046d** | **0x0b1c** | vendor HID interface 8, usage page 0xFF32, usage 0x0074; NOT HID++ and NOT Centurion **[V]** |
| A50 Gen 3 and older | 0x9886 | **[U]** | not in any source reviewed |

## 2. Gen 4 (9886:002c)

* Collection: **usage page 0xFF32, usage 0x0074; interface 6**; report ID **0x02**, 64-byte frames; strictly one reply per request, no unsolicited frames. **[V]**
* Request: `02 <CMD> [LEN PAYLOAD...]` zero-padded. Reply: `02 <STATUS> <LEN> <PAYLOAD...>` with STATUS 0x02 = OK with payload, 0x00 = accepted without payload, 0x01 = error (payload[0] = error code). LEN is unreliable (firmware sometimes wrong) so do not trust it. **Payload starts at byte[3].** **[V]**
* **Link check first**: command **0x54** -> payload[0] (byte[3]): bit0 = docked, **bit1 = live link to the headset**. The base **keeps serving the last known battery level after the headset powers off**, so *only trust the battery if bit1 is set*; otherwise report "headset off". **[V]**
* **Battery**: command **0x7C** (no args) -> **payload[0] (byte[3]): bit7 = charging, bits 0..6 = percent**. **[V]**
* After a timeout, wait briefly for the outstanding reply and discard it before the next request. **[V]**
* Settings are not persisted; irrelevant for monitoring. **[V]**

## 3. Gen 5 (046d:0b1c)

* Collection: vendor HID, **interface 8, usage page 0xFF32, usage 0x0074**, report ID **0x02**, 64-byte interrupt reports. 8 UAC interfaces also present. **[V]**
* Frame: `02 0C <LEN> 00 <CMD> <HANDLE> <payload...>`; LEN = 3 + payload length; HANDLE is an echo token (any value works, G HUB uses low nibble 0xC for GETs; replies come back with 0x00); little-endian payloads. The device also pushes frames unsolicited on state change. **[V]**
* **Battery**: GET `02 0C 03 00 06 0C` (CMD 0x06, no payload) -> reply `02 0C 06 00 06 00 <level%> <level2> <dock/charging>`: **byte[6] = percent (0..100)**, byte[7] = secondary level (meaning **[U]**), **byte[8] != 0 = docked/charging**. Match replies on CMD alone, not on the handle. **[V]**
* Headset-off state: **[U]** (HC does not decode one for Gen 5; the Gen 4 "link" concept likely exists as CMD 0x0e/"device state", mentioned as read-only state in `ASTRO_A50_EXTRA_FEATURES.md`, but byte layout not given). **[I]**

## 4. Quirks
* Gen 4 caches stale battery when headset is off - gate on link bit. **[V]**
* Gen 5 emits unsolicited frames; a poller must skip frames whose CMD != 0x06. **[V]**
* PlayStation-mode Gen 4 exposes no HID; the tray should show "switch base to PC" rather than "not found". **[V]/[I]**

## 5. Sources
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/logitech_astro_a50_gen4.hpp
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/logitech_astro_a50.hpp
* https://github.com/Sapd/HeadsetControl/blob/master/docs/ASTRO_A50_EXTRA_FEATURES.md
