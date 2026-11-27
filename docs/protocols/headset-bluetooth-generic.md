# Bluetooth headsets / earbuds on Windows: generic battery path (HFP property vs BLE Battery Service)

Tags: **[V]** verified in a cited source, **[L]** verified locally on the dev machine (Windows 11 10.0.26300, non-elevated PowerShell, 2026-11-24), **[I]** inferred, **[U]** unknown. Never assume a [I]/[U] row works - the verification script in section 8 exists for that.

Goal: read battery % (and charging if available) for any paired Bluetooth headset **from an unelevated Win32 app**, without admin, without waking or connecting a device that is off.

## 1. Two completely different mechanisms

| | Classic Bluetooth (BR/EDR) headsets | BLE / LE Audio devices |
|---|---|---|
| Where the battery number comes from | The headset pushes it to Windows over **HFP** (Windows is the Audio Gateway): standard HFP indicator `battchg` (0..5) via `+CIEV`, and/or the Apple/Plantronics-style `AT+IPHONEACCEV=<n>,1,<0..9>[,2,<dock>]` (battery 0..9 = 10..100 %, key 2 = dock state) **[I]** (well-known extensions; spec not re-fetched this session). HSP (headset profile) has **no** battery mechanism **[I]**. | **GATT Battery Service 0x180F**, characteristic **Battery Level 0x2A19**, uint8 0..100 % (Bluetooth SIG BAS v1.1: https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/BAS_v1.1/out/en/index-en.html ; characteristic confirmed in https://www.andreasjakl.com/read-battery-level-bluetooth-le-devices/) **[V]**. BAS v1.1 also adds *Battery Level Status* (0x2BED **[I]**), which carries charging state; support is rare. |
| How Windows exposes it | A **device property** on the PnP node: `{104EA319-6EE2-4701-BD47-8DDBF425BBE5}`, **pid 2**, type **Byte**, value = percent. Read with `SetupDiGetDevicePropertyW` / `CM_Get_DevNode_PropertyW` (DEVPROP_TYPE_BYTE), or `Get-PnpDeviceProperty -KeyName '{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2'`. Not exposed via WinRT. | Not (necessarily) a property; read via GATT (WinRT or Win32). Whether Windows also mirrors it into the same DEVPKEY for BLE devices is **[U]** - probe at runtime. |
| Needs the headset connected? | Yes - the value exists only while the HFP service-level connection is up. | Cached value may exist while disconnected (see 5) but is stale. |
| Charging flag | **None** in the property (single byte). HFP has no charging indicator; the Apple `dock` key is not surfaced by Windows **[I]**. | Only via optional Battery Level Status **[I]**. |
| Resolution | HFP battchg gives 6 steps (0/20/.../100 %) **[I]**; IPHONEACCEV gives 10 steps; some headsets report fine-grained values via vendor extensions. Property is just a byte, so check step size per device empirically. | 1 % if the vendor implements it. |

## 2. Classic path in detail

### 2.1 The property **[V]/[L]**
* Key: **`DEVPKEY` GUID `{104EA319-6EE2-4701-BD47-8DDBF425BBE5}`, pid `2`**, DEVPROP_TYPE_BYTE. Third-party evidence it holds battery %: PowerShell script for Sony WF-1000XM3 https://gist.github.com/muedsa/d5a94e2d5827d3b518063b9a2d2fdda3 (enumerates all PnP devices by friendly name, reads this key, **keeps entries whose Type is not `Empty`**); a C# WMI tool for Sony WH-1000XM4 https://github.com/nikvoronin/xm4battery (search-snippet level evidence: key listed as byte percent); a repo "Get-Bluetooth-Battery-Persentage" (HyperNylium; page not retrievable, snippet only) using `Get-PnpDevice -Class Bluetooth -FriendlyName ... | Get-PnpDeviceProperty -KeyName '{104EA319-...} 2'`. A Microsoft Q&A commenter claimed the Settings battery value was not reachable from PowerShell (2023) - contradicted by the scripts above; it is driver/device dependent.
* Microsoft does not document this key (no hit in docs search). Treat as an **undocumented but stable-in-practice** property. **[V]**

### 2.2 Which devnode **[L]**
On this machine every paired classic device has ~10 PnP nodes. For one earbud set (e.g. "Razer Hammerhead TWS X") the nodes were:
* `BTHENUM\DEV_<12-hex MAC>\7&...&BLUETOOTHDEVICE_<MAC>` - class **Bluetooth**, FriendlyName = device name. **This is the device-level node** and the best first place to look.
* `BTHENUM\{0000110B-...}_VID&..._PID&...\...` A2DP sink - class **MEDIA**, same friendly name.
* `BTHENUM\{0000110C-...}` / `{0000110E-...}` AVRCP target/controller "... Avrcp Transport" - class Bluetooth.
* `BTHENUM\{0000111E-0000-1000-8000-00805F9B34FB}_HCIBYPASS_...` "... Hands-Free AG" - class **System** (this is the HFP node Windows uses as the Audio Gateway; the battery plausibly originates here) **[I]**.
* `BTHENUM\{00001101-...}` SPP COM port (class Ports), `{00001801-...}` GATT, vendor service nodes (Apple "AAP Server" `{74EC2172-0BAD-4D01-8F77-997B2BE0722A}`, "UARPS").
Scripts match on FriendlyName and accept whichever node has a non-Empty value, i.e. the value may sit on different nodes per driver version. **Recommended algorithm**: collect all nodes whose instance ID contains the device's 12-hex MAC (also available as `DEVPKEY_Bluetooth_DeviceAddress`), read key {104EA319...}:2 on each, take the first non-Empty Byte. **[I]**

### 2.3 Local verification results **[L]**
Three paired classic devices (AirPods, Taotronics SoundLiberty 53, Razer Hammerhead TWS X), **all disconnected** at test time:
* `Get-PnpDeviceProperty -KeyName '{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2'` returned **type Empty on all 10 class-Bluetooth nodes**, and a full property dump of all 26 BTHENUM/BTH nodes (every class) contained **no {104EA319...} property at all** (property absent while disconnected). So: absent value == "not connected / not reporting", never "0 %".
* `{83DA6326-97A6-4088-9453-A1923F573B29} 15` (a Boolean) was `False` on all device nodes and `True` on the bus nodes (`MS_BTHBRB`, `MS_RFCOMM`, `MS_BTHLE`): this is the node's **connected** flag **[L]** (tentative naming; use it as a "is connected right now" gate before polling) - read **without connecting anything**.
* `DEVPKEY_Bluetooth_DeviceAddress`, `DEVPKEY_Bluetooth_DeviceVID/PID`, `DEVPKEY_Bluetooth_LastConnectedTime` exist on the DEV_ node **[L]** (AirPods: VID 76 = 0x004C Apple, PID 8217 = 0x2019).
* All of this ran **unelevated** (`IsInRole(Administrator)` = False) **[L]**, confirming no admin is needed to read PnP properties. The battery property itself could not be read live (no connected headset at test time) - **[U] pending the section 8 script**.
* WinRT Bluetooth enumeration also works unelevated: `BluetoothDevice.GetDeviceSelectorFromPairingState(true)` + `DeviceInformation.FindAllAsync` returned the 3 paired classic devices; `BluetoothLEDevice...(true)` returned 0 (no LE devices paired) **[L]**.

### 2.4 Which headsets populate it
No authoritative list exists. Evidence: Sony WF-1000XM3, WH-1000XM4/WF-1000XM4 (third-party tools above) **[V]**. Apple AirPods: they send IPHONEACCEV to iOS/macOS, but their primary battery channel is Apple's proprietary AAP over L2CAP (the `AAP Server` service node seen locally) - **whether Windows' HFP stack parses it is [U]**; Windows 11 Settings may or may not show an AirPods percentage. TWS earbuds from Razer/Taotronics etc.: **[U]**. Gaming headsets in Bluetooth mode (Arctis Nova, Cloud Alpha Wireless BT, G735...): **[U]**. In general "if Windows Settings > Bluetooth & devices shows a battery % under the device, the property is populated" **[I]** (same source in the Windows stack).

### 2.5 Win32 read recipe (no admin) **[I]**
1. `SetupDiGetClassDevsW(NULL, L"BTHENUM", NULL, DIGCF_ALLCLASSES | DIGCF_PRESENT)` (or CM_Get_Device_ID_List with filter `BTHENUM`), enumerate `SP_DEVINFO_DATA`.
2. For each node: `SetupDiGetDevicePropertyW(devInfo, &data, &DEVPROPKEY{ {0x104EA319,0x6EE2,0x4701,{0xBD,0x47,0x8D,0xBF,0x42,0x5B,0xBE,0x5}}, 2 }, &type, &byte, 1, &required, 0)`; accept only `type == DEVPROP_TYPE_BYTE`, value 0..100.
3. Gate on the connected flag `{83DA6326-97A6-4088-9453-A1923F573B29}` pid 15 (Boolean) of the DEV_ node, or `DEVPKEY_Device_IsPresent`.
4. Update trigger: `CM_Register_Notification` on the device interface class or just poll every 30-60 s (cheap: local PnP database read, no radio traffic). Event-driven update of this property on battery change: **[U]**.
Windows 10 (build >= ~1809) and 11 both expose it **[I]**; Windows 7/8 not supported.

## 3. BLE path in detail

### 3.1 Where it lives
Windows models a paired LE device as `BTHLE\DEV_<MAC>` with child service nodes `BTHLEDEVICE\{0000180f-0000-1000-8000-00805f9b34fb}_DEV_...` **[I]** (pattern from GATT-service devnodes; none present on the dev machine - **[U] locally**). Property mirroring into `{104EA319...}:2` for BLE nodes: **[U]**.

### 3.2 WinRT (recommended; works from an unpackaged Win32/Rust app)
```
BluetoothLEDevice.FromBluetoothAddressAsync(addr)              // or FromIdAsync(deviceId)
  -> GetGattServicesForUuidAsync(0x180F, BluetoothCacheMode.Cached)
  -> service.GetCharacteristicsForUuidAsync(0x2A19, Cached)
  -> characteristic.ReadValueAsync(Cached)   // IBuffer, 1 byte
```
* Doc facts **[V]** (https://learn.microsoft.com/en-us/uwp/api/windows.devices.bluetooth.bluetoothcachemode): for `GetGattServicesAsync/ForUuidAsync`, `GetCharacteristicsAsync/ForUuidAsync`, `GattCharacteristic.ReadValueAsync`, **`Cached` means "try the system cache first and, if absent, retrieve from the device"**; `Uncached` always goes to the device. The cache is **system-wide** (not per process) and is invalidated on service-change indications or unpair.
* So **`Cached` does NOT guarantee "no radio traffic"** - it falls back to the device if nothing is cached. There is no WinRT mode that means "cache only, never connect" for these calls.
* Creating the `BluetoothLEDevice` object does not by itself connect; a connection is initiated by `GattSession.MaintainConnection = true`, an uncached service discovery, or a read/write (search result summarising MS docs: https://learn.microsoft.com/en-us/windows/apps/develop/devices-sensors/gatt-client) **[V]**.
* `FromBluetoothAddressAsync` can return null/fail if the device is **not paired and not in the system cache** (docs; https://learn.microsoft.com/en-us/uwp/api/windows.devices.bluetooth.bluetoothledevice.frombluetoothaddressasync) **[V]**. Some peripherals allow battery reads unpaired, some (Surface Dial) only when paired (andreasjakl article) **[V]**. For headsets assume **paired**.
* Packaged apps need the `bluetooth` DeviceCapability; **unpackaged desktop apps (Win32/Rust/.NET) need no manifest capability** **[I]** (btleplug, the common Rust BLE crate, uses exactly these WinRT classes from plain executables: https://docs.rs/crate/btleplug/0.12.0/source/src/winrtble/ble/device.rs ). Admin is not required **[I]**; unelevated WinRT Bluetooth enumeration confirmed locally **[L]**; an actual GATT read could not be tested (no BLE device).
* Known hazard: some Windows BLE drivers hang indefinitely on Uncached requests; btleplug wraps them in a timeout and falls back to Cached **[V]** (snippet). Always use a timeout (3-5 s) and release the objects (`Dispose`/drop) so Windows can drop the link.
* Subscription alternative: write CCCD via `GattCharacteristic.ValueChanged` + `WriteClientCharacteristicConfigurationDescriptorAsync(Notify)`; keeps a connection - fine for an already-connected headset, undesirable for polling a sleeping one. **[I]**

### 3.3 Win32 `BluetoothGATT*` in BluetoothApis.dll
* `BluetoothGATTGetCharacteristicValue(hService, &characteristic, size, &value, NULL, flags)`; declared in bluetoothleapis.h, **BluetoothAPIs.dll**, Windows 8+ **[V]** https://learn.microsoft.com/en-us/windows/win32/api/bluetoothleapis/nf-bluetoothleapis-bluetoothgattgetcharacteristicvalue
* Flags **[V]**: `BLUETOOTH_GATT_FLAG_NONE` (default), `..._FORCE_READ_FROM_DEVICE` (read directly from device, overwrites cache), **`..._FORCE_READ_FROM_CACHE` (read from cache "regardless of whether it is present")**, plus `..._CONNECTION_ENCRYPTED/AUTHENTICATED`.
* Doc remark **[V]**: "the characteristic value is returned from the cache if one is already present ... all device attributes are cached at the time of pairing or association; if not present, it is read from the device and cached." The parent service must be present in the cache.
* => Win32 gives the **only true "never wake the device" read**: `FLAG_FORCE_READ_FROM_CACHE` (value may be stale). Use `NONE`/`FORCE_READ_FROM_DEVICE` only after the connected flag says the headset is connected.
* How to get `hService` **[I]**: enumerate device interfaces of class `GUID_BLUETOOTH_GATT_SERVICE_DEVICE_INTERFACE` {6E3BB679-4372-40C8-9EAA-4509DF260CD8} with SetupAPI; pick the interface whose path contains `{0000180f-0000-1000-8000-00805f9b34fb}`; `CreateFileW(path, GENERIC_READ|GENERIC_WRITE, FILE_SHARE_READ|FILE_SHARE_WRITE, NULL, OPEN_EXISTING, 0, NULL)` (the MS GATT client sample uses this pattern and runs as a standard user - **not re-fetched**, treat as [I]); then `BluetoothGATTGetCharacteristics(hService, NULL, 0, NULL, &n, BLUETOOTH_GATT_FLAG_NONE)` -> find UUID 0x2A19 -> `BluetoothGATTGetCharacteristicValue`. The device interface for the LE device itself is `GUID_BLUETOOTHLE_DEVICE_INTERFACE` {781AEE18-7733-4CE4-ADD0-91F41C67B592} **[I]**.
* Some services (HID-over-GATT) are owned exclusively by in-box drivers and open attempts return access-denied; the Battery service on headsets is expected to be openable, but **[U]**.
* Errors worth mapping **[V]**: ERROR_SEM_TIMEOUT (request timed out), ERROR_PRIVILEGE_NOT_HELD (characteristic not readable), E_BLUETOOTH_ATT_INSUFFICIENT_AUTHENTICATION/ENCRYPTION (need pairing).

## 4. Passive alternatives (no connection at all) **[I]**
`BluetoothLEAdvertisementWatcher` (WinRT) delivers advertisements without connecting; some ecosystems put battery in advertisements:
* **Google Fast Pair** battery notification (service data UUID 0xFE2C, left/right/case %, charging flag): spec https://developers.google.com/nearby/fast-pair/specifications/extensions/batterynotification **[V]** spec exists; whether a specific headset advertises it while connected is **[U]**.
* Apple proximity-pairing manufacturer data (company 0x004C) carries AirPods L/R/case battery + charging bits - reverse-engineered by third parties, only while the case lid is open / device advertising. **[I]**
These are brand-specific and unreliable once the headset is connected over classic BT (it stops advertising); use only as a fallback.

## 5. Not waking / not connecting - summary

| Goal | Classic HFP property | BLE GATT |
|---|---|---|
| Know if the device is connected without touching it | read connected flag `{83DA6326-...} 15` or presence of the {104EA319...}:2 value **[L]** | `BluetoothLEDevice.ConnectionStatus` after `FromBluetoothAddressAsync` (object creation alone does not connect) **[V]**; or the same PnP connected flag on `BTHLE\DEV_` node **[I]** |
| Read battery with zero radio traffic | property read is purely local **[I]** | Win32 `FORCE_READ_FROM_CACHE` only (stale) **[V]**; WinRT cannot guarantee it |
| Fresh value from an already-connected headset | same property (pushed by headset) | `NONE`/Uncached read: cheap if the link is already up |
| Never poll a headset that is off | gate on connected flag; do nothing otherwise | gate on `ConnectionStatus == Connected`; never call Uncached discovery on a disconnected device (it triggers a connection attempt and can wake or stall) |

## 6. Recommended implementation order for the tray app
1. Enumerate paired devices (WinRT `DeviceInformation.FindAllAsync` for classic and LE, or SetupAPI on `BTHENUM`/`BTHLE`); skip devices whose connected flag is False.
2. For classic: read {104EA319...}:2 across the device's nodes. If non-Empty -> done (percent, charging unknown).
3. Else if the device has GATT Battery Service in the system cache / is LE: WinRT Cached read of 0x2A19 with timeout; then optional Win32 FORCE_READ_FROM_CACHE fallback.
4. Else show "battery not reported over Bluetooth" (vendor-specific protocols: SPP/RFCOMM or custom GATT - see per-brand notes; e.g. AirPods AAP, Sony Headphones Connect, Jabra, Bose).
5. Poll 60 s; refresh on connect/disconnect notifications.

## 7. Open issues / [U]
* Does Windows populate {104EA319...}:2 for BLE or LE Audio nodes? Which node type carries it per driver version?
* Does the property update immediately on headset battery events, or only at connect time?
* Real-world list of headsets that populate it (needs community / lab data).
* Exact charging state for BT headsets: not available from either path in the general case.

## 8. Verification script (run when a headset is *connected*)
```powershell
# Dumps the battery property and connected flag for every BTHENUM/BTHLE node. Unelevated is fine.
Get-PnpDevice | Where-Object { $_.InstanceId -match '^(BTHENUM|BTHLE|BTHLEDEVICE)' } | ForEach-Object {
  $b = Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName '{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2' -ErrorAction SilentlyContinue
  $c = Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName '{83DA6326-97A6-4088-9453-A1923F573B29} 15' -ErrorAction SilentlyContinue
  '{0,-8} {1,-40} conn={2} batt={3} ({4})' -f $_.Class, $_.FriendlyName, $c.Data, $b.Data, $b.Type
}
```

## 9. Sources
* Microsoft: BluetoothGATTGetCharacteristicValue https://learn.microsoft.com/en-us/windows/win32/api/bluetoothleapis/nf-bluetoothleapis-bluetoothgattgetcharacteristicvalue ; BluetoothCacheMode https://learn.microsoft.com/en-us/uwp/api/windows.devices.bluetooth.bluetoothcachemode ; GattDeviceService.GetCharacteristicsForUuidAsync (capability `bluetooth`) https://learn.microsoft.com/en-us/uwp/api/windows.devices.bluetooth.genericattributeprofile.gattdeviceservice.getcharacteristicsforuuidasync ; GATT client guide https://learn.microsoft.com/en-us/windows/apps/develop/devices-sensors/gatt-client
* Bluetooth SIG Battery Service: https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/BAS_v1.1/out/en/index-en.html
* Windows HFP battery property in the wild: https://gist.github.com/muedsa/d5a94e2d5827d3b518063b9a2d2fdda3 , https://github.com/nikvoronin/xm4battery , Microsoft Q&A thread (device-dependence) https://learn.microsoft.com/en-in/answers/questions/1272102/how-to-get-bluetooth-battery-percentage-in-powersh
* BLE battery on Windows walk-through: https://www.andreasjakl.com/read-battery-level-bluetooth-le-devices/
* Google Fast Pair battery notification: https://developers.google.com/nearby/fast-pair/specifications/extensions/batterynotification
* btleplug WinRT device wrapper (cache fallback pattern): https://docs.rs/crate/btleplug/0.12.0/source/src/winrtble/ble/device.rs
* Local tests: this document, section 2.3.
