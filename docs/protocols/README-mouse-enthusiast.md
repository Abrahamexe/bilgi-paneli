# Enthusiast / Chinese-OEM mice: brand -> protocol family map

Scope: read-only battery %, charging flag, polling rate (Hz), model name on Windows via raw HID. Research date 2026-11-24.

Method and honesty note: the vendors' WebHID configurators are mostly large lazily loaded bundles. Most facts here come from **open reverse-engineering projects** (madr, libatk-rs, lamzu-cfg, pulsar-mouse-linux, Linux hid-kysona / hid-pulsar patch, OpenMouse's npm package `@openmouse/protocol` which itself transcribes ATK HUB 3.2.27, M HUB, xpanel, Aurora, mouse.xyz, etc.). I fetched the ATK HUB landing page and its main chunk list but did not independently dump the vendor bundles; OpenMouse is AGPL so only protocol facts are recorded, in my own words, no code copied.

Confidence: V = verified in published source; HW = source states hardware-confirmed; I = inferred; U = unknown / not found.

## Families (one driver each)

| Family file | Core idea | Brands |
|---|---|---|
| mouse-family-compx-report8.md | 17-byte report 8, checksum 0x55-sum, battery cmd 0x04 | ATK, VXE, Dragonfly, Pulsar (Nordic gen, CrazyLight), Lamzu Atlantis, Scyrox, VGN, Kysona, Teevolution, Gravastar, G-Wolves XVI:0, Dareu, Endgame WE (OP1we, XM2we), Waizowl (I) |
| mouse-family-compx-xvi-page64.md | 64-byte unnumbered feature report, page/command, battery 0x83 | WLMouse, Lamzu Maya X/Inca/Paro, Attack Shark R5/R6/R8, LunaFury, CRDRAKO, Vancer, RAWM, G-Wolves XVI:1, Hitscan Hyperlight |
| mouse-family-pulsar-sonix-fusion2.md | 64-byte feature, sum16 checksum, cat 8 battery | Pulsar X2A/X3/Xlite v4/Feinmann + 1K/8K dongles, Pulsar Pro 5405 |
| mouse-family-beken-x11-gearhub.md | push battery report 03 / GearHub dongle status | Attack Shark X11/R1/X3/X6/R2, Delux, Fantech WG14P, Lingbao |
| mouse-family-vaxee.md, -finalmouse-xpanel.md, -rapoo-vt.md, -mchose.md, -ninjutso-incott-ajazz.md | single-brand protocols | Vaxee, Finalmouse, Rapoo VT9, MCHOSE, Ninjutso, Incott, Ajazz/K-snake |

Key finding: the hypothesis holds. **Family A** (CompX report 8) covers the biggest share of the brands: VXE/ATK/Dragonfly, Pulsar's Nordic mice, Lamzu Atlantis/Thorn/Maya(old), Scyrox, VGN, Kysona and G-Wolves share the same frame, checksum and command ids, and the Linux kernel reviewers independently called the Pulsar, Kysona and ATK R1 SE+ protocols identical. VID 0x3554 is the CompX ODM id (Pulsar, Kysona, VXE, Lamzu, Scyrox, VGN, Teevolution, Gravastar all sit on it; the distinct PIDs and CID/MID separate brands). **Family B** (CompX XVI 64-byte page protocol) is the second CompX stack (WLMouse, newer Lamzu, Attack Shark R5, G-Wolves XVI:1).

## Brand table

| Brand / models | Family | Battery % | Charging | Polling Hz | Model name | Confidence |
|---|---|---|---|---|---|---|
| VXE R1/R1 Pro/Max/SE/SE+, MAD R, ATK F1/X1/A9/U2/Y9, Dragonfly | A | cmd 0x04 [6] | [7]==1 | flash 0x0000 code (R1 SE+ 1K dongle: live row 0x70, read path U) | CID/MID cmd 0x10 + catalog | V, HW |
| Pulsar X2/X2A/X2H/X2V2/Xlite V3 (+Mini), 4K receiver (0x3554) | A | cmd 0x04 | yes | flash 0x0000 | cmd 0x01 model id | V, HW |
| Pulsar X2 CrazyLight, Noctua (0x3710 Nordic) | A | cmd 0x04 | yes | flash 0x0000 (2K/4K/8K 0x10/0x20/0x40) | cid 0x57 + mid | V, HW |
| Pulsar X3, Xlite v4, X2A wired, Feinmann, 1K/8K dongles | C | cat 8 reg 0x81 | cat 8 reg 0x83 | cat 1 reg 0x89 [7] | cat 1 reg 0x84 | V, HW |
| Pulsar Pro 0x5405 | C (sec. 6) | cmd 0xB4 | [6] | cmd 0xB2 u16 Hz | U | V only |
| Lamzu Atlantis / Thorn / Mini / OG V2 (0x3554 f50d/f50f/f510) | A | yes | yes | flash 0x0000, ambiguous codes >=0x10 | same PID for all models | V, HW (Atlantis Mini 4K) |
| Lamzu Maya X, Inca 8K, Paro Aurora (0x373e / 0x37b0) | B | cmd 0x83 | yes | page 1 cmd 0x80 | paired product cmd 0x8B | V, HW (Inca) |
| WLMouse Beast X/Pro/Mini/Miao/Max/G, Strider, Huan, Ying, Sword X | B | cmd 0x83 | yes | page 1 cmd 0x80 | cmd 0x8B | V, HW (receiver) |
| WLMouse Beast X 4K (36a7:a887) | own (report 4, CRC16) | **U** | U | settings offset 0x0B | U | V (settings), battery U |
| Attack Shark R5 Ultra/R6/R8 (0x373e) | B | yes | yes | yes | PID | V, HW (R5 U) |
| Attack Shark X11/R1/X3/X6 (0x1d57) | D1 | push report 03 (X11/R1/X6 scales known) | no | **no read-back** | model id in push | V, HW (X11) |
| Attack Shark R2, Lingbao M5 Pro (0x3151) | D3 | cmd 0xF7 [2] | U | OPTIONPARAM0 byte 9 | GET_USB_VERSION | V, HW |
| Fantech WG14P Yari Pro 8K | D3/Fam B variant | via dongle 0xF7 (I) | U | 0=8000..5=125 (zero replies on 0x402D) | | V, partial |
| Scyrox V8/V8S | A | cmd 0x04 | yes | flash 0x0000, CompX table | cid 0x3E mid 5 | V, HW |
| VGN Dragonfly F2 Master+ | A | cmd 0x04 | yes | flash 0x0000 | | V |
| Kysona M600 | A | `08 04 .. 49` | yes | flash | | V (kernel) |
| MCHOSE A7 V2 / L7 / A7 V3 / A5 V3 / K7 / K5 / R7 | MCHOSE V2 or V3 | V2 cmd 0x06 [9]; V3 cmd 0x0900 [12] | V2 [10], V3 [11] | config blob rate index | PID inside reply | V, HW (A7 V2 Ultra+) |
| Ninjutso Sora V3 / TEN | Ninjutso | cmd 18 | cmd 17 | cmd 6 (1000-8000) | cmd 167 | V, HW U |
| Ninjutso Sora V2 | Ninjutso legacy | settings [7] | [8] | settings [24] | cmd 40 | V, HW U |
| Finalmouse ULX / Starlight X | xpanel | ULX mV curve (cmd 5), SLX cmd 38 | SLX cmd 37 | u16 Hz cmd 4/17 | fixed by PID | V |
| Endgame OP1we / XM2we | A (variant A2: feature 8 / input 9) | cmd 0x04 [5] | [6] | flash 0x0000 125-1000 | PID | V |
| Endgame OP1 8K / XM2w 4K / OP1w 4K | own (report 0xA1) | **U** | U | U | PID | battery U |
| Endgame OP1 / OP1 RGB / XM1r (wired) | n/a | no battery | | | | V |
| G-Wolves HTX Ultra (XVI:0, 0x33e4:5618/3854) | A | cmd 0x04 | yes | flash 0x0000 | | V, HW |
| G-Wolves XVI:1 (HTX Mini 8K, Fenrir, HTM Plus, VUK ...) | B | cmd 0x83 / legacy 0x8F | yes | page 1 cmd 0x80 | MID | V, HW (Fenrir, HTX Mini) |
| Vaxee XE/NP-01S/AX/E1/Sora | Vaxee | cmd 0x0B steps*5 | cmd 0x10 | cmd 0x07 | cmd 0x0A | V |
| Rapoo VT9 Pro (VT3 Pro) | Rapoo | input 0xBB byte 5 / cmd 0xAA | cmd 0xAA marker 2 | EEPROM 0x880 | U | V, HW |
| Ajazz NJ07/NJ08 (+MC, PRO), K-snake X11, Noir M2-NEX | yjx2012 | cmd 0x30 [7] | [8] | config [9]-1 | PID | V, HW (NJ07 MC) |
| Incott G23V2Pro | Incott | unsolicited input 0x09 | byte0>100 | feature 0x81 | PID | V, HW |
| Dareu A950 Pro / TM265 | A | cmd 0x04 [5] | [6] status | yes | PID | V |
| Delux M600 Pro / M800 Mini | D1 | push model 0x20 | no | no | | V, HW |
| Waizowl (Duckbill Elite/OGM) | A (I) | I | I | I | ATK catalog "WAIZOWL,1" | I |
| Darmoshark, Zaopin, Waizowie spelling | **no source found** | U | U | U | U | U |
| Redragon M719 Pro (0x25a7) | D2 | U | U | U | | V ids only |

## Suggested implementation order for the tray app

1. **Family A** first (one driver, ~30 brands). Implement: find collection with report 0x08 (0xFF02/0xFF04), `HidD_SetOutputReport` (variant A1) or `HidD_SetFeature` + read input 0x09 (A2), send cmd 0x03 then 0x04, validate checksum, parse [6],[7],[8..9]. Polling: read flash 0x0000. Identity: cmd 0x10 (ATK) or 0x01 (Pulsar).
2. **Family B** (WLMouse/Lamzu/G-Wolves XVI) next: Set/GetFeature report 0, battery 0x83 and polling 0x80.
3. **Pulsar C** (Fusion2 dongles) and **D1/D3** push/relay readers.
4. Brand singletons (Vaxee, Rapoo, MCHOSE, Finalmouse, Ninjutso, Incott, Ajazz) as time allows; Rapoo and Incott and Attack Shark X11 are **listen-only** paths (do not need requests).

## Cross-cutting caveats

* "Polling rate" in all families except Finalmouse/Pulsar Pro (u16 Hz) is the **configured profile value**, not a measurement. Wired and wireless links can keep separate stored rates (MCHOSE, Rapoo). Fallback for a true measured rate: count the mouse's own input reports with a high-resolution timer while it moves (I, not in any source).
* Never send write/reset/pair/upgrade/profile-change commands (list in family A file).
* Poll >= 30-60 s; several firmwares sleep after 30 s idle and the dongle then answers "no link" (status 0x05 or 0x02) or nothing.
* Vendor apps (Pulsar Fusion, ATK HUB, Lamzu Aurora) hold the same collections; Windows input-report queues are per handle, so a second reader is expected to work (I) but control-transfer writes can interleave: serialise requests and match replies by command echo.
* Interface selection is the main practical risk: every device exposes 3-5 HID collections; pick by usagePage/usage and by report IDs declared (HidP_GetCaps), not by PID alone.

## Sources
See per-family files. Main: https://github.com/dkbednarczyk/madr, https://github.com/cyberphantom52/libatk-rs, https://github.com/LeadSun/lamzu-cfg, https://github.com/packerlschupfer/pulsar-mouse-linux, https://patchew.org/linux/20260318205503.64420-1-nikolas@koesling.info, https://github.com/OpenMouse-Project/openmouse, https://unpkg.com/@openmouse/protocol@0.60.0/dist/, https://github.com/BuSd777/OpenVXE
