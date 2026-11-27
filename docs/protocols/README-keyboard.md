# Keyboard protocol research - summary (battery %, charging, polling Hz; wired / 2.4 GHz / Bluetooth)

Research date 2026-11. Method: third-party reverse-engineering repos (cloned and read), vendor web-driver bundles where reachable, Linux kernel/openrazer/Solaar sources. No GPL code copied; byte facts restated.
Confidence: **V** verified in source (URLs in each file), **I** inferred, **U** unknown / no public source found.

## Protocol families (one driver each)
| File | Family | Covers |
|---|---|---|
| keyboard-rongyuan-monsgeek.md | RongYuan web-driver protocol (feature report ID 0, 64 B, cmd+checksum; dongle `F7`, poll `0x83`) | MonsGeek, Akko, Womier, Epomaker (RT/HE/TH/G84), Yunzii, ikbc, Hator, Keydous, Gamakay, Noppoo, MageGee, EWEADNV, Cherry MX8.2/K5, Valkyrie/VGN (VID 0x3151, 0x38EE, 0x379A, 0x374A...) |
| keyboard-aula-compx-aa.md | AULA/CompX dongle (32 B `20 01` battery) + "AA frame" (PMO, AULA F87 Pro) + SinoWealth BYCOMBO | AULA, Epomaker x AULA (F75/F87/S75/F108), generic CompX 2.4G boards, PMO Wave 75 |
| keyboard-sinowealth-0x13-mchose.md | SinoWealth report 0x13 (20 B) + MCHOSE 0x4D / 0x55 generations | MCHOSE (G87, Z75S, G75 Pro, G98V2, Jet75...), VGN V108, Xinmeng M71 |
| keyboard-nuphy-55frame.md | 0x55/0xAA 65-byte chunk frame | NuPhy V1/WH80 (and MCHOSE "CZ", AULA mouse) |
| keyboard-qmk-via-keychron.md | QMK/VIA/Vial raw HID (0xFF60/0x61) | Keychron, IQUNIX, NuPhy V2+, Weikav, Lofree-like, ZMK |
| keyboard-logitech-hidpp.md | HID++ 2.0 | Logitech G915/G913/G Pro/MX Keys/K-series |
| keyboard-razer.md | 90-byte Razer report | BlackWidow V3/V4 wireless, DeathStalker V2 Pro, Huntsman |
| keyboard-corsair-bragi.md | Bragi 65 B | K70 Pro Mini WL, K100 Air, K65 Plus WL, MAKR 75, Vanguard 99 Air |
| keyboard-asus-rog.md | ROG 0x12 commands | Falchion RX LP, Azoth 96 HE |
| keyboard-wired-analog-misc.md | no battery / unknown | Wooting, DrunkDeer, Glorious, SteelSeries, HyperX, Ducky, Varmilo, RK, Ajazz AK820, Lofree, Luminkey, Feker, Leobog |
| keyboard-windows-generic-ble-and-fallbacks.md | OS-level: BLE BAS, USB interval, Windows access quirks, fingerprint table | all |

## Brand table
Columns: Battery / Hz / Charging per connection; confidence per cell. "BAS" = BLE Battery Service (percent only). "n/a" = not applicable (wired powered).

| Brand | Family | 2.4 GHz dongle | Wired USB | Bluetooth | Polling Hz | Charging flag | Conf |
|---|---|---|---|---|---|---|---|
| MonsGeek / Akko (RY5088 era) | rongyuan | battery: `F7` (feature) idx1 = %; async input 0x05 `88` | battery via F7 (no charge) I; n/a | BAS | `0x83` code 0..6 = 8000..125 | async event bit1 (V); F7 byte unreliable | V (dongle, Hz) / I (wired) |
| MonsGeek / Akko (YC3121/YC3123/YC500) | rongyuan | same cmds I | I | BAS | U | U | I |
| Womier, Epomaker RT/HE/TH/G84, Yunzii, ikbc, Hator, Keydous, Gamakay, Noppoo, MageGee, EWEADNV, Cherry MX8.2/K5, VGN/Valkyrie | rongyuan | as above (VK 374A:A272 F7 quirk V) | as above | BAS | as above (8K models) | as above | V (catalog membership) / I (battery on all) |
| Epomaker x AULA (F75/F87/S75/F108), AULA, generic CompX | aula-compx | `20 01` query -> byte3 % | none | BAS | U (settings 0x11 byte5 3..6 = 1K..8K on F87 only) | U | V (battery) |
| PMO Wave 75 | aula-compx (AA) | `AA 30` -> [8] %, [9] 0/1/2 | none | BAS | U | V | V |
| MCHOSE (G87/Z75S/G75 Pro/G98V2) | sinowealth-0x13 | `13 4A 01` -> [5] %, [6] 0x10 = charging | none | BAS | U | V (stale-cache caveat) | V |
| MCHOSE G87 V2 / K99 V3 / K87S (VID 0x3837) | mchose QHW / 0x55 | 0x4D `0x0904` (%, charge 0/1/2) or [41] bit7/low7 | same | BAS | I (perf cfg 0x0002 on mice) | V | V (G87V2 hw-tested) / I |
| VGN V108, Xinmeng M71 | sinowealth-0x13 | `4A` / push 0x0A type 5 | none | BAS | U | V | V |
| NuPhy Air/Halo V1, WH80 | nuphy-55frame | cmd 0x05 @ `mode*64+32` -> [9]&0x7F | same (wired PID 0xA011) | BAS | cfg `+4` low nibble 1..4 = 125..1000 | inferred from PID/100 % | V (battery) / I |
| NuPhy Air75/96/60 V2, Halo V2, Field75 | qmk-via | U | n/a | BAS | n/a | none | I |
| Keychron (all) | qmk-keychron | U (Link receiver bridge usage 0x8C) | n/a | BAS (firmware pushes % to BT module) | `A7 0D` -> idx 0..6 = 8000..125 (8K models) | none | V (no raw battery cmd) / V (Hz layout, doc-derived) |
| IQUNIX | qmk-via | U | n/a | BAS I | U | U | U |
| Weikav (NUT65/75, Sugar65, WK87) | qmk-via | U (closed tray adapter exists) | n/a | BAS | U | U | U |
| Luminkey, Feker, Leobog | misc | Leobog GM5 Pro passive push (V, 1 device); others U | n/a | BAS | U | U | U |
| Logitech | hidpp | receiver slot or direct: feature 0x1004 fn1 / 0x1000 / 0x1001 / 0x1F20 | same, devIdx 0xFF | HID++ over BLE collection I; BAS | 0x8060 (ms) / 0x8061 (index 0..6 = 125..8000) | 0x1004 p2 (0/1/2/3...) V | V |
| Razer (V3 Pro/Mini, V4 Mini/TKL HyperSpeed, DeathStalker V2 Pro) | razer | class 07 cmd 80 -> arg[1]/255; tid 0x9F | tid 0x1F/0x3F | BAS I | class 00 cmd C0 -> arg[1] 1/2/4/8/10/20/40 = 8000..125 (V4 TKL HS, V4, Huntsman V2) | class 07 cmd 84 | V |
| Corsair (K70 PM WL, K100 Air, K65 Plus WL, MAKR 75, Vanguard 99 Air) | corsair-bragi | `02 0F` endpoint 0x09 -> LE u16/10 | endpoint 0x08 | BAS I | set known (1..7 = 125..8000); get U | `02 10` -> 0/1/2/3 | V |
| ASUS ROG Falchion RX LP | asus-rog | `02 12 01` -> rep[6] % (receiver 0B05:1ACE) | n/a | BAS I | U | U | V |
| ASUS ROG Azoth 96 HE | asus-rog | `{rid 12 01}` -> RX[6] %, RX[9]==1 charging | 1C10 same | BAS I | U | V | V (community) |
| Wooting | wooting | no battery | n/a | no BT | U (8K on 60HE v2/80HE) | n/a | V (no battery) / U (Hz) |
| DrunkDeer | misc | no battery | n/a | none | U | n/a | V (no battery) |
| Glorious GMMK | misc | no battery found | n/a | n/a | U | n/a | I |
| SteelSeries Apex Pro wireless | misc | U | n/a | BAS I | U | U | U |
| HyperX Alloy Rise 75 Wireless | misc | U | n/a | BAS I | U | U | U |
| Ducky, Varmilo, Lofree (stock), Royal Kludge, Ajazz AK820 Pro | misc | U (AK820 BLE module battery is an internal UART only) | n/a | BAS | U | U | U |

## Notable findings
1. One OEM (RongYuan) sits behind a large share of "Chinese gamer" brands - ~960 catalog entries at VID 0x3151 plus 0x38EE/0x379A/0x374A/0x39AB; one driver (F7 / 0x83 / 8F+FC) covers MonsGeek, Akko, Womier, Epomaker, Yunzii, ikbc, Hator, Keydous, Gamakay, Cherry (MX8.2/K5) etc.
2. Keychron firmware has **no HID battery command**; battery only via BLE BAS. Hz via `A7 0D` on 8K models only.
3. No standard in QMK/VIA/Vial/ZMK for battery or polling; ZMK = BLE BAS only (USB-HID battery is an open PR).
4. The cheap 2.4 GHz "CompX" dongle (`05AC:024F`, Apple VID borrowed) answers a 2-byte query `20 01` with the % at byte 3 (AULA F75 Max, S75 Pro; generic CompX boards).
5. The SinoWealth 20-byte report-0x13 channel (`13 4A 01`) is shared by MCHOSE/VGN/Xinmeng; dongle returns **stale cached values for hours** when the keyboard idles.
6. Windows blocks read access to keyboard/mouse TLCs; some replies need `dwDesiredAccess=0` + `HidD_*` (DrunkDeer case).
7. Polling-rate code tables **differ per family** (RongYuan 0=8000..6=125; Keychron same order; AULA AA 3=1K..6=8K; Razer bitmask 0x01=8000..0x40=125; Logitech 0x8061 index 0=125..6=8000 us-based; Corsair set 1=125..7=8000; NuPhy 1..4=125..1000). Do not share tables.
8. Wooting, DrunkDeer, Glorious, SteelSeries, HyperX, Ducky, Varmilo: no public keyboard battery/polling protocol found.
9. Generic fallbacks: BLE BAS via WinRT/`DEVPKEY_Device_BatteryLevel` for all Bluetooth boards; USB endpoint `bInterval` for wired polling rate.
