# Family D: Attack Shark / Delux "X11-style" (VID 0x1d57), GearHub MU (0x25a7) and GearHub-V5 (0x3151: Fantech, Lingbao, Attack Shark R2)

Three related low-cost OEM stacks sold as Attack Shark, Delux, Fantech, Lingbao, Redragon (M719). Battery is mostly **push-based**, so a tray app mostly listens.

Confidence: V verified in source, HW hardware-confirmed per source, I inferred, U unknown.

## 1. D1: VID 0x1d57 "X11 family" (Beken)

Models: Attack Shark X11, R1, X3, X3 Max/Pro, X6, X1, X8 SE/Plus/Pro, G3/G3 Pro, V3/V3 Pro/V6, X11 SE/Pro; Delux M800 Mini, M600 Pro. PIDs are shared and do not name the model: wired 0xfa55 (X11, M800 Mini), 0xfa61 (R1), 0xfa71 (M600 Pro wired); 2.4G receiver **0xfa60**. V

* The wireless receiver's **interface 2** pushes an input report on its own, no request: 5 bytes `03 <model> 40 01 <p2>`:
  * `[0]` = 0x03 report ID, `[1]` = model id of the paired mouse (X11 0x55, R1 0x10, X6 0x85, X3 0x4D, X3 Max 0x4E, X3 Pro 0x01, X11 SE 0x07, X11 Pro 0xBE, X1 0xB1, X8 SE 0x02, X8 Plus 0x06, X8 Pro 0x08, G3 0x04, G3 Pro 0x0E, V6 0x24, V3 Pro 0xE5, V3 0xEA, Delux M600 Pro 0x20), `[2]` = 0x40 event "battery", `[3]` = 0x01, `[4]` = value.
  * Percent = value for "percent" models (X11, X11 SE, Delux M600 Pro); value*10 for "tenths" models (R1, X6; value <= 10). Others named but the scale is unchecked. V, HW (X11 id 0x55, M600 Pro)
* Interface 2 declares battery collection usagePage 0x0A (input report 3) and config collection usagePage 0x0B (feature reports 0x04 DPI, 0x06 polling). On Windows Chrome cannot open them; native HID (node-hid with stock input.inf, or hidapi opening the `&col04` sub-collection) can. For Win32: open the 0x0A top-level collection read-only and wait up to ~2.5 s for the next push (wait cap used by OpenMouse; cache 30 s). Wired PIDs never push battery. V
* Charging state: **not reported** on this path (OpenMouse hard-codes "Discharging"). U
* Polling: feature report 0x06, 9 bytes `[len=0x09, 0x01, rate, checksum, 0,0,0,0]` write only; codes 0x08=125, 0x04=250, 0x02=500, 0x01=1000. The firmware has **no read-back**; everyone defaults to 1000 Hz. Current rate: not readable. U
* Quirk: two config writes 0.4-0.5 s apart through the receiver froze the link until replugged; 1-3 s gaps were fine (read-only apps unaffected).
* Real model: model id byte in the push report (table above).

## 2. D2: VID 0x25a7 (Compx/Areson) "GearHub MU class"

Attack Shark X3/X6/X8/X11 "direct", Redragon M719 Pro (0xfab0 cable / 0xfa7e receiver, command collection 0xFF02:0x0002 + notify 0xFF01:0x0001). Pulsar X2 gen1 also uses 0x25a7 (fa7b/fa7c) but speaks Family A variant A2.

* Feature reports, report ID 0, 64 bytes, commands padded to 9 bytes, checksum in byte[7]. Command ids (MU class): 0x80 get firmware, 0xD4 get DPI, 0x04 set report rate. Polling codes {125:0x08, 250:0x04, 500:0x02, 1000:0x01, 2000:0x84, 4000:0x82, 8000:0x81}. V (OpenMouse transcription of qmk.top GearHub bundle)
* Battery: not decoded by OpenMouse for this protocol. U

## 3. D3: GearHub-V5 on VID 0x3151 ("MicLink/mlzn" ODM)

Lingbao M5 Pro (PAW3395, device id 2285), Attack Shark R2 (PAW3950, 1893), Fantech WG14P Yari Pro 8K (0x3151:0x503d). Product ids settle only the transport: 0x402D 2.4G receiver, 0x4026 direct cable, 0x402C Bluetooth (AJ179 Pro, usagePage 0xFF55 usage 0x0202, report 6 wrapped in 0x55). Not Fantech's 0x402D "Family B" variant (zeros). V, HW (M5 Pro, R2 on receiver 0x402D)

Feature report ID 0, 64 bytes.
* **Battery**: send raw (no checksum) `[0xF7]` (GET_STATUS), wait ~10 ms, GET_FEATURE. Reply (measured `01 00 2D 01 00 01`): `[0]`=1 ready to read, `[1]` keyboard battery, `[2]` **mouse battery** (0x2D = 45%, plain value), `[3]` keyboard online (0 = online), `[4]` mouse online (0 = online), `[5]` ready to send. Charging: not exposed. V, HW
* Commands to the mouse need first `SELECT_TARGET` = `[0xF6, 0x05]` (wait 50 ms) then 9-byte frames with checksum `byte[7] = 255 - sum(bytes[0..6])`.
* Polling: OPTIONPARAM0 (GET 0xD3) 64-byte block, report-rate byte **offset 9**: 129 = 8000, 130 = 4000, 132 = 2000, 1 = 1000, 2 = 500, 4 = 250, 8 = 125 (hardware: R2 read code 1 at 1000 Hz). Direct (cable) ceiling 1000 Hz. V, HW
* Real model: GET_USB_VERSION 0x8F "device id" (e.g. 2285, 1893).
* Fantech WG14P "Family B" (different report-rate map: 0=8000, 1=4000, 2=2000, 3=1000, 4=500, 5=125; commands GET_REPORT_RATE 131, SET 8, GET_ALL_PARAMS 159) answers on usagePage 0xFFFF usage 2 but the **receiver 0x402D returned 64 zero bytes** to those; do not trust any value unless the reply is non-zero. Battery: none in that decoder.

## 4. Sources

* https://unpkg.com/@openmouse/protocol@0.60.0/dist/drivers/attackshark/hid.js, delux/index.js, gearhub/index.js, drivers/gearhub/hid.js, drivers/fantech/hid.js
* Referenced upstream projects (not read): xb-bx/attack-shark-r1-driver, HarukaYamamoto0/attack-shark-x11-driver, qmk.top GearHub-V5 bundle
* Delux M600 Pro / model ids: OpenMouse docs/delux-m600-pro-testing.md (as cited in source comments)
