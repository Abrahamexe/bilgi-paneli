# Family B: CompX "XVI" 64-byte page/command feature-report protocol (WLMouse, Lamzu Maya X / Inca / Paro / Aurora, Attack Shark R5 Ultra/R6/R8, LunaFury, CRDRAKO, Vancer, RAWM, G-Wolves XVI:1, Hitscan Hyperlight)

Second CompX firmware stack. Unnumbered 64-byte **feature report ID 0**, request/poll style. Distinct from Family A (report 8 / 0x55 checksum). Same ODM: OpenMouse notes the framing is "shared by WLMouse and Lamzu receivers"; G-Wolves' and Hitscan's own web drivers use the same frame.

Confidence: **V** verified in source, **HW** hardware-confirmed per source, **I** inferred, **U** unknown.

## 1. Brands and IDs

| Brand | Models | VID | PIDs | Evidence |
|---|---|---|---|---|
| WLMouse | Beast G a860/a861, Huan a863/a864, Beast Miao a866/a867, Beast Mini Pro a868/a869, Beast X Pro a870/a871, Strider a872/a873, Ying a874/a875, Sword X a878/a879, Beast Max a880/a881, Beast X a883/a884, Beast Mini a885/a886, 1K receiver a882 (in each pair the first PID is the wireless/receiver entry, the second the wired mouse) | 0x36a7 | the shared 1K receiver a882 reports the paired mouse via cmd 0x8b | V, HW (receiver) |
| WLMouse Beast X 4K | 36a7:a887 | 0x36a7 | **different protocol, see section 8** | V |
| Lamzu (CompX generation 2) | Maya X (wired 001c, 1K RX 001d, 8K RX 001e) | 0x373e | | V |
| Lamzu own VID | Inca 8K (0009 wired, 000f 1K RX, 0010 8K RX), Paro Aurora (0007, 000d, 000e), shared "Aurora 8K receiver" 0001, "Aurora 1K receiver" 0003 | 0x37b0 | collection usagePage 0xFFFF | V, HW (Inca 0009/0010, receiver 0001) |
| Attack Shark (Lamzu OEM) | R5 Ultra 0046/0047(8K RX), R6 0021/0022, R8 003a/003b | 0x373e | | V (R5 Ultra HW) |
| LunaFury | LUNA33 0032/0033, TYPE33 0054/0084 | 0x373e | | V |
| CRDRAKO | KO-ONE 006a (wired) / 006b (RX) | 0x373e | wired mouse target 0x00 | V |
| Vancer (Arbiter Studio) | Akitsu Medium 3950 0066/0067 | 0x373e | | V, HW |
| RAWM | Leviathan V4 GT 0098/0099 | 0x373e | | V (untested) |
| G-Wolves XVI:1 | HTX Mini 8K, Fenrir (3508 wired/3517 RX HW), HTM Plus 3808/3817, HSK Pro 2.0 6808/6817, HTXU 5608/5617, Fenrir Pro 3608/3617, VUK 3908/3917, HT-S2 7904/7913, HTS Ultra 5308/5317, HTR 7704/7713, HT-S2 Pro 7908/7917, Fenir Max 3708/3717 | 0x33e4 | | V (Fenrir HW) |
| Hitscan | Hyperlight X 0136, Hyperlight N 0194 (wired), shared receiver 0300 | 0x3770 | | V (unverified on HW) |

WebHID filter used by OpenMouse: whole VID (0x373e, 0x36a7) or VID+usagePage 0xFFFF (Inca). The G-Wolves web driver finds the collection by **shape** (unnumbered 64-byte feature report) not by usage page. Under Win32: pick the collection whose FeatureReportByteLength is 65 and which has no report IDs; several collections may exist, drivers "reject interfaces lacking the feature-report-0 control channel".

## 2. Frame

Feature report, report ID 0 (Windows buffers have a leading 0x00 byte, so 65 bytes). Request body:

```
[0] 0x00 status (request)
[1] 0x00
[2] target   0x00 dongle itself, 0x01 receiver's pairing memory, 0x02 mouse (wired CRDRAKO/LunaFury use 0x00 for the mouse)
[3] length   (data length expected / sent)
[4] page / class   0 device, 1 profile-sensor, 2 dongle, 3 buttons, 4 macro
[5] command   (reads = write command | 0x80)
[6..] args
```
No checksum.

Send with `HidD_SetFeature`, wait ~30 ms (up to 20-30 ms polls, up to 5 sends), read with `HidD_GetFeature` of the same report. Reply uses the same layout:
`[0]` status: 0xA1 OK, 0xA0 pending, 0xA2 unsupported command, 0xA3 busy, 0x02 mouse asleep (G-Wolves/Hitscan). `[2]` target, `[3]` payload length, `[4]/[5]` echo page/command, payload from `[6]`.
Windows may include the report ID in the returned buffer; WLMouse's driver tries frame offsets 0 and 1 and matches on page+command echo. Always match `[4],[5]` against the request (stale answers from the previous request are common; Rapoo-like lossy behaviour is not reported here). V, HW

## 3. Battery (read-only)

Request: target=2, length=2, page=0, command=0x83:

`00 00 02 02 00 83 00 ...` (64 bytes)

Reply payload (from [6]): `[0]` charging (1 = charging, 0 = not), `[1]` percent (0..100, reject >100).
Same on WLMouse, Lamzu/Inca/Maya, Attack Shark R5, Hitscan XVI (decodeBattery requires charging in {0,1}). V, HW

G-Wolves XVI "legacy" layout (older XVI:1 models) uses a different sub-layout: request `[0, len=2, cmd=0x8F, wireless(0/1)]`, reply `[4]` charging, `[5]` percent. V, HW (HTX Mini 8K receiver)

Passive alternative: input report **ID 4** notifications from the mouse/receiver. `type 0x03` = battery (data[1] percent, data[2] charging), 0x06 = sleep/wake (data[1] 0 sleep, 1 wake), 0x08 other. Needs a second handle on the interface that declares input report 4. V (Hitscan XVI decoder; WLMouse/Lamzu NOTIFY_KINDS {03,06,08})

## 4. Polling rate

Request: target=2, length=2, page=1, command=0x80, arg0 = profile (1-based; get active profile with page 0 cmd 0x85 length 1). Reply payload `[1]` = code.

| Code | 0x08 | 0x04 | 0x02 | 0x01 | 0x10 | 0x20 | 0x40 | 0x80 |
|---|---|---|---|---|---|---|---|---|
| WLMouse / Lamzu table | 125 | 250 | 500 | 1000 | 1000 (8K-family alias) | 2000 | 4000 | 8000 |

G-Wolves XVI and Hitscan XVI tables: 8=125, 4=250, 2=500, 1=1000, 32=2000, 64=4000, 128=8000, 16=1000. Same numbers as WLMouse (0x20=32 etc.). Inca: the cable answered 0x01 and the 8K receiver 0x40 for the same setting, so rate lists are per-PID (wired 125-1000; 8K RX 500-8000). The value is the stored (configured) profile rate. V, HW

## 5. Real model name

* Receivers share one PID. Ask the receiver which mouse is paired: target=1, page=0, command=0x8B, length=6, arg [0x02]; payload bytes [4..5] = paired product id (big-endian) -> look up in the brand table (WLMouse list above; Aurora table for Lamzu). V, HW
* Hitscan: `GetDevicePID` (device id 1, trailing 2). G-Wolves: model id (MID) from env-models.json.
* Firmware: page 0 cmd 0x81 length 0x10 -> payload `[2].[3]` (WLMouse) ; dongle firmware with target 0.
* Serial: page 0 cmd 0x82 length 2.

## 6. Quirks

* Mouse asleep: reply never arrives or status 0x02; WLMouse retries 12x with 30 ms then 300 ms "wake" waits. Do not hammer; poll every 30-60 s.
* Receiver (wireless) is slower than cable; use the first answer whose page/command match.
* G-Wolves "XVI:0" (HTX Ultra, 0x33e4:5618/3854) is **not** this family; it is Family A.
* Writes through a receiver 0.4-0.5 s apart froze a Delux link (other family); keep requests serial with a queue.
* WLMouse configurator and Lamzu Aurora/Attack Shark Core use the same collection; vendor apps open it exclusively on some systems (I), expect occasional HidD_GetFeature failures.
* "Lamzu Atlantis generation" (0x3554) is Family A, not this one; the Lamzu Maya X / Inca / Paro Aurora are this one.

## 7. Web configurators

Lamzu Aurora (https://lamzu.com/blogs/faq/what-is-aurora), G-Wolves https://mouse.xyz / https://mouse.fit (env-models.json), LunaFury https://mouse.lunafury.games, Arbiter https://webapp.arbiterstudio.com, WLMouse web driver (URL not found, product pages say web based: https://wlmouse.com/products/x-pro). I did not dump these bundles myself; facts come from OpenMouse's reverse engineering of them.

## 8. WLMouse Beast X 4K (36a7:a887), different

Output/input **report 4**, 63-byte body: `[0..1]` CRC-16/MODBUS (poly 0xA001 reflected, init 0xFFFF) over bytes `[2..30]` little-endian, `[2]` command (1 open session, 2 close, 5 read settings, 6 write), `[3]` length (<=24), `[4..5]` address LE, `[6]` status (0xFE/0xFF error), `[7..30]` data. Polling index at settings offset 0x0B into [125,250,500,1000,2000,4000]. **Battery location unknown (OpenMouse leaves it null).** Needs a USB capture. U

## 9. Sources

* OpenMouse protocol 0.60.0: https://unpkg.com/@openmouse/protocol@0.60.0/dist/ (compx/codec.js, wlmouse/index.js, drivers/wlmouse/hid.js, lamzu/index.js, drivers/lamzu/hid.js, hitscan/xvi.js, drivers/gwolves/xvi-hid.js, wlmouse/beast-x-4k.js)
* OpenMouse repo: https://github.com/OpenMouse-Project/openmouse
