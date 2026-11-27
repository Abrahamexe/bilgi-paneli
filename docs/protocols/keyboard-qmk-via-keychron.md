# Keyboard family: QMK / VIA / Vial raw HID, Keychron (Launcher), IQUNIX, NuPhy V2+, Weikav, ZMK

Legend: **V** verified in third-party source (URL), **I** inferred, **U** unknown.

## 1. Generic QMK / VIA / Vial raw HID - is there a standard battery/polling query? **No.**

* Raw HID interface: **usage page 0xFF60, usage 0x61**, 32-byte input and output reports, report ID 0 (so 33 bytes with the leading 0 on Windows). V: https://docs.qmk.fm/features/rawhid
* VIA protocol (command byte 0) defines get-protocol-version / get-keyboard-value (uptime, layout options, switch-matrix state) / custom channels. It has **no battery id and no polling-rate id** (I, from VIA spec knowledge; not re-read in this session). Vial uses prefix 0xFE for its own commands (I).
* QMK has an internal battery *driver* abstraction (`BATTERY_ENABLE`, 0-100 % callback) but it is not wired to any host-facing HID report (V: https://docs.qmk.fm/features/battery - docs mention only the sampling API).
* Consequence: battery/polling for a stock QMK/VIA board can only come from (a) vendor-specific commands on the same raw HID pipe (Keychron below), (b) the standard BLE Battery Service when connected over Bluetooth, or (c) nothing.
* **Detection of "this is a QMK/VIA board"** is cheap and safe: enumerate HID collections with usagePage 0xFF60 / usage 0x61. Do NOT send arbitrary bytes there to "probe" - VIA command 0x03/0x04+ are *writes*; the only read-safe command is `0x01` (get protocol version) -> reply echoes `01` + big-endian version.
* ZMK: battery is exposed through the **BLE Battery Service (0x180F)**; only the central half is reported unless proxy/fetching options are enabled (V: https://zmk.dev/docs/features/battery). USB-HID battery reporting is only an *open PR* (zmk #2938, per a blog post - U whether merged). Windows may show stale BLE battery values (ZMK docs say Windows does not request notifications properly). Polling rate on ZMK: not exposed (fixed ~125 Hz BLE / 1000 Hz USB) - U.
* "QMK wireless forks": Keychron (below) pushes the % into the Bluetooth module (BAS) rather than HID; no common wire format exists.

## 2. Keychron (VID 0x3434)

Brands/models: Q/V/K/K Pro/K Max/Q Max/V Max/Q HE/K HE/Lemokey L/P/X series. Wireless models carry a Bluetooth module (LKBT51) and optionally a 2.4 GHz "Link" receiver.

| Item | Value | Conf |
|---|---|---|
| VID | 0x3434 (PIDs per model: unknown here; Keychron uses 0x0Xxx, receivers 0xD0xx) | V (info.json uses `0x3434`) |
| Launcher WebHID filters | `{usagePage 0xFF60, usage 0x61}` keyboard; `{usagePage 0x8C, usage 1}` 2.4 GHz bridge; `{0xFFC1, 1}` mouse; `{0xFF0A,1}` 4K mouse | V (keychron-vial docs, quoting the Launcher JS) |
| Raw HID size | 32 bytes (QMK default) | V |
| Keychron command set | `0xA0` protocol version, `0xA1` firmware version string, `0xA2` support-feature bitmap, `0xA3` default layer, `0xA7` misc group, `0xA8` RGB, `0xA9` analog matrix, `0xAA` BT-module DFU, `0xAB` factory test | V (firmware keychron_raw_hid.c, wireless_playground branch) |
| `0xA2` feature bits | bit1 FEATURE_BLUETOOTH, bit2 FEATURE_P24G, bit3 analog matrix, bit5 dynamic debounce, bit6 snap click | V |

### 2.1 Battery
* **Public Keychron firmware has NO raw-HID battery command.** The battery % goes to the Bluetooth chip (`update_bat_level(battery_get_percentage())`) which publishes it via the standard BLE Battery Service. V: https://github.com/Keychron/qmk_firmware/tree/wireless_playground/keyboards/keychron/common (wireless/wireless.c, keychron_raw_hid.c).
* A community fork adds `0xAC` "get battery level" (reply byte 1 = %, 0 when on USB). That collides with the Launcher's own `0xAC` (screen), so it is **not** a Keychron stock command. V: https://github.com/Tymon3310/keychron-vial/blob/main/docs/firmware/core-commands.md. Do not use.
* The same community doc claims keyboard battery rides in "sleep settings" reply (0xA7/0x0B) - I could not find that in the firmware source (reply only carries backlight/idle timeouts); treat as **U/likely wrong**.
* Keychron **mice** (not keyboards) have a proven vendor battery path: Linux driver uses report `0xB3` (request) / `0xB4` (reply) with command `0x06`, battery in reply byte 20: bit7 = charging, bits0-6 = %, interface 4 / usage 0xFFC1; receiver `3434:D028` (Ultra-Link 8K); model behind receiver via `0xB5 0x03` (state) / `B5 02 <target>`. V: https://github.com/csutcliff/keychron-battery-dkms and FerrisPulse doc (Link-KM `3434:D026`). Included only because Keychron keyboards on the Link receiver may reuse the bridge command set (`0xB1` version, `0xB2` slots, `0xB3` fw) - **U** whether the 2.4 GHz keyboard receiver answers the mouse-style status.
* Net: Keychron keyboards - **BLE only via Windows GATT**; wired = N/A; 2.4 GHz = U.

### 2.2 Polling rate
* `0xA7 0x0D` REPORT_RATE_GET (set = 0x0E), only on 8K-capable models (`HSUSB_8K_ENABLE`, feature bit 5 of the misc feature word at `0xA7 0x01` reply idx 5). Handler body is a closed library in the public tree. V (enum + dispatch): keychron_raw_hid.h/.c; reply layout V (third-party doc, reverse-engineered from Launcher JS): https://github.com/Tymon3310/keychron-vial/blob/main/docs/firmware/misc-commands.md
  * Request: `[0xA7, 0x0D]` (32-byte report, rest zero).
  * Reply v1: `data[2]=status(0 ok)`, `data[3]=rate_index`, `data[4]=supported_mask`. v2 (dual USB/2.4G): `data[3]=usb_rate_index`, `data[4]=usb_supported_mask`, `data[5]=fr_supported_mask`, `data[6]=fr_rate_index`.
  * Index table: 0=8000, 1=4000, 2=2000, 3=1000, 4=500, 5=250, 6=125 Hz. mask bit N set = index N supported (default 0x7F).
* Reply data[] index is *with* the command echo at 0 (so `data[0]=0xA7`, `data[1]=0x0D`). Conf: V for table, **I** for the exact v1/v2 discriminator (version from `0xA7 0x01` reply, protocol 0x0002).

### 2.3 Model name
`0xA1` returns an ASCII version string ("v1.2.3 <build date>"), not the model. Model comes from USB product string / PID (the Launcher uses its own PID->json). 2.4 GHz bridge (`usage 0x8C`): `0xB2` GET_STATE lists up to 3 slots as `[vid_hi,vid_lo,pid_hi,pid_lo,status]` per slot (offset 2) - V (third-party doc).

## 3. IQUNIX (VID 0x3869)
Raw HID `FF60/61`, 32-byte In/Out, VIA-style WebHID filter (V: emulation lab https://github.com/1916395355-coder/usbip-vidpid-emulator - PIDs 0x7838 EC75-class, 0x7C18, 0x63E1 EV63). IQUNIX uses a **proprietary command set on top of the same pipe** (their "EZ" web configurator) - bytes **U**. Fn+B on Q101/MQ80 shows battery on the keyboard LEDs only (custom firmware project). Battery/polling to host: **U**; BLE BAS expected (I).

## 4. NuPhy
* Air60/Air75/Halo75 **V1**: proprietary 0x55-frame console protocol (see keyboard-nuphy-55frame.md).
* Air75 V2 / Air96 V2 / Halo V2 / Field75 / Gem80 etc.: **QMK/VIA** (NuPhy docs) - battery: BLE BAS only; wired/2.4 GHz: **U** (nothing in VIA). V (statement by third-party): https://github.com/donn/nudelta (Readme). NuPhy IO (nuphy.io, "later Air75 HE...") is a proprietary web driver - protocol **U** (not decompiled here).
* VID 0x19F5 (NuPhy), see keyboard-nuphy-55frame.md for the 2.4G dongle PIDs seen in a third-party tray tool.

## 5. Weikav / Lofree / Leobog / Feker (summary only)
* Weikav NUT65/NUT75/Sugar65/WK87/Stars21: ship QMK/VIA (VIA json repos exist; SignalRGB plugin for NUT65 uses VIA raw HID) - V: https://github.com/bhctsntrk/nut65-signalrgb , https://github.com/trananhmanh89/weikav-wk87-via-json (names only; not read). Battery over HID **U**; BLE BAS. A tray tool lists "Weikav / NUT75" as supported on 2.4G (bytes private) - https://github.com/ZGMFX01A/Wireless-Device-Battery-Monitor (README; closed protocol crate, non-commercial licence) - so some Weikav 2.4G dongle has a vendor battery query: **U** (not published).
* Lofree Flow/Flow2: stock firmware = BLE BAS + closed web driver; Flow2 *custom* "Studio" firmware has a documented 32-byte vendor request/response (not stock; ignore). Stock USB protocol **U**.
* Leobog (Hi75/GM5 Pro): FerrisPulse reports a "LEOBOG GM5 PRO (04F3:026F) usage 0x000A/0x0000 32 B push frames: `[1]=awake`, `[4]=percent`" - V (third-party doc, single device): https://github.com/Ferris-echo/FerrisPulse/blob/main/docs/HARDWARE_PROTOCOLS.md ("Community batch" item 6). Passive listen only.
* Feker (Redragon-like) : **U**.

## 6. How the tray app should treat QMK-class boards
1. Wired USB: battery = N/A (keyboard is powered); polling = derive from USB endpoint interval (see keyboard-windows-generic-ble-and-fallbacks.md).
2. Bluetooth: read BAS (0x180F/0x2A19) via WinRT / `DEVPKEY_Device_BatteryLevel` on the BTHLE parent node.
3. Keychron 8K models only: attempt `A7 0D` for polling.
4. Never write to usage 0xFF60 collections other than the Keychron read commands above (A0/A1/A2/A7 01/A7 0D); VIA "set" commands change user config.

## Sources
* https://docs.qmk.fm/features/rawhid , https://docs.qmk.fm/features/battery
* https://github.com/Keychron/qmk_firmware (branch wireless_playground) keyboards/keychron/common/*
* https://github.com/Tymon3310/keychron-vial/tree/main/docs (reverse-engineered from launcher.keychron.com main bundle)
* https://launcher.keychron.com/ (main bundle fetched; contains mouse battery/pollrate monitor code `report[0]==1 && report[3]==1`, power at `[5]`/`[6]` - mouse only)
* https://github.com/csutcliff/keychron-battery-dkms
* https://zmk.dev/docs/features/battery
