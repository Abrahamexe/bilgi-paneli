# Windows generic paths for keyboards: Bluetooth LE battery, standard HID battery, polling-rate fallbacks, access quirks, device fingerprinting

Legend: **V** verified in third-party source (URL), **I** inferred (standards knowledge / not exercised here), **U** unknown.

## 1. Bluetooth LE keyboards (every brand): standard Battery Service
* GATT Battery Service **0x180F**, Battery Level characteristic **0x2A19** (uint8 0..100, read + notify). Percent only, **no charging flag**. Almost every BLE tri-mode board (Keychron, NuPhy, Akko/MonsGeek, Epomaker, Lofree, ZMK, Logitech, MCHOSE ...) exposes it. V (BAS spec reference https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/BAS_v1.1/out/en/index-en.html ; used in vigil-watt `Ble/BleBatteryWatcher.cs`).
* **Two ways on Windows (both used in the wild, V):**
  1. WinRT: `BluetoothLEDevice.GetDeviceSelector()` -> `DeviceWatcher` with extra property `System.Devices.Aep.Battery_Level` / `System.Devices.AepBattery_Level` (name varies by Windows build; try both) = value cached by the OS; fallback: `BluetoothLEDevice.FromIdAsync` -> `GetGattServicesForUuidAsync(0x180F)` -> `GetCharacteristicsForUuidAsync(BatteryLevel, Uncached)` -> `ReadValueAsync`. Works for already-paired devices without owning the HID service. (vigil-watt BleBatteryWatcher.cs)
  2. PnP property: the keyboard shows up as two nodes: `BTHLEDEVICE\{0000180F-...}_DEV_VID&0xxxxx_PID&xxxx_REV&xxxx_<MAC>` (has VID/PID, no battery) and its parent `BTHLE\DEV_<MAC>` (has FriendlyName and **`DEVPKEY_Device_BatteryLevel`, type DEVPROP_TYPE_BYTE**, 0..100). Read it as a single byte (reading 4 bytes gives garbage). In instance IDs VID/PID are written `VID&023837` / `PID&4026`: only the **last 4 hex digits** are real. MAC: last 12 hex digits of the segment after the final underscore before the last backslash. (FerrisPulse HARDWARE_PROTOCOLS.md 1.6; same property used for classic-BT HFP headsets in vigil-watt.) The property key GUID is `{104EA319-6EE2-4701-BD47-8DDBF425BBE5}`, pid 2 - **I** (widely quoted, not independently verified in this session).
* Staleness: Windows can fail to request BAS notifications, so the displayed level may lag (ZMK docs warning; Windows "battery level not refreshing" threads). Use periodic uncached reads (30-60 s) rather than waiting for notifications. V (https://zmk.dev/docs/features/battery).
* Device identity: VID/PID from the `BTHLEDEVICE` node (some boards present generic VID:PID like `07D7:EFFF`; confirm name from the `BTHLE` parent FriendlyName prefix).
* HID-over-GATT keyboards cannot be opened for vendor commands (vendor collection absent) - e.g. MonsGeek BLE ignores vendor commands, MCHOSE K99 V3 BLE has none. V.

## 2. Standard HID battery usage (rare on keyboards)
* HID usage page 0x06 (Generic Device Controls) usage 0x20 (Battery Strength), 0..100, in an input or feature report; optional page 0x85 (Battery System) usage 0x44 (Charging). Linux turns it into a power_supply automatically; **Windows does not expose it to user mode apps by itself** - the app must scan each HID collection (`HidP_GetCaps`, `HidP_GetValueCaps` for UsagePage 0x06/Usage 0x20) and then `HidD_GetFeature` / `HidD_GetInputReport`. (I; descriptor shape verified in the patched MonsGeek firmware and hid-asus: report ID 7 feature `[7, level, charging]`, ASUS BT keyboard report ID 3.) Cheap to probe: if present it is a safe read.
* ZMK USB-HID battery: open PR only (zmk #2938) - U if shipped.
* Stock Chinese tri-mode boards almost never include it; vendor commands (other files) are needed.

## 3. Polling rate when no vendor command exists (wired USB)
1. **USB endpoint interval (best generic source), I.** The HID interrupt-IN endpoint descriptor's `bInterval` gives the *negotiated polling period*: Full-Speed = `bInterval` ms (1 ms = 1000 Hz); High-Speed = `2^(bInterval-1)` microframes of 125 us (bInterval 4 = 1 ms = 1000 Hz, 3 = 500 us = 2000 Hz, 2 = 250 us = 4000 Hz, 1 = 125 us = 8000 Hz). Read the configuration descriptor through a USB hub IOCTL (`IOCTL_USB_GET_DESCRIPTOR_FROM_NODE_CONNECTION` on `\\.\USB#ROOT_HUB...`, via SetupAPI hub enumeration + `IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX` for device speed) - no driver install needed, read-only. Caveat: boards that change rate by re-enumerating with a new descriptor are accurate; boards that keep one descriptor and gate reports in firmware (some 8K boards) report the *maximum*, not the active rate - then prefer the vendor command (RongYuan `0x83`, Razer `0xC0`, Keychron `A7 0D`, NuPhy block `+4`).
2. **Measuring** report spacing with Raw Input timestamps is unreliable for keyboards (reports are event-driven, not periodic; 8 kHz cannot be seen from key events) - do not rely on it. (I)
3. Bluetooth/2.4 GHz rate: cannot come from USB; use vendor commands only.

## 4. Windows HID access quirks (all vendor protocols)
* Keyboard / mouse **top-level collections cannot be opened with read/write access** by user mode (kernel-owned). Replies that arrive on them (observed on DrunkDeer gen-2) can only be read by opening the handle with `dwDesiredAccess = 0` and using `HidD_GetInputReport` / `HidD_GetFeature`; feature-report based protocols (RongYuan, Razer) are fine because their vendor collection is separate. V (https://github.com/svumo/DrunkDeer-Control docs/gen2-oem-investigation.md - ~50 % confidence stated by authors).
* Use `HidD_SetFeature/GetFeature` for feature-report protocols (RongYuan 0xFFFF/0x02 64 B; Razer 91 B); `WriteFile/ReadFile` (interrupt) for output-report protocols (QHW 0x4D, 0x13 family, AULA, Logitech HID++, Corsair, NuPhy, ASUS); the QHW family ignores feature reports. V.
* Write size must equal the collection's declared report length (`HIDP_CAPS.OutputReportByteLength`, includes the report-ID byte); a report ID the collection doesn't declare returns `ERROR_INVALID_FUNCTION`. Read the report ID from caps, don't hard-code. V (vigil-watt protocols.md 4.3).
* Overlapped read first, then write, for protocols whose reply arrives immediately (NuPhy). Do not drain stale reports before writing for 8K receivers (breaks the wireless handshake) (FerrisPulse).
* Only one session client: official apps/WebHID tabs hold device state (MCHOSE session, RongYuan cache slot). Retry gracefully; back off exponentially after 3 failures (vigil-watt policy: 30 s .. 5 min).
* Detect devices that are asleep: dongles keep answering with cached values (MCHOSE `4A`, RongYuan `F7`); show "last update" age.

## 5. Fingerprint table (VID/usage -> family)
| Match | Family file |
|---|---|
| VID 0x3151/0x38EE/0x39AB/0x379A/0x374A + collection usage 0xFFFF/0x02 (feature 64) | keyboard-rongyuan-monsgeek.md |
| VID 0x05AC PID 0x024F, product "2.4G Wireless Receiver", usage 0xFF60 | keyboard-aula-compx-aa.md |
| 0x0C45:0xFEFC/0xFEFE/0x8803, 0x38A6:0x27xx, 0x36B0:0x3002 | keyboard-aula-compx-aa.md (AA frame) |
| usage 0xFF02/0x0002, in/out = 20, report ID 0x13 (VID 0x258A, 0x41E4, 0x3554, 0x391D) | keyboard-sinowealth-0x13-mchose.md |
| VID 0x3837 (+0xFF01/0xFF0B 0x4D frame; or 65-byte usage 1/0) | keyboard-sinowealth-0x13-mchose.md section 6 |
| VID 0x046D, usage 0xFF00/0x0001 (7) or 0x0002 (20) | keyboard-logitech-hidpp.md |
| VID 0x1532 with a 91-byte feature report | keyboard-razer.md |
| VID 0x1B1C, 65-byte vendor interface | keyboard-corsair-bragi.md |
| VID 0x0B05 PIDs 0x1ACE/0x1AD0/0x1C10, usage 0xFF02 or 0xFF00 | keyboard-asus-rog.md |
| VID 0x19F5, in/out 65 | keyboard-nuphy-55frame.md |
| usage 0xFF60/0x61, 32 B, VID 0x3434 / 0x3869 / others | keyboard-qmk-via-keychron.md |
| VID 0x31E3 usage 0xFF53/0xFF54 | keyboard-wired-analog-misc.md (Wooting) |
Generic diagnostic to ship: list every HID collection of every keyboard-class device (VID, PID, product string, usage page/usage, in/out/feature lengths, `MI_xx`/`COLxx`) in a copyable block - the same PowerShell `Win32_PnPEntity` filter the Wireless-Device-Battery-Monitor project asks users to run (HID/USB/BTHLE nodes, `HardwareID`/`CompatibleID`). V: https://github.com/ZGMFX01A/Wireless-Device-Battery-Monitor/blob/main/docs/device-support.md

## Sources
* https://github.com/Mightling/vigil-watt (MIT) `src/VigilWatt.Core/Ble/BleBatteryWatcher.cs`, docs/hardware/protocols.md
* https://github.com/Ferris-echo/FerrisPulse docs/HARDWARE_PROTOCOLS.md (GPL-3, facts only)
* https://github.com/svumo/DrunkDeer-Control docs ; https://zmk.dev/docs/features/battery
* https://github.com/torvalds/linux/blob/master/drivers/hid/hid-asus.c ; echtzeit-solutions/monsgeek-akko-linux docs/FIRMWARE_PATCH.md
