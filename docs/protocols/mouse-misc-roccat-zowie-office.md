# Roccat / Turtle Beach, Zowie / BenQ, Microsoft / HP / Dell office mice, generic Bluetooth

Scope: what (if anything) can be read read-only. Research date 2026-11-24. Tags: **V** verified in source, **I** inferred, **U** unknown.
These brands have little or no open protocol documentation; the honest result is mostly "unknown" or "nothing to read".

## 1. Roccat / Turtle Beach (Kone Pro Air, Burst Pro Air, Kone XP Air, Kone Pro, Burst Pro, Kone XP)

* VIDs: Roccat `0x1E7D` (legacy; wired Kone Pure `2DBE/2DC2/2DCB`, Kone XTD `2E22`, Kone EMP `2E24`, Kone AIMO `2CED`); post-acquisition products use Turtle Beach VID `0x10F5` (seen in HaloBattery's device list for Turtle Beach headsets, e.g. `10F5:2232`). V libratbag data/devices, perihub devices.md; I for which VID the Air mice use.
* **Battery: U.** No open-source project documents it. A SignalRGB forum thread ("[Battery Status] Kone Pro Air") confirms users cannot read it outside Roccat Swarm. Eruption (Linux) lists Kone Pro Air as "testing/work-in-progress" without battery. U. Do not guess a protocol.
* **Polling rate: only for old wired Kone** (libratbag `driver-roccat.c`): feature report ID **6**, 43 bytes (`ROCCAT_REPORT_ID_SETTINGS`), first select the profile with feature report ID 4 (`04 <profile> 80`), then GetFeature 6; struct field `report_rate` (offset 19 in the 43-byte report counting the ID as byte 0) = index: 0 = 125, 1 = 250, 2 = 500, 3 = 1000 Hz. V libratbag driver-roccat.c L43-L51, L58-L76, L645-L690 (the sources' own error message mentions `buf[26]`; offset I). The settings report carries a checksum - never write.
* Newer Roccat (Kone Pro / XP / Burst, Air) use the Turtle Beach "Swarm 2" protocol: U.

## 2. Zowie / BenQ

* VID/PIDs: U (not researched to a source; do not hard-code). Wired mice (EC/FK/ZA/S series) have no battery. **Nothing to read**: the polling rate is set by a hardware button or Zowie's "Mouse Rate Adjuster" utility, which only writes; no HID readback is documented anywhere we found. -> show n/a; use measurement (see README-mouse-major.md) if a rate display is wanted.
* Zowie EC-CW / EC2-CW (wireless, 2023+): protocol U.

## 3. Microsoft / HP / Dell office mice

* **Bluetooth (Microsoft Bluetooth Mouse, Arc, Surface Mobile, HP/Dell Bluetooth mice)**: do not talk to the device. Windows already reads the standard GATT Battery Service and exposes it as a PnP property: **`DEVPKEY_Bluetooth_Battery` = `{104EA319-6EE2-4701-BD47-8DDBF425BBE5}`, property id 2** on the BTHLE/BTHENUM device node (value 0..100). Connection state: WinRT `BluetoothLEDevice.FromBluetoothAddressAsync(mac).ConnectionStatus`. V HaloBattery providers/bluetooth.py L1-L30. Charging flag: none (GATT battery level has no charge state; U).
  Rust: `SetupDiGetDevicePropertyW` / `CM_Get_DevNode_PropertyW` with that key on each present `BTHLE\DEV_*` node (Windows 10/11).
* **Microsoft 2.4 GHz (Nano Transceiver v2/v1), Dell WM-series, HP wireless**: Microsoft's non-Bluetooth wireless mice expose no vendor battery report in any open project (U). **Dell and HP office mice that ship a Logitech-made Nano receiver** (`046D:C535` "branded as Dell", `C52F`, `C534` ...) are Logitech HID++ devices: read them with `mouse-logitech-hidpp.md` (HID++ 1.0 registers `0x07`/`0x0D`, or HID++ 2.0 `0x1004`). V Solaar base_usb.py L135 (`C535` Dell). Dell Premier/MS7xxx wireless via Dell Peripheral Manager: U.
* Microsoft/HP/Dell **wired** mice: nothing to read (no battery; no polling readback).

## 4. Generic fallback for any HID mouse
* Standard HID Battery System collection (usage page `0x85`, usage `0x44` Battery Strength) or Generic Device Controls `0x06/0x20`: some BLE HID mice report battery as a normal input report inside their own HID descriptor. Parse the report descriptor with `HidP_GetCaps`/`HidP_GetValueCaps` for usage page `0x06` usage `0x20` (Battery Strength) or page `0x85` usage `0x66`/`0x44`; read the value, no vendor protocol needed. V: used by the Linux kernel (hid-input battery) and the HID-BPF driver described in glorious-i2-battery (it injects exactly that usage). I for Windows availability per device.

## 5. Sources
* libratbag: https://github.com/libratbag/libratbag/blob/master/src/driver-roccat.c ; data/devices/roccat-*.device
* HaloBattery: https://github.com/HeyOkay/HaloBattery/blob/main/providers/bluetooth.py
* Solaar base_usb.py (Dell Nano receiver); perihub docs/devices.md (Roccat list)
* Search results (no protocol found): SignalRGB forum "Battery Status Kone Pro Air"; Eruption project device list
