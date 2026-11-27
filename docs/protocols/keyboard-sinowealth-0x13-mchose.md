# Keyboard family: SinoWealth "report 0x13" 20-byte family (MCHOSE G87/Z75S/G75 Pro/G98V2, VGN V108, Xinmeng/XINMENG M71, ...) and MCHOSE's newer 0x4D / 0x55 generations

Legend: **V** verified in third-party source (URL), **I** inferred, **U** unknown. Hardware-tested notes are from the cited projects.

## 1. Identification

Chip vendor: SinoWealth (`0x258A`) / Beken-ish 2.4 GHz SoCs; brand shells: MCHOSE (迈从), VGN, Xinmeng, Ajazz/Ikbc-like (unconfirmed), Dareu, Redragon-likes (unconfirmed). Same 20-byte vendor channel.

| VID:PID | Product | Role | Conf |
|---|---|---|---|
| 41E4:2001 | MCHOSE G87 2.4 GHz dongle ("old generation") | MI_01 / Col01, **usage page 0xFF02**, In/Out = 20 bytes, **report ID 0x13**; MI_01 Col06 (usage page 0xFF04) is a decoy "mailbox", do NOT use for battery | V (vigil-watt protocols.md section 1) |
| 258A:01BB | MCHOSE Z75S (2.4G); wired PID 01B9 | usage page 0xFF02 / usage 0x0002, 20/20/0 bytes | V (FerrisPulse MCHOSE_Z75S_ADAPTATION.md; EZ2.4G README) |
| 3554:FA09 | MCHOSE G75 PRO dongle (shared with KZZI K75: probe cmd 0x05 fingerprint first) | FF02/0002, 20/20 | V (FerrisPulse HARDWARE_PROTOCOLS.md 1.5, 1.9-batch #8) |
| 41E4:2004 | MCHOSE G98V2 | same 0x13/0x4A frame, model byte 0x18 | V (FerrisPulse batch #5) |
| 391D:2A02 | VGN V108 2.4G | FF02/0002 20 B, 0x13 family: `0x4A` battery (high nibble = charging), `0x05` model code 0x30 | V (FerrisPulse batch #3) |
| ?:? | Xinmeng M71 (and "XinMengK65Keyboard" in the web-driver catalog) | Linux RFC shows the same `0x13 0x0A 01 00 04 05 <pct> <flag>` push frames | V (lkml RFC 2506.2/09463) |
| 258A:010C | **different**: SinoWealth combo receiver (Weikav/Dareu/Redragon-likes) | no reachable battery channel found | V (FerrisPulse 1.9) |

(With a Windows report-ID byte the buffers below are exactly 20 bytes including the ID 0x13 at index 0; no extra leading 0.)

## 2. Frame and checksum (V)
`[0]=0x13 [1]=cmd [2]=param [3..18]=0 [19]= (sum of bytes 0..18) & 0xFF`. Replies use the same shape, checksum at [19].

## 3. Session + battery sequence (V; hardware-tested on G87 dongle 41E4:2001 and Z75S)
1. **Open session** (needed on the 41E4 dongle - silent otherwise): `13 05 01 00*16 19` (the 0x05 command is also "model token" query). ACK `13 05 01 00 0A 03 00 ...`. On Z75S the reply to cmd 0x05 carries the model token at idx 5..10 (`0A 00 00 00 00 09` for Z75S; last byte `01` = Z75) - use it to tell models apart on the shared 258A:01BB id.
   *A crashed client leaves the session open and the dongle then ignores everyone (including the official app); unplug/replug fixes it.* Only one client at a time.
2. **Battery query**: `13 4A 01 00*16 5E`  (G75 PRO uses param 00 -> `13 4A 00 ... 5D`; both accepted by their own dongles - send param 01 first, fall back to 00 if no answer: **I**).
3. Reply: `13 4A 01 00 02 <pct> <state> 00*12 <chk>`
   * `[5]` = battery percent, 0..100. **Reject <=1** (never real; firmware sends low placeholder values).
   * `[6]` = state: **high nibble 0x1 = charging**; low nibble 0x1 = "full/discharging marker". Observed: `01` discharging, `10` charging, `01` after unplug at 100 %, `00` also seen right after power-cycle (**U** meaning). Z75S samples: `55 00` = 85 %, `32 10` = 50 % charging, `64 01` = 100 % not charging.
4. Unsolicited push on the same interface: `13 0A 01 00 04 <type> <payload>`:
   * type `0x05` -> `[6]=percent, [7]=charge flag` (0x10 bit = charging) = battery push.
   * type `0x02` -> **not** battery: payload 0x00/0x01 alternates (link/awake state; a `00` appeared exactly at keyboard power-off, `01` at power-on). Treating `[6]` of a type-2 frame as battery produced bogus "1 %" readings. Always check `[5]`.
5. **Stale cache**: the dongle answers `4A` from a snapshot of the keyboard's last report. If the keyboard is idle/asleep the answer can be byte-identical for hours (e.g. "99 % charging" for 2 h after the charger was unplugged); pressing a key refreshes it. There is no host-side way to force it. (V vigil-watt section 1.3.1.)
6. No "connected" field exists in the protocol; "no reply" is the only offline signal.

## 4. Polling rate
No polling-rate field is documented for the 0x13 family. **U**. Use the USB-interval fallback for wired.

## 5. Model name
Use USB product string + the `0x05` model token (Z75S `0A0000000009`). For 41E4:2001 the OEM official table (MCHOSE HUB JS) maps 0x41E4 = Ace60/Ace68/Jet75/Mix87/Zero75 wired + old dongle (V - vigil-watt section 5, official device table inside M HUB main JS) - so the *dongle* cannot name the keyboard (**U**).

## 6. MCHOSE newer generations (VID 0x3837) - different protocols
These are not "0x13" but are the same vendor's web driver (M HUB WEB, www.mchose.com.cn) so the tray app must tell them apart by VID/PID:

### 6.1 QHW 0x4D frame (dongle 3837:3033 shared by G87 V2 / G75 V2 / K87S / K99 V3)  - V (reverse-engineered from the web app JS by vigil-watt; not hardware-tested on keyboards in that project, "implemented, reserve")
`[0]=0x4D [1]=ver 0x01 [2]=flags 0x03 [3]=payload length [4..5]=cmd (LE) [6]=bizCode 0 [7]=sequence [8..]=payload [8+len]=XOR checksum`. XOR range disagrees between sources (vigil: from [1]; FerrisPulse: from [2] through [7+len]) -> **U**, verify with a reply.
Handshake first: write `4D AA 55 AA 55 AA 55` style raw (no QHW header) once.
Commands: `0x0900` base info (reply payload: `[0..1]`=VID LE, `[2..3]`=PID LE, `[4]`=max profile, **`[10]`=connection 1 wired/2 wireless, `[11]`=charge 0 none/1 charging/2 full, `[12]`=battery %**); `0x0904` battery (**payload `[0]`=charge state 0/1/2, `[1]`=percent**); `0x0901` firmware version.
Must use WriteFile/ReadFile (interrupt) - feature reports are ignored. Reply payload's first byte 0xFF = busy: retry after ~90 ms (FerrisPulse).
Mice of the same generation carry polling rate in `0x0002` ("perf config", byte 9/10 high nibble = USB / 2.4G rate index; rates 125/500/1000/2000/4000/8000 Hz, index+1) - keyboards of this generation likely share it (**I**).

### 6.2 G87 V2 / K99 V3 "65-byte 0x55/0xAA" (3837:3033 2.4G, 3837:2020 wired) - V (FerrisPulse 1.6, community-verified on G87V2)
Collection usage page 1 / usage 0, **65-byte** buffer (report ID 0). Request: `00 55 05 00 38 38 ...` (zero padded to 65). Reply: `[0]=00 [1]=AA [2]=cmd echo [3]=00 [4]=checksum [5]=data length [9..]=data`; **battery+charge share byte [41]: bit7 = charging, bits0-6 = percent**; `0x80` alone = charging but not initialised (do not show 0 %). Example `0xC3` = 67 % charging, `0x44` = 68 % not charging.
Spontaneous dongle push seen on the "CZ" channel: `[FA, FB, 07, percent, charge]` (vigil-watt 2.3) - **I** (seen on a different generation).

### 6.3 BLE
K99 V3 etc. expose only standard GATT 0x180F; Windows node pair: `BTHLEDEVICE\{0000180F-...}_DEV_VID&0107D7_PID&EFFF...` has VID/PID but no battery; the parent `BTHLE\DEV_<MAC>` has `DEVPKEY_Device_BatteryLevel` (single BYTE). Details in keyboard-windows-generic-ble-and-fallbacks.md.

## 7. Safety notes
* Only send cmd 0x05 and 0x4A on the 0x13 family; never other command bytes (some change configuration/enter flashing).
* Only one client; the official MCHOSE HUB must be closed.
* Min interval ~5 s between queries; cache last valid value; show "stale" when keyboard asleep.

## Sources
* https://github.com/Mightling/vigil-watt/blob/main/docs/hardware/protocols.md (MIT; section 1, 2, 5)
* https://github.com/Ferris-echo/FerrisPulse/blob/main/docs/MCHOSE_Z75S_ADAPTATION.md and docs/HARDWARE_PROTOCOLS.md (GPL-3.0; facts only)
* https://github.com/Ap0LoG1zE/EZ2.4G/blob/main/c/PROTOCOLS.md (MIT; Z75S `258A:01BB` `0x13/0x4A`, `[5]` battery, last byte checksum)
* https://lkml.indiana.edu/hypermail/linux/kernel/2506.2/09463.html (Xinmeng M71 raw frames)
