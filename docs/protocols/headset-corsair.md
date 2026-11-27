# Corsair headsets (Void family, Virtuoso, HS series, Void Wireless V2, Virtuoso MAX, HS80 MAX)

VID **0x1b1c** for all. Tags: **[V]** verified in source (URL), **[I]** inferred, **[U]** unknown.
Sources (facts only, GPL-3.0): https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/ - `corsair_void_rich.hpp`, `corsair_device.hpp`, `corsair_void_v2w.hpp`, `corsair_virtuoso_xt.hpp`; guide `docs/ADDING_A_CORSAIR_DEVICE.md`. The Linux kernel also has `drivers/hid/hid-corsair-void.c` for the Void Wireless (not re-read this session).

Three dialects:
1. **Legacy Void protocol** (2-byte request `C9 64`, 5-byte reply) - Void / Void Pro / Void Elite family.
2. **Bragi property protocol, unnumbered frame** (65-byte writes, report ID 0) - Void Wireless V2, Virtuoso MAX, HS80 MAX.
3. **Bragi property protocol, report ID 0x02** - Virtuoso XT / SE.

## 1. Legacy Void dialect **[V]**

PIDs (HC `corsair_void_rich.hpp`, 18 entries; names are NOT given per PID in HC, so I do not map them): `0x1b1c`(sic, as listed), `0x1b27`, `0x0a14`, `0x0a16`, `0x0a17`, `0x0a1d`, `0x0a1a`, `0x1b2a`, `0x1b23`, `0x1b29`, `0x0a55`, `0x0a51`, `0x0a52`, `0x0a38`, `0x0a4f`, `0x0a2b`, `0x0a75`, `0x0a56`. HC's own guide names 0x1b27 = VOID PRO, 0x1b2a = VOID PRO Wireless, 0x1b23 = VOID USB. Other Void-family names (Void Elite Wireless, HS70, Void RGB Elite Wireless, etc.) **[U]** per PID. For a quick user-side check: `lsusb`/Device Manager "Hardware Ids".

* Collection: **usage page 0xFFC5, usage 0x01, interface 3** (battery, mic status, lights, notification sound); sidetone uses usage page 0xFF00 / interface 0. **[V]**
* Request: write `C9 64` (2 bytes). **[V]** (`corsair_device.hpp: requestBatteryCorsair`) **[I]** `C9` is the output report ID and `64` the command.
* Reply: 5 bytes `64 00 <bat> B1 <status>`:
  * byte[2] = battery; **bit 7 = "microphone boom up" flag**, bits 0..6 = percent 0..100 (mask with 0x7F). **[V]**
  * byte[4] = status: **0 = headset not connected to receiver (dongle-only)**, **1 = normal**, **2 = low battery**, **4 or 5 = charging**; other = unknown. **[V]**
  * bytes[0]=100, [1]=0, [3]=177 are constants. **[V]**
* Wired Void (e.g. 0x1b23 USB) reports a status too; whether it ever returns 4/5 on USB power without a battery **[U]**.

## 2. Void Wireless V2 / Virtuoso MAX Wireless / HS80 MAX Wireless - Bragi, unnumbered **[V]**

| Product | Receiver PID |
|---|---|
| Void Wireless V2 | 0x2a08 |
| Virtuoso MAX Wireless | 0x2a02 |
| HS80 MAX Wireless | 0x0a97 |

* Collection: **interface 4**, no usage-page hint in HC (usage 0/0). **[V]**
* Writes are **65 bytes: `00` (report ID 0) + 64 bytes**; reads are 64 bytes. Layout of a request after the report ID: `02 <endpoint> <op> <property> [00 <LE value>]` with endpoint **0x08 = the receiver itself**, **0x09 = the headset**; op **0x02 = GET**, 0x01 = SET. **[V]**
* Wake / handshake (battery-only, does NOT switch to software mode, avoids an audible pop): 
  1. GET receiver firmware: `00 02 08 02 13`
  2. Receiver heartbeat: `00 02 08 02 12`; flush the input queue (5 ms read loop)
  3. Headset heartbeat: `00 02 09 02 12`; **read one reply - if no reply, the headset is offline** (HC returns "not connected").
  **[V]**
* Battery: after flush, GET property 0x0f on the headset: `00 02 09 02 0f`. Reply bytes **[4] | ([5] << 8) = battery in 0.1 % units (0..1000); percent = value / 10**. The receiver sometimes answers with the paired-headset ID instead of a level, so **re-request up to 3 times while the value is 0 or > 1000**; still invalid -> unavailable. **[V]**
* Mic-mute (boom up): GET property 0xa6 on the headset: `00 02 09 02 a6`; reply byte[4] == 1 = muted. **[V]**
* Charging: **not decoded by HC in this dialect** **[V]**; the Virtuoso XT dialect below has a charge-state property (0x10) - **[I]** the same property id may exist here, untested.
* Accuracy: HC header says battery is "reliable up to an approximate 10 % error". **[V]**

## 3. Virtuoso XT / SE (Bragi, report ID 0x02) **[V]**

| Product | Wireless receiver PID | Wired PID |
|---|---|---|
| Virtuoso XT | 0x0a64 | 0x0a62 |
| Virtuoso SE | 0x0a3e | 0x0a3d |

* Collection: **interface 3, usage page 0xFF42, usage 0x0001**. Plugging the USB-C cable into the headset re-enumerates it as the *wired* PID and the receiver then reports no headset attached. **[V]**
* Request (64 bytes, report ID 0x02): `02 <target> <op> <property> 00 <LE value>`; **target 0x09 = headset behind a receiver, 0x08 = this device itself** (wired); op 0x02 = GET, 0x01 = SET. Wrong target is not reported consistently (XT stays silent). **[V]**
* Reply (64 bytes, report ID 0x01): `01 <src: 01 = relayed from headset, 00 = from this device> <cmd echo> <status: 00 ok, 05 no such property, 09 write refused> <LE value...>`. Skip unsolicited report **0x0e** (volume events). **[V]**
* **Battery level property 0x0f** -> LE u16 in 0.1 % units, percent = value / 10. **Battery status property 0x10 -> 1 = charging.** **[V]**
* No writes are needed for battery; writes require software mode (property 0x03 = 2) - do not enter it from a tray app.

## 4. HS55 Wireless / HS80 RGB Wireless / HS70 / Virtuoso RGB Wireless / Void Elite Wireless
* PIDs and protocol dialect: **[U]**. HC README/guide says "Virtuoso and HS series use different protocols"; HS80, HS70 wired, RGB Elite and Virtuoso other than the XT expose sidetone through ALSA (Linux) rather than HID. OpenRGB lists Virtuoso RGB Wireless 1b1c:0a42 (https://gitlab.com/CalcProgrammer1/OpenRGB/-/issues/1620) and a kernel thread shows Virtuoso SE as 1b1c:0a40 in `lsusb` (https://www.lkml.org/lkml/2022/3/25/637) - **[I]** these are different from the PIDs HC lists (0x0a3e/0x0a3d), suggesting multiple hardware revisions; not verified.
* Best bet for HS80 RGB Wireless / HS55 Wireless: they probably speak Bragi like the V2/XT dialects (`02 08/09 02 <prop>`) **[I]**; capture iCUE traffic or probe interfaces before shipping.

## 5. Quirks
* Void legacy: bit 7 of the battery byte is the mic-up flag; forgetting the mask yields 128+ percent. **[V]**
* V2W receivers answer with garbage until the heartbeat dance is done; always flush the input buffer first. **[V]**
* iCUE running in parallel can hold software mode; battery reads in V2W/XT survive because they only use GET + heartbeat. **[I]**

## 6. Sources
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/corsair_void_rich.hpp
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/corsair_device.hpp
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/corsair_void_v2w.hpp
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/corsair_virtuoso_xt.hpp
* https://github.com/Sapd/HeadsetControl/blob/master/docs/ADDING_A_CORSAIR_DEVICE.md
