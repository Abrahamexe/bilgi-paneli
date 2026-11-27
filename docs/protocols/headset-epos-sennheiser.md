# EPOS / Sennheiser Gaming (H3PRO Hybrid, GSP 370) and Jabra GN (Link 380/390)

VID **0x1395** (EPOS / Sennheiser Communications). Tags: **[V]** verified in source (URL), **[I]** inferred, **[U]** unknown.
Source (GPL-3.0, facts only): https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/epos_h3pro_hybrid.hpp , `sennheiser_gsp370.hpp`, `protocols/jabra_gnp_protocol.hpp`, `jabra_link_380.hpp`, `jabra_link_390.hpp`.

## 1. EPOS H3PRO Hybrid (dongle 0x1395:0x0303; headset on USB cable 0x1395:0x0101) **[V]**

* Collection: **usage page 0xFFFF, usage 0x0001, interface 3**; **report ID 0x01, 106-byte reports** (107-byte buffer incl. ID) in both directions.
* Frame inside the report (all fields little-endian 16-bit): 
  `[0]=01 [1..2]=EB FE (magic) [3..4]=version 0x0001 [5..6]=type [7..8]=target [9..10]=payload length (bytes) [11..12]=CRC (low byte is CRC-8, high byte 00) [13..]=payload words: group, command, values...`
  * type **0x0004** = request and reply; 0x0110 = notification.
  * target **0x0001** = the plugged-in device (dongle or cabled headset); **0x0005 = the headset behind the dongle**; replies come back with target - 1 (0x0000 / 0x0004).
  * CRC-8: polynomial 0x07, initial value 0xFF, no reflection / no final xor, over the payload bytes.
* Commands (group/command): `01/09` dongle only: 1 while headset connected, 0 otherwise; **`12/03` battery percentage (steps of 20)**.
* **Battery request** (target 0x0005): `01 EB FE 01 00 04 00 05 00 04 00 A5 00 12 00 03 00` + zeros to 107 bytes (CRC-8 of `12 00 03 00` = 0xA5, computed by me **[I]**). Reply: same header with target 0x0004, payload `12 00 03 00 <percent lo> <percent hi>`; **percent = first value word (0..100, in steps of 20)**.
* **Headset-on check** (do this first, target 0x0001): `01 EB FE 01 00 04 00 01 00 04 00 7A 00 01 00 09 00`; reply value 1 = connected. If a request for target 0x0005 is answered with target 0x0001, **the headset is off / out of range**. **[V]** (CRC 0x7A computed by me **[I]**).
* Charging: not exposed. **[U]**
* While the headset is off the dongle returns headset-targeted requests unanswered with target 0x0001. **[V]**
* Other models of the same family (H3 Hybrid, H6 Pro, GSP 370/670/602/601/500/600, Adapt): **[U]**.

## 2. Sennheiser / EPOS GSP 370 (dongle 0x1395:0x009A) **[V]**

* HID interface 3 carries **only media keys**. Settings and battery use the dongle's **CDC-ACM serial port (interface 4) at 921600 baud**, packets framed like Bluetooth HCI vendor commands: `01 <opcode> FC <param length> <params...>`; reply `04 0E <length> 01 <opcode> FC <status> <values...>`; unrelated events `04 E9 00`, `04 E8 00` are to be skipped.
* **Battery: opcode 0x9C**, request `01 9C FC 00`; reply value = **percentage (0..100)**, one byte. **Status 0x0C = headset switched off** (all commands fail with 0x0C when off). **[V]**
* HC implements serial lookup on **Linux only**; on Windows the dongle's serial interface should show up as a COM port (Ports class, driver usbser) - **[I]** not tested; the HID path is useless for this model.
* Charging: none. **[U]**

## 3. Jabra GN Protocol (Link 380 / 390 dongles; Evolve2 65 Flex / Evolve3 75 over USB) **[V]**

Not requested, included because HC verified it on Windows (Evolve3 75).
* Collection: **vendor usage page 0xFF00**, report ID **0x05**, 64-byte; usage 0x05 on Link 390 / Evolve3 75, usage 0x01 on Link 380 / Evolve2 65 Flex USB. Interface 3.
* PIDs: Link 380 0x24C7 / 0x24C8 / 0x24C9 / 0x24CA; Link 390 0x2E50 / 0x2E51 / 0x2E56 / 0x2E57; Evolve2 65 Flex USB 0x2517 / 0x2518 / 0x2519; Evolve3 75 0x2546 / 0x2547 / 0x2548. (Jabra VID **0x0b0e** - **[I]**, not stated in the files I read.)
* Headset GNP address **0x04 behind a dongle, 0x08 when on USB**. Battery = status command **0x12 / sub 0x02 (HS_BATTERY)**, GET -> `[status bits][level %][adc hi][adc lo]...`; **status bit0 = charging, bit1 = low, bit6 = fast charging**. Exact GNP framing (dst/src/seq/len/cmd/sub offsets 1..6, payload from 7) in `jabra_gnp_protocol.hpp`. **[V]**

## 4. Sources
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/epos_h3pro_hybrid.hpp
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/sennheiser_gsp370.hpp
* https://github.com/Sapd/HeadsetControl/blob/master/lib/devices/protocols/jabra_gnp_protocol.hpp
