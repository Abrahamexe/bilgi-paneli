# Smaller OEM families: Ninjutso (NinjaForce), Incott, Ajazz NJ07/K-snake (yjx2012 panel), misc

Confidence per item: V = in OpenMouse source, HW = hardware-verified per source, U = unknown.

## 1. Ninjutso (NinjaForce web client). Mostly unverified on hardware.
* Current gen: VID **0x093A**; mice e010 (Sora V3), e020 (TEN/TEN AIR), ea01; receivers eb01, eb02. Legacy Sora V2: VID **0x1915** mice ae11-ae16, receivers ae1c ae8c ae8a.
* Current protocol: **feature report ID 6**, 15 bytes. Request `[cmd, 0, 0, 1, 0, nargs, profile, args...]` (SET_FEATURE) then GET_FEATURE ID 6; reply value from index 8 (`bytes[1]==cmd`; sometimes ID byte missing). Commands: **17 = battery charging** (value[0] nonzero), **18 = battery percent** (value[0], cap 100), 19 online, 21 firmware, **6 = polling** (value[0] code 1..4 = 1000, 2000, 4000, 8000), 167 = paired product id (value[0]|value[1]<<8, name of mouse behind receiver). Profile byte only for profile commands.
* Legacy (Sora V2): feature report ID 5 (31 bytes) + ID 4; a settings read returns data where `[7]` = battery percent, `[8]` = 1 charging (wireless only), `[24]` rate index; polling only on high-rate receivers; model via command 40.
* Wait ~ms between set and get; retries 4. Confidence V, HW U.

## 2. Incott (VID 0x093A, PID 0x522C 2.4G / 0x622C wired; G23V2Pro)
* Config: usagePage 0xFF05, **feature report 0x09**. Battery in feature replies was proven wrong (constant). **Battery lives in the unsolicited input report 0x09** the mouse sends only while being used: `byte0` (after ID) <= 100 = percent discharging; > 100 = charging, percent = byte0 - 128; `byte1` high nibble = DPI stage, low nibble = polling index. V, HW
* Polling read: feature `09 81` reply byte 2 wire code: 0=1000, 1=500, 2=250, 3=125, 4=8000, 5=4000, 6=2000 (wired max 1000). V, HW
* Listener must stay open; battery is only available after the mouse moved.

## 3. Ajazz NJ07/NJ08 and K-snake X11, Noir M2-NEX (yjx2012 web panels: nacodex.yjx2012.com, x1a11.yjx2012.com)
* VID 0xA8A4 wired / 0xA8A5 dongle; NJ07 2157, NJ08 2158, NJ07 MC 2167, NJ08 MC 2168, MC PRO 2177/2178; K-snake X11 / Noir M2-NEX 2255. usagePage 0xFF01 usage 0x10.
* Ajazz: **output report 0xF0**, 63-byte body, reply as input report with first byte echoing the command. Battery: send `[0x30, 0x01, 0 ...]`; reply byte[7] = percent, byte[8] nonzero = charging. Polling: GET_CONFIG `[0x0E, 0x01, 0x0B, 0x2E]`, reply byte[9]-1 = index into [125,250,500,1000]. Firmware cmd 0x04. HW (NJ07 MC fw 2.0.1)
* K-snake: output report 0, 64 bytes starting with 0x55 magic; battery cmd 0x30 with tail `a5 0b 2e 01 01`; decoder in drivers/ksnake (unverified).
* Ajazz AJ200P/AI line: different (VID 0x363C, usagePage 0xFFA0 usage 2, output report 0x0B, 127 bytes, info cmd 0x13 battery at data[14]). V, HW unknown.

## 4. Other items found, not families of interest
* Dareu: Family A (report 8, 0xFF02) - see mouse-family-compx-report8.md.
* Bytech (VID 0x372E), Redragon M690/M719, Motospeed, Zaunkoenig, Fater, Thermaltake: individual drivers in OpenMouse, not decoded here.
* Darmoshark, Zaopin: **no public protocol source found** (search rate limit hit before further attempts). U. Waizowl: appears in the ATK HUB catalog (373b:1282) so probably Family A (I).

## 5. Sources
https://unpkg.com/@openmouse/protocol@0.60.0/dist/ninjutso/index.js, incott/index.js, ajazz/index.js, ksnake/index.js, delux/index.js, drivers/*/hid.js
