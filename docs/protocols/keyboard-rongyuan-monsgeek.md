# Keyboard family: RongYuan "MonsGeek / Akko web driver" protocol (VID 0x3151 and friends)

Confidence legend: **V** = verified in third-party source/docs (URL given), **I** = inferred, **U** = unknown.
All protocol facts below are restated in my own words from third-party reverse-engineering; no code copied.

## 1. Who uses it

One OEM (RongYuan, `rongyuan.tech`) ships the MCU/firmware + a shared web driver (WebHID) used under many brand names. The
catalog scraped from the web drivers (web.monsgeek.com / akkogear / womier) lists ~1127 devices. Brands that appear in it
(`company` field): MonsGeek, Akko, Womier, Epomaker (36 entries, e.g. HE65/HE68/HE75/G84 HE/RT100/RT75/Epomaker 65/TH80), Yunzii,
ikbc, Hator, Keydous, Gamakay, Noppoo, MageGee, EWEADNV (ZAP/Deep/Seek...), Cherry (MX8.2, K5 TMR, VID 0x046A), Ajazz (some), Attack Shark,
Hammerwolf, MSI (VID 0x0DB0), Valkyrie/VGN (VID 0x374A, `0xF7` family), Fullspirit/Fuling (VID 0x38A9). Also many mice (ignore).  
Source (V): https://github.com/echtzeit-solutions/monsgeek-akko-linux (`data/devices.json`, `devices_webapp.json`), analysed by me.

Chip generations visible in the catalog name prefixes (count in catalog): `ry5088` 541, `yc3123` 130, `yc3121` 119, `pan1086` 45, `ry6609` 42,
`ry6602` 40, `yc500` 21, `yc300`, `yc580`, `ry1086`, `ry5008`. Boards with `*_dm_*` = "direct matrix", `*_soc_*` = SoC variant, `_8k_` = 8 kHz capable, `_hall`/`he`/`tmr` = analog.
(V) Only RY5088 has a detailed public protocol description. YC3121/YC3123/YC500 (older 3-mode boards, PID 0x4015/0x5002/0x4010...) share the
same command numbering per the Linux project's comments (`CommonKBRY5088.js` stubs YC500-only commands) -> **I** that battery/polling work the same; **not verified on those chips**.

## 2. USB identifiers

| VID | PID | Role | Notes | Conf |
|---|---|---|---|---|
| 0x3151 | 0x5030 | RY5088 keyboard, wired USB (M1 V5 HE/TMR, FUN60 Max/Ultra, Epomaker HE, ...) | most common | V |
| 0x3151 | 0x5029 / 0x502D / 0x502F / 0x502C / 0x502E / 0x5054 / 0x5056 / 0x504C / 0x5033 | other RY5088/RY6602/RY6609 wired PIDs | seen in catalog | V (catalog) |
| 0x3151 | 0x4015 / 0x4035 / 0x4010 / 0x4018 / 0x5002 / 0x5025 | YC3121 / YC500 / YC3123 / PAN1086 era boards | older protocol generation | V (catalog) |
| 0x3151 | 0x5038 | 2.4 GHz dongle (M1 V5 HE) | | V |
| 0x3151 | 0x503A, 0x503D, 0x502B, 0x5006 | other dongles (legacy/TMR/Attack Shark K86) | | V (device_registry.rs) |
| 0x3151 | 0x5027 | Bluetooth (HOGP) node of M1 V5 HE | BLE variant, usage 0xFF55/0x0202, report ID 6 | V |
| 0x38EE | 0x0001/0x0009/0x0040/0x0046... | Akko/MonsGeek re-badged VID (Shine68, FUN60 Ultra TMR, M1 V5 TMR ISO) | same protocol | V (catalog) |
| 0x39AB | 0x9016 | MonsGeek/Akko FUN60 Pro JP | | V (catalog) |
| 0x379A | 0x1803/0x1805/0x1806 | Hator | | V (catalog) |
| 0x374A | 0xA2xx/0xA3xx | Valkyrie/VGN | | V (catalog) |
| 0x461 | 0x4002/0x4003 | Yunzii/AJAZZ (yzw/yc200/yc300 chips) | older | V (catalog) |
| 0x046A | 0x012D / 0x0141 | Cherry MX8.2 / K5 TMR | listed in the same driver | V (catalog) |

Discovery rule used by the Linux driver: accept any `0x3151` PID; for other VIDs accept only (VID,PID) pairs present in the catalog, because those VIDs
are shared with unrelated products.

Interfaces (wired RY5088): IF0 boot keyboard; IF1 multi-function (NKRO 0x01, mouse 0x02, consumer 0x03, **vendor input report 0x05**);
**IF2 vendor config, usage page 0xFFFF, usage 0x02, feature report ID 0, 64 bytes** (descriptor `06 FF FF 09 02 A1 01 ... 95 40 75 08 B1 02 C0`). (V)
Dongle: same layout; vendor config on IF2; IF1 is mis-advertised as boot keyboard but carries the vendor input reports (report 0x05).

## 3. Framing and checksum

* Transport: HID **feature reports**, report ID 0, 64 payload bytes (65 incl. ID on Windows). Request = `SetFeature`, then reply = `GetFeature` (report ID 0). Response must be re-read 2-3 times with 50-100 ms gaps (stale-buffer behaviour on hidraw; same advisable on Windows). (V)
* Request layout (after report ID): `[cmd, p1..p6, checksum, payload...]`.
* **Checksum "Bit7"**: sum of bytes 0..6 (cmd + 6 params) mod 256 -> `checksum = 255 - (sum & 0xFF)` stored at index 7. Example `F7 00 00 00 00 00 00 08`. (V)
* "Bit8" variant (LED commands only, checksum at index 8 over bytes 0..7). Irrelevant for read-only use.
* Replies echo the command byte at index 0 for 0x80-0xE6 GET commands; `0xAA` in byte 1 on some success replies. Exceptions without echo: `GET_MULTI_MAGNETISM (0xE5)` and `GET_DONGLE_STATUS (0xF7)` (V).
* BLE framing differs (report ID 6, 0x55 marker, checksum from cmd byte) and **vendor commands are ignored over BLE**; use the standard GATT battery service instead (see keyboard-windows-generic-ble-and-fallbacks.md). (V)

## 4. Battery

### 4.1 2.4 GHz dongle (best-documented path)  - V
Request: feature report ID 0: `F7 00 00 00 00 00 00 08 ...` (command 0xF7 = GET_DONGLE_STATUS, **handled locally by the dongle, no RF traffic**, does not wake the keyboard).
Reply (`GetFeature`, report ID 0; indices below are *after* stripping the report-ID byte):

| idx | meaning |
|---|---|
| 0 | has_cached_response flag (1 = a forwarded command's reply is waiting; fetch with 0xFC) |
| 1 | **keyboard battery %, 0-100** (taken from RF packets; 0 right after dongle power-up until the keyboard talks) |
| 2 | 0 (reserved) |
| 3 | charging flag 0/1 (set from keyboard RF event) - **see quirk** |
| 4 | constant 1 |
| 5 | rf_ready (0 = waiting for keyboard, 1 = idle; forced 1 while charging) |
| 6 | constant 1 (dongle alive) |
| 7 | pairing_mode |
| 8 | paired (1 = linked to a keyboard) |

Sources (V): `docs/PROTOCOL.md` section 2.2, `iot_driver_linux/monsgeek-transport/src/command.rs` (`DongleStatusResponse`, `hid_dongle.rs` `query_dongle_status`) in https://github.com/echtzeit-solutions/monsgeek-akko-linux
Quirk: the same project's eBPF battery bridge comments that on stock firmware the F7 bytes were *identical with and without a charger*, i.e. the charging byte may not be trustworthy; their README says "charging status not available". Treat charging from F7 as **U/unreliable**; the async event below is the better source.
Quirk: battery value is a cached snapshot; during keyboard deep-idle it can stay stale for hours (the dongle only learns on keyboard wake). Documented for a different vendor (Sinowealth 0x13 family) but same architecture - **I**.

### 4.2 Async vendor input event (dongle and wired) - V
Input report **ID 0x05** on IF1: `05 88 00 00 <level> <flags>`; `flags` bit0 = online, bit1 = charging. Sent by the dongle when the keyboard reports; not triggered by F7. (docs/PROTOCOL.md 6.5; firmware analysis `firmwares/2949-v407/CHARGING.md`.) On the stock keyboard-wired path charging is *not* propagated (firmware only forwards it on the RF/BT event path). A patched-firmware variant exposes a standard HID battery (usage page 0x06/usage 0x20, report ID 7) - not stock, ignore.

### 4.3 Wired USB keyboard - partly V
Firmware RE says a wired board answers the same `F7` request with the battery level in a feature report (level at response byte 1) but **no charging flag** (CHARGING.md). The Linux driver itself does not read it (returns a constant 100 for wired). Exact reply layout on a *wired board* (vs dongle) is **I** - verify on hardware; treat "battery not meaningful while on USB" as an acceptable fallback.

### 4.4 Bluetooth
Standard GATT Battery Service 0x180F/0x2A19 only (vendor channel is non-functional). (V)

## 5. Polling rate (RY5088 only) - V
Request: command **0x83** (`GET_REPORT`), `83 00 00 00 00 00 00 7C` (checksum = 255-0x83 = 0x7C). Reply: echo `0x83` at idx 0, byte idx 1 reserved (0), **rate code at idx 2**. Reading idx 1 makes every rate look like code 0 (documented bug).

| code | Hz |
|---|---|
| 0 | 8000 |
| 1 | 4000 |
| 2 | 2000 |
| 3 | 1000 |
| 4 | 500 |
| 5 | 250 |
| 6 | 125 |

Set = command 0x03 with payload `[0, code]` (not needed). Not available on YC3121/YC500-era (those have no 8K): **U** whether they answer 0x83.
Cross-check: the AULA "AA" family stores polling in a settings blob with a *different* code table (3=1K,4=2K,5=4K,6=8K) - do not mix families (see keyboard-aula-compx-aa.md).

## 6. Model name behind a dongle - V (format) / I (lookup)
1. Send `8F` (GET_USB_VERSION) - **forwarded over RF** to the keyboard; then send `FC` (GET_CACHED_RESPONSE) and read; allow ~220 ms RF round-trip; one cache slot only, so read promptly.  
2. Reply: idx0 = 0x8F, **idx1..4 = device ID (u32 LE)**, idx7..8 = firmware version (u16 LE).  
3. Device ID == the `id` field of the web-driver device catalog (e.g. 1427 = BK98S). Dongle-local helpers: `F0` GET_DONGLE_INFO (`F0,1,8,0,0,0,0,fw`), `FD` GET_DONGLE_ID (`AA 55 01 00`), `FB` GET_RF_INFO.
The Linux project states identity is decided from this query rather than USB PID (V). Mapping ID->marketing name via an embedded copy of the catalog is **I** (catalog scraped from the vendor sites).
Wired boards: send `8F` directly (no FC needed).

## 7. Quirks / safety
* Never send 0x7F (bootloader, needs 55AA55AA magic, triggers erase) or 0xF8 (RF programming mode). Stay to F7/8F/FC/83/F0/FD.
* Dongle holds one 64-byte cache slot: don't interleave queries from two clients (official web driver also talks to the same interface - close its tab).
* All timing: dongle-local queries ~immediate; forwarded ones ~220 ms.
* Dual MCU boards: USB-power ADC lowers the reported % by up to ~18% while on USB (V, HARDWARE.md) - don't trust battery % while the cable is attached.

## 8. Sources
* https://github.com/echtzeit-solutions/monsgeek-akko-linux (docs/PROTOCOL.md, docs/HARDWARE.md, firmwares/2949-v407/CHARGING.md, iot_driver_linux/*, data/devices.json)
* https://github.com/Tricked-dev/monsgeek-js-driver (mentioned only; not read)
* https://github.com/strodgers/epomaker-controller (Epomaker RT100: sends `f7` poll on 2.4 GHz - confirms Epomaker uses this family)
* FerrisPulse notes (VK/Valkyrie 374A:A272 returns `00 00 <batt> 00 00 00 01 01` to F7; "baseOffset" quirk): https://github.com/Ferris-echo/FerrisPulse/blob/main/docs/HARDWARE_PROTOCOLS.md (section 1.4) - V (third-party doc)
