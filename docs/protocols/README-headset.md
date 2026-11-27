# Headset battery protocols - index and summary

Scope: battery %, charging state, "headset off / dongle only" state for 2.4 GHz dongle headsets (raw Windows HID, no vendor software) and Bluetooth headsets (generic Windows path).
Written 2026-11-24. Facts extracted from HeadsetControl (GPL-3.0; **no code copied**, own words), HyperHeadset (MIT), kernel mailing lists, blogs, Microsoft docs, plus local tests on a Windows 11 box.
Tags in every file: **[V]** verified in a cited source, **[I]** inferred, **[U]** unknown, **[L]** verified locally.

## Files
| File | Contents |
|---|---|
| `headset-steelseries.md` | Arctis 1/7/7+/9/Pro Wireless/Nova 3P/5/7/Pro Wireless/GameBuds |
| `headset-hyperx.md` | Cloud II / II Core / III / III S / Flight / Alpha / Alpha 2 / Stinger 2 (Kingston vs HP VID/PID) |
| `headset-logitech.md` | G533, G535, G633/733/933/935, G PRO / PRO X (HID++ voltage), PRO X 2 / PRO X 3 / G522 (Centurion) |
| `headset-astro.md` | A50 Gen 4 and Gen 5 |
| `headset-corsair.md` | Void (legacy), Void Wireless V2 / Virtuoso MAX / HS80 MAX (Bragi), Virtuoso XT/SE |
| `headset-razer.md` | BlackShark V2 Pro PS (documented), others unknown |
| `headset-turtlebeach-roccat.md` | nothing public found (honest gap) |
| `headset-audeze.md` | Maxwell / Maxwell 2 |
| `headset-sony-inzone.md` | INZONE H5 / H9 II / Buds, PULSE 3D |
| `headset-epos-sennheiser.md` | EPOS H3PRO Hybrid, Sennheiser GSP 370 (serial), Jabra GNP |
| `headset-other-2g4.md` | Fifine HT6, MCHOSE X9/V9 Pro, Lenovo VoIP, BT600; ASUS / JBL / Cooler Master / Attack Shark (unknown) |
| `headset-bluetooth-generic.md` | HFP battery property vs BLE Battery Service, non-admin Win32/WinRT recipes |

## Summary table
Legend: Batt = battery % readable; Chg = charging flag; Off = in-band "headset off" state; Eff = effort (L low, M medium, H high).

| Brand / model | VID:PID (dongle) | Collection (usage page:usage / iface) | Batt | Chg | Off | Confidence | Eff |
|---|---|---|---|---|---|---|---|
| SteelSeries Arctis 1 / 7X / 7P | 1038:12b3,12b6,12d7,12d5 | FF43:0202 / 3 | yes (0..100) | no | yes | V | L |
| SteelSeries Arctis 7 / 7(2019) / Pro | 1038:1260,12ad,1252,1280 | FF43:0202 / 5 | yes, unreliable on 2019 | no | no | V (quirk) | L |
| SteelSeries Arctis 9 | 1038:12c2,12c4 | - / 0 | yes (scaled) | yes | weak | V | L |
| SteelSeries Arctis Pro Wireless | 1038:1290 | - / 0 | 5 steps | no | yes | V | L |
| SteelSeries Arctis 7+ | 1038:220e,2212,2216,2236 | FFC0:0001 / 3 | 5 steps | yes | yes | V | L |
| SteelSeries Nova 3P/3X, Nova 5/5X | 1038:2269,226d,2232,2253 | FFC0:0001 / 3 | yes (%) | yes | yes | V | L |
| SteelSeries Nova 7 family | 1038:2202,22a1,227e,2206,2258,229e,22ad,223a,22a9,227a,22a4,22a5,220a,22a7,2298 | FFC0:0001 / 3 | 5 steps or % (by PID/firmware) | yes | yes | V | L-M |
| SteelSeries Nova Pro Wireless | 1038:12e0,12e5 | - / 4 | 9 steps | yes (cable) | yes | V | L |
| SteelSeries GameBuds | 1038:230a | FFC0:0001 / 3 | yes per bud | no | docked=off | V | L |
| HyperX Cloud II Wireless (Kingston) | 0951:1718 | - | yes | yes | no | V | M |
| HyperX Cloud II Wireless (HP) / Stinger 2? | 03F0:0696,018B,0D93 | FF90:0303 / 0 | yes + mV | yes | no | V / I (Stinger 2) | M |
| HyperX Cloud Flight (Wireless) | 0951:16C4,1723 (HP 0E90,1749) | - | % from voltage | by voltage | no | V | M |
| HyperX Cloud Alpha Wireless | 03F0:098D,1743,1765 | - | yes | yes | yes | V | M |
| HyperX Cloud Alpha 2 Wireless | 03F0:08BE | FF13:FF00 / 2 | yes + mV | yes | yes | V (Windows tested) | M |
| HyperX Cloud II Core / III / III S Wireless | 03F0:069F,0995 / 05B7,0C9D / 06BE,02CC | - | yes | yes | partial | V (HyperHeadset) | M |
| Logitech G533 / G535 / G633-G935 / G PRO / PRO X | 046d:0a66 / 0ac4 / 0a5c,0a89,0a5b,0a87,0ab5,0afe,0b1f / 0aa7,0aaa,0aba,0afb,0afc | FF43:0202 (G535: 000C:01) / 3 | % from mV curve | yes | yes | V (curves approximate) | M |
| Logitech G PRO X 2 | 046d:0af7 | FFA0:0001 / 3 | yes | yes | yes | V | H |
| Logitech G PRO X 3 / G522 | 046d:0b45 / 0b18 | FFA0:0001 / 3 | yes | yes | yes | V | H |
| Logitech G435 / G930 | 046d:0acb / 0a1f | - | U / U (G930 decode not read) | - | - | U | - |
| Astro A50 Gen 4 | 9886:002c | FF32:0074 / 6 | yes | yes | yes (link bit) | V (Linux-tested) | M |
| Astro A50 Gen 5 | 046d:0b1c | FF32:0074 / 8 | yes | yes (dock) | U | V | M |
| Corsair Void family (legacy) | 1b1c: many (see file) | FFC5:0001 / 3 | yes | yes | yes | V | L |
| Corsair Void Wireless V2 / Virtuoso MAX / HS80 MAX | 1b1c:2a08 / 2a02 / 0a97 | - / 4 | yes (0.1 %) | no | yes | V | M |
| Corsair Virtuoso XT / SE | 1b1c:0a64,0a62 / 0a3e,0a3d | FF42:0001 / 3 | yes | yes | yes | V | M |
| Corsair HS80 RGB Wireless / HS55 Wireless | U | U | U | U | U | U | - |
| Razer BlackShark V2 Pro (PS) | 1532:1014 | FF00:0001 / 3 | yes (+mV) | yes | yes | V | H |
| Razer Barracuda / Kaira / Nari / other | U | U | U | U | U | U | - |
| Turtle Beach Stealth / ROCCAT Elo Air | U (Elo Air 1e7d:3a37, no battery) | U | U | U | U | U | - |
| Audeze Maxwell / Maxwell 2 | 3329:4b19,4b18 / 4b29,4b28 | FF13:0001 / 0 and 5 | yes | no | timeout | V | M |
| Sony INZONE H5 / H9 II | 054c:0EBF / 0FA8 | FF04:0002 / any; FF04:0001 / 5 | yes | yes | yes (0xFF) | V | M |
| Sony INZONE Buds / PULSE 3D | 054c:0EC2 / 0D5E | passive reports | yes | PULSE yes | PULSE yes | V | L |
| EPOS H3PRO Hybrid | 1395:0303,0101 | FFFF:0001 / 3 | yes (20 % steps) | no | yes | V | M |
| Sennheiser GSP 370 | 1395:009A | serial/COM (not HID) | yes | no | yes | V (Linux only) | H |
| Fifine HT6 | 3142:00c3 | FF90:01 | yes | yes | yes | V | L |
| MCHOSE X9 / V9 Pro | 3837:6045 / 291d:385d | FF90:01 | yes | no | no | V | L |
| Lenovo VoIP | 17EF:A07D | FF07:0222 / 3 | yes | no | no | V | L |
| ASUS ROG Delta/Cetra, JBL Quantum, Cooler Master, Attack Shark | U | U | U | U | U | U | - |
| Any Bluetooth headset | - | PnP property or GATT 0x180F | yes (if reported) | rarely | by connection flag | V/I/L mix | M |

## Cross-cutting Windows notes (all [I] unless marked)
* hidapi convention in HeadsetControl: **byte 0 of a buffer is the report ID** (0x00 = unnumbered, not sent). On Windows via `WriteFile`/`HidD_SetOutputReport`: buffer length must equal `OutputReportByteLength` and byte 0 = report ID (0 if the descriptor has none). Read `HidP_GetCaps` at runtime instead of trusting a hard-coded length.
* A dongle usually exposes several top-level collections (each its own device path). Select by **usage page + usage** via `HidD_GetAttributes`/`HidP_GetCaps`; HeadsetControl's "interface" numbers map to the `MI_xx` part of the path.
* Open vendor collections with `FILE_SHARE_READ | FILE_SHARE_WRITE`, no admin needed. Vendor apps (GG, iCUE, NGENUITY, G HUB, INZONE Hub) usually keep the collection open and, for several protocols, **input reports are delivered to every handle** - loop reads and match on command/ID/TID. [V] for Alpha 2, Razer, Sony, SteelSeries settings.
* Always use a read timeout (0.5-2 s); do not block the UI thread. Poll no faster than 30-60 s; Razer BlackShark and Logitech Centurion involve mode switches/bridges - poll slowly.
* "Dongle present, headset off" is the normal idle state; every protocol that supports it reports it in-band (see per-file tables). Some (Astro Gen 4, SteelSeries Nova dongles for chat-mix) keep serving **stale** values - gate battery on the link/offline byte first.
* Use `Windows Settings > Bluetooth` battery as a cross-check when the headset also connects over Bluetooth.

## Key sources
* HeadsetControl https://github.com/Sapd/HeadsetControl (device list in README; `lib/devices/*.hpp`)
* HyperHeadset https://github.com/LennardKittner/HyperHeadset (MIT)
* Linux kernel steelseries series https://patchew.org/linux/20260112041941.40531-1-srimanachanta@gmail.com
* Microsoft Bluetooth GATT docs (see `headset-bluetooth-generic.md`)
