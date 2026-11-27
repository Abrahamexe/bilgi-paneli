# Keyboard family: ASUS ROG (VID 0x0B05) wireless keyboards - Falchion RX Low Profile / Falchion, Azoth 96 HE, Claymore II

Legend: **V** verified in third-party source (URL), **I** inferred, **U** unknown. Same "0x12" command family as ASUS ROG headsets/mice (see headset-other-2g4.md if present). Wired-only ROG keyboards (Strix Scope, Claymore, Scope II) have no battery.

## 1. Identifiers
| Device | VID:PID | Collection | Conf |
|---|---|---|---|
| ROG Omni Receiver (Falchion RX LP 2.4 GHz) | 0B05:**1ACE** | usage page **0xFF02**, usage 1, input and output >= 64 bytes, **report ID 0x02** | V (Yng314/PeripheralBattery `src/main.mm`, macOS tray, works on hardware) |
| ROG SpeedNova 8K receiver (Azoth 96 HE 2.4 GHz) | 0B05:**1AD0** | usage page 0xFF00 / usage 1, `MI_02 & COL02`, 64/64 bytes, report ID 0x02 (avoid the mouse slot 0xFF01/1 on the same receiver) | V (FerrisPulse `AsusDeviceHelper.Rog.cs` notes) |
| ROG Azoth 96 HE wired | 0B05:**1C10** | `MI_01`, 65/65 B (report ID 0x00) | V (same) |
| ROG Falchion (original, 65 %) | wired/receiver PIDs not extracted | **U** | |
Report ID is device-defined: read it from HID caps.

## 2. Falchion RX LP battery (V, reproduced on hardware)
* Output report (64 B, report ID 0x02): `02 12 01 00 ... 00` = "get battery". Wake/ping: `02 12 00 ...` (send if no answer; the keyboard sleeps; retry up to 3 times, wait ~0.75 s after the battery request and ~0.2 s after the wake).
* Reply: input report ID 0x02: `02 12 01 ?? ?? ?? <percent> ...` accept only if `rep[0]==0x02 && rep[1]==0x12 && rep[2]==0x01`, **`rep[6]` = percent, valid 0..100**. Needs the receiver to be present; works only in 2.4 GHz mode (Bluetooth: U, use BAS).
* Charging flag: **U** for this device (receiver mode: cable charging disconnects the dongle link on some boards).

## 3. Azoth 96 HE (V, community-verified, FerrisPulse)
* Ready probe: `{rid, 0x12, 0x00}` -> reply is "ready" when `RX[5] != 0xFA`. Caveat: with the receiver plugged in the reply says ready (`RX[5]=0x01`) even when the keyboard is switched to wired/Bluetooth - do not infer online from it.
* Battery + charge: `{rid, 0x12, 0x01}` -> valid if `RX[5]==0 && RX[6]<=100 && RX[9]<=1`; **`RX[6]` = percent, `RX[9]==1` = charging**; `RX[12]` is a drifting current-like value (not battery). Sleep/standby frame = `02 FF AA`.
* Wired (1C10) answers the same pair with report ID 0x00 and 65-byte buffers; wired channel gives real charge status; when wired answers, treat the same-model 2.4G entry as offline (FerrisPulse arbitration).
* ROG headsets on the same stack use `{0xCC, 0x12, 0x07}` battery / `{0xCC, 0x12, 0x08}` charging (V) - not keyboards.

## 4. Polling rate
ROG Azoth/Falchion polling (1000/2000/4000/8000 on 8K receivers) is set in Armoury Crate - protocol **U**. Windows fallback: USB interval of the receiver/keyboard.

## 5. Model name
Receiver PID + keyboard USB product string for wired; behind receivers **U** (shared Omni receiver may serve mouse+keyboard: some Omni receivers can't list paired devices, see issue #3 of https://github.com/ZGMFX01A/Wireless-Device-Battery-Monitor).

## 6. Standard HID battery in ASUS-branded Bluetooth keyboards (kernel hid-asus)
For ASUS *Bluetooth* keyboards (not the ROG ones above) Linux's `hid-asus.c` reads **input report ID 0x03, 9 bytes**: `[1]` = level 0..255 (percent = level*100/255), `[2..7]` = BT MAC, `[8]` = status 0 discharging / 1 charging / 2 full, obtained via GET_REPORT(input, id 3). V: https://github.com/torvalds/linux/blob/master/drivers/hid/hid-asus.c . Applicability to ROG boards: U.

## Sources
* https://github.com/Yng314/PeripheralBattery (src/main.mm `queryRogFalchionBattery`)
* https://github.com/Ferris-echo/FerrisPulse/blob/main/docs/HARDWARE_PROTOCOLS.md section 2.3 #8
* https://github.com/torvalds/linux/blob/master/drivers/hid/hid-asus.c
