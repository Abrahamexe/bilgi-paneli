//! Driver registry. A driver supports one protocol *family*: it may list many
//! VID/PIDs from many brands (rebadged chipsets) and/or recognise the family
//! structurally (usage page, report lengths, strings) so unlisted brands that
//! use the same firmware work without code changes.
//!
//! Adding support = new file here + one line in [`DRIVERS`].
//! Protocol notes live in `docs/protocols/`.

use super::{Driver, HidInfo};

mod example;
mod logitech_hidpp;
mod razer;
mod steelseries_mouse;
mod corsair_mouse;
mod hyperx_mouse;
mod asus_rog_mouse;
mod glorious;
mod coolermaster_mouse;
mod mouse_io;
mod family_compx_report8;
mod family_compx_xvi;
mod family_pulsar_fusion2;
mod family_gearhub;
mod vaxee;
mod rapoo_vt;
mod mchose_mouse;
mod finalmouse;
mod ninjutso_incott_ajazz;
mod util;
mod hs;
mod headset_steelseries;
mod headset_hyperx;
mod headset_logitech;
mod headset_astro;
mod headset_corsair;
mod headset_audeze;
mod headset_sony_inzone;
mod headset_epos;
mod headset_other;
mod kb_util;
mod kb_rongyuan;
mod kb_aula_compx;
mod kb_aa_frame;
mod kb_sinowealth;
mod kb_mchose;
mod kb_nuphy;
mod kb_keychron;
mod kb_asus_rog;

/// All drivers. The first driver whose `matches` returns true for an interface
/// is used for that interface.
pub static DRIVERS: &[&dyn Driver] = &[&logitech_hidpp::LogitechHidpp, &razer::Razer, &steelseries_mouse::SteelSeriesMouse, &corsair_mouse::CorsairMouse, &hyperx_mouse::HyperxMouse, &asus_rog_mouse::AsusRogMouse, &glorious::Glorious, &coolermaster_mouse::CoolerMasterMouse, &family_compx_report8::CompxReport8, &family_compx_xvi::CompxXvi, &family_pulsar_fusion2::PulsarFusion2, &family_gearhub::GearHub, &vaxee::Vaxee, &rapoo_vt::RapooVt, &mchose_mouse::MchoseMouse, &finalmouse::Finalmouse, &ninjutso_incott_ajazz::NinjutsoAjazz, &headset_steelseries::SteelSeries, &headset_hyperx::HyperX, &headset_logitech::Logitech, &headset_astro::Astro, &headset_corsair::Corsair, &headset_audeze::Audeze, &headset_sony_inzone::SonyInzone, &headset_epos::Epos, &headset_other::OtherHeadsets, &example::Example, &kb_rongyuan::RongYuan, &kb_aula_compx::AulaCompx, &kb_aa_frame::AaFrame, &kb_sinowealth::SinoWealth, &kb_mchose::Mchose, &kb_nuphy::NuPhy, &kb_keychron::Keychron, &kb_asus_rog::AsusRog];

/// First driver claiming this interface.
pub fn find(info: &HidInfo) -> Option<&'static dyn Driver> {
    DRIVERS.iter().copied().find(|d| d.matches(info))
}

/// A (vid, pid) table helper. `pid == 0` in an entry means "any PID of that vendor".
pub fn id_listed(table: &[(u16, u16)], info: &HidInfo) -> bool {
    table.iter().any(|&(v, p)| v == info.vid && (p == 0 || p == info.pid))
}

/// Structural match on the interface that carries a vendor protocol.
pub fn usage_is(info: &HidInfo, page: u16, usage: u16) -> bool {
    info.usage_page == page && info.usage == usage
}
