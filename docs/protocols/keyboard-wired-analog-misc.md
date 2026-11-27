# Keyboard brands with no (or unproven) host-readable battery: Wooting, DrunkDeer, Glorious, SteelSeries, HyperX, Ducky, Varmilo, Cherry, Royal Kludge, Ajazz/Epomaker (non-RongYuan), Lofree, Luminkey, Feker

Legend: **V** verified in third-party source (URL), **I** inferred, **U** unknown (not found in any public source I could reach). Nothing here is invented; where a bullet says "U" no byte-level information was found.

## 1. Wooting (VID 0x31E3; legacy 0x03EB)  - wired only, no battery
* Keyboards: One, Two, 60HE, 60HE+, 60HE v2, 80HE, Two HE, UwU RGB. All USB cable. **No battery exists** (V by product line; no wireless keyboard).
* Analog interface (V, https://github.com/WootingKb/wooting-analog-sdk `plugin/wooting.rs`): VID `0x31E3` (current) or `0x03EB` with PIDs `0xFF01/0xFF02` (legacy). Analog report interface usage page **0xFF54** (v1, 48-byte buffer) or **0xFF53** (v2, 64-byte buffer); `PID & 0xFFF0` masks the mode nibble (the PID changes in bootloader/gamepad modes). Wootility Web opens exactly these two usage pages (its WebHID filter compares `usagePage` to 65364 / 65363 = 0xFF54 / 0xFF53) - V (bundle fetched from wootility.io, constants decoded).
* Polling rate: set in Wootility ("Polling Rate", 8 kHz only on 60HE v2 and 80HE per Wooting changelog; gamepad polling up to 8000 Hz on those, 1000 Hz elsewhere - V https://wooting.io/wootility/changelogs/5.3.0-beta.2). **The config/feature-report protocol that carries it is not public** and the web bundle is obfuscated: **U**. Use USB bInterval fallback (8 kHz = high-speed endpoint with bInterval 1, 125 us).
* Do not send anything to the analog interfaces; only read input reports (they stream key depth).

## 2. DrunkDeer (VID 0x352D gen-1, 0x19F5 gen-2 OEM)  - wired only
* A75 / A75 Pro / A75 Ultra / G65 / G60. USB cable; **no battery**. V: https://github.com/svumo/DrunkDeer-Control docs.
* Vendor interface: usage page 0xFF00, 64/65-byte reports, **output report ID 0x04**, payload 63 bytes; identity query `04 A0 02 00 ...` -> reply `04 A0 02 00 ?? B5 B6 B7 fwLo fwHi ...` (model triple + firmware); toggles at bytes 15,16,18,19. Gen-2 firmware requires 65-byte buffer. Known PIDs (VID 0x352D): 0x2382, 0x2383 (G65), 0x2384, 0x2386, 0x2387 (A75 Ultra), 0x238F. An A75 Pro OEM variant appears as `19F5:FB5C` (same VID as NuPhy - match by manufacturer string "DrunkDeer"). V.
* Polling rate: not in the documented opcode set (opcodes A0/A2/A7/A8/AA/AB/B5/B6/C1/FB). **U** (A75 Pro 8 kHz selection happens in firmware/keybord.net.cn driver; unknown byte).
* **Windows quirk (important for all vendor protocols):** on gen-2 boards replies arrive on a keyboard/mouse top-level collection (`mi_02&col01\kbd`) that the OS blocks from normal open. Workaround that Chrome WebHID effectively uses: `CreateFile(..., dwDesiredAccess = 0, ...)` then `HidD_SetOutputReport` / `HidD_GetInputReport` (the HidD_* family works on handles with zero access). Not confirmed on hardware by the project (~50 %). V (https://github.com/svumo/DrunkDeer-Control docs/gen2-oem-investigation.md).

## 3. Glorious (GMMK / GMMK 2 / GMMK Pro / GMMK 3 / GMMK 3 HE)
* SinoWealth VID `0x258A` (GMMK 1/2; I) and Evision-type VID `0x320F` (GMMK Pro/3; I - from memory, not re-verified here). Keyboards are cabled; **no wireless keyboard battery** in the lineup I could confirm. The Glorious battery request seen in third-party notes (`00 00 02 02 00 83`, feature report, "258A scheme") is the **mouse (Model O/D wireless)** query; it was rejected by a 258A:010C combo receiver (V FerrisPulse 1.9) - do not use for keyboards.
* GMMK Pro polling/other: software (Glorious Core) only; protocol **U**. (OpenRGB / dokutan `rgb_keyboard` only drive RGB.)

## 4. SteelSeries (VID 0x1038)
* Apex Pro TKL Wireless / Apex Pro Mini Wireless / Apex Pro (wired): battery readable in GG, **protocol U** - no public byte-level source found (searches for Linux/Windows reverse engineering returned only headset code: Linux `hid-steelseries` covers Arctis headsets).
* Headset hint only: Arctis Nova 5/5X uses usage page 0xFFC0, 65-byte interrupt report `00 B0` -> reply `rx[2]==2` offline, `rx[4]` percent, `rx[5]==1` charging (V FerrisPulse). See headset-steelseries.md. Whether Apex wireless keyboards reuse it: **U**.
* Apex 7 TKL / Apex Pro wired: macro/OLED/brightness only (apexctl, apex7tkl_linux); no battery. V (names).

## 5. HyperX (VID 0x03F0 / 0x0951)
* Alloy Origins/Core/FPS: wired, no battery. Alloy Rise 75 Wireless: 2.4 GHz + BT; battery shown in NGENUITY; **protocol U** (no reverse-engineering write-up found). Headsets/mice: see headset-hyperx.md.

## 6. Ducky, Varmilo, Cherry, Royal Kludge, Ajazz/Epomaker (non-RongYuan), Lofree, Luminkey, Feker
| Brand | Findings | Conf |
|---|---|---|
| Ducky (One 3, One X Mini Wireless) | No public HID protocol found. Wired boards: none. BLE: GATT BAS. | U |
| Varmilo (EC/Minilo BT, VXX) | No public HID protocol found; BLE BAS only. | U |
| Cherry | MX 8.2 (`046A:012D`) and K5 TMR (`046A:0141`) appear in the **RongYuan web-driver catalog** -> treat as RongYuan family (keyboard-rongyuan-monsgeek.md). Cherry classic wireless (KW 9100, Stream, MX Board 3.0S wireless) : U. | V (catalog) / U |
| Royal Kludge (RK61/RK68/R65/RK84/R87/R98/RK100) | RK software "Rangoli"/kludgeknight/rkcu drive Sonix-based wired config (RK61 reverse-engineered USB config) - no battery commands (grep found none). Tri-mode boards: BLE BAS only; 2.4 GHz dongle battery **U**. | V (repos checked) / U |
| Ajazz AK820 Pro (and Epomaker AK820) | Two-chip design: USB MCU + CH582F BLE/2.4G module. Battery is an **internal UART link**: MCU sends `A6 53` every 5 s and gets `5C <pct> <sum>` (`5C 64 C0` = 100 %); not a host HID command. Host-side battery over USB: **U** (Linux tools only do lighting/clock/sleep). V (https://github.com/fpb/ajazz-ak820-pro docs/CH582F_PROTOCOL.md). Other Ajazz boards in the RongYuan catalog (K690T `05AC:024F`/`0461:xxxx`) -> see other families. | V / U |
| Epomaker | Mixed: RT/HE/TH/G84 = RongYuan (31 PIDs listed); F75/F108/S75 "x Aula" = AULA family (keyboard-aula-compx-aa.md); other Epomaker = VIA. Controller project `epomaker-controller` (Python) sends `f7` poll on 2.4G for RT100 -> RongYuan. | V |
| Lofree (Flow, Flow2, Edge, Dot) | Stock: BLE BAS + closed web driver. A third-party custom firmware ("Studio") uses a 32-byte request/response with nonce for battery on wired Flow2 - **not stock**. Stock USB protocol U. | V (existence) / U |
| Luminkey (Luminkey65 etc.) | No source found (one search, nothing). Likely generic web driver over WebHID; battery U. | U |
| Feker | No source found. | U |
| Womier | RongYuan catalog: SK75 TMR, M68 HE PRO, KF112/KF305 (VID 0x3151) -> RongYuan family; Womier K66/K87 etc. may be QMK/VIA. | V (catalog) |
| Leobog | `04F3:026F` GM5 PRO: input push frames on usage page 0x000A/0x0000, 32 B: `[1]` awake flag, `[4]` percent (passive). Hi75/Hi8: U. | V (FerrisPulse) |
| Weikav | See keyboard-qmk-via-keychron.md (VIA + a closed 2.4G tray-tool adapter). | U |
| MCHOSE | See keyboard-sinowealth-0x13-mchose.md. | V |
| IQUNIX | See keyboard-qmk-via-keychron.md section 3 (VIA-style FF60/61 + closed commands, VID 0x3869). | U |

## 7. Practical consequence for the tray app
For every brand in this file: wired -> show "wired, battery n/a" and derive polling rate from USB descriptor/measurement; Bluetooth -> BAS; 2.4 GHz -> unsupported until a protocol is captured. Add a **diagnostic dump** (VID/PID, product string, HID collections with usage page/usage/report lengths - same data the Wireless-Device-Battery-Monitor project asks users for) so new protocols can be added from user reports.

## Sources
* https://github.com/WootingKb/wooting-analog-sdk ; https://wootility.io (bundle) ; https://wooting.io/wootility/changelogs/5.3.0-beta.2
* https://github.com/svumo/DrunkDeer-Control (docs/keyboard-protocol-gen2.md, gen2-oem-investigation.md, Driver/KeyboardManager.cs)
* https://github.com/fpb/ajazz-ak820-pro (docs/CH582F_PROTOCOL.md)
* https://github.com/Ferris-echo/FerrisPulse/blob/main/docs/HARDWARE_PROTOCOLS.md ; https://github.com/strodgers/epomaker-controller
* https://github.com/rnayabed/rangoli , https://github.com/oddlyspaced/rkcu , https://github.com/AstroSnail/apexctl (checked for battery strings: none)
