# Turtle Beach / ROCCAT (Stealth, Elo 7.1 Air)

Tags: **[V]** verified in source (URL), **[I]** inferred, **[U]** unknown.

Honest summary: **no open-source or publicly documented HID battery protocol was found** for any Turtle Beach or ROCCAT wireless headset. Do not invent one.

## 1. ROCCAT Elo 7.1 Air (VID 0x1e7d, PID 0x3a37) **[V]**
* HeadsetControl supports only **lights and inactive time** for it; **no battery** capability: https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/roccat_elo_7_1_air.hpp
* Transport facts (useful for probing): 64-byte packets, device "hates quick packets" (>= 75 ms between commands), command byte[0] = 0xff for LED group, 0xa1 for the sleep-timer set. **[V]** The battery query, if any, is **[U]**.
* Elo 7.1 USB (0x1e7d:0x3a34): wired, 16-byte packets, 40 ms delay, lights only. **[V]**

## 2. Turtle Beach Stealth 600 (Gen 1/2/3), Stealth 700, Stealth Pro, Stealth 500, Stealth Ultra / Stealth 700 Gen 3 / Atlas
* Current Turtle Beach dongles appear to use VID **0x10f5** (Stealth Ultra dongle 10f5:7070 in a pending xpad patch: https://github.com/paroj/xpad/pull/364) - **[V] for that one dongle only**; the VID/PID of the Stealth 600/700/Pro/500 dongles are **[U]**.
* Battery over USB-HID: **[U]**. No HeadsetControl class, no kernel driver, no blog found. Turtle Beach's Swarm II / Audio Hub software shows battery; an USBPcap capture is the only way forward.
* Many TB headsets also pair over **Bluetooth** (Stealth 600 Gen 2 Max, Stealth 700 Gen 2/3 dual transmitters) - use `headset-bluetooth-generic.md`.

## 3. How to extend
1. Plug in the dongle; list HID collections (`HidD_GetAttributes`, `HidP_GetCaps` per path) and note vendor usage pages (0xFF00-0xFFFF).
2. Run vendor app (Swarm II / ROCCAT Swarm) + USBPcap; filter on the dongle; look for output reports whose reply byte changes with charge.
3. Add the discovered frame to this doc with a [V] tag and source.

## 4. Sources
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/roccat_elo_7_1_air.hpp , `roccat_elo_7_1_usb.hpp`
* https://github.com/paroj/xpad/pull/364 (Stealth Ultra dongle 10f5:7070, controller input only)
