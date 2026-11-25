//! Windows 11 "audio enhancements" and why a microphone can ignore the EQ.
//!
//! Since 24H2, Windows attaches its own effect pack (Voice Clarity) to
//! microphones whose driver has none, which is nearly every USB microphone. An
//! endpoint runs either that pack or its own effects, never both, and
//! Equalizer APO is one of the endpoint's own effects. So while Voice Clarity
//! is the selected enhancement, Equalizer APO is installed but never loaded.
//!
//! The choice is the "Audio enhancements" box in Sound settings. It is stored
//! as an endpoint property, and can be changed without admin rights through
//! the audio service (the same route the Settings app uses).

// The interface below keeps the names Windows gives its methods.
#![allow(non_snake_case)]

use std::ffi::c_void;

use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    ERole, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eCommunications, eConsole, eMultimedia, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
use windows::core::{BOOL, GUID, HRESULT, HSTRING, IUnknown, IUnknown_Vtbl, PCWSTR, interface};
use winreg::RegKey;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};

/// Endpoint property set holding the selected enhancement: `,2` is its id as
/// text, `,3` the same id with three flags.
const SELECTION: GUID = GUID::from_u128(0xc876062a_a276_4ed6_b15b_3962e69007f8);
const SELECTION_ID: &str = "{c876062a-a276-4ed6-b15b-3962e69007f8},2";
/// "Device Default Effects": the endpoint's own effects, where Equalizer APO lives.
const DEVICE_EFFECTS: GUID = GUID::from_u128(0x00d5b26c_4d0f_4a83_bb69_c2f12dc5fb10);
const DEVICE_EFFECTS_ID: &str = "{00D5B26C-4D0F-4A83-BB69-C2F12DC5FB10}";
/// Set in FxProperties when a Windows effect pack is attached to the endpoint.
const EFFECT_PACK: &str = "{9e6136e0-57ab-4949-b57a-3627be142855},100";

const VT_LPWSTR: u16 = 31;
const VT_BLOB: u16 = 65;

fn endpoint(guid: &str, capture: bool) -> Option<RegKey> {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(format!(r"{}\{guid}", crate::devices::branch(capture)), KEY_READ)
        .ok()
}

/// The Windows effect pack attached to this endpoint ("…VocaEffectPack…" is
/// Voice Clarity), if any.
pub fn effect_pack(guid: &str, capture: bool) -> Option<String> {
    endpoint(guid, capture)?
        .open_subkey_with_flags("FxProperties", KEY_READ)
        .ok()?
        .get_value::<String, _>(EFFECT_PACK)
        .ok()
        .filter(|s| !s.is_empty())
}

/// The selected enhancement's id, when one was ever chosen.
pub fn selected(guid: &str, capture: bool) -> Option<String> {
    endpoint(guid, capture)?
        .open_subkey_with_flags("Properties", KEY_READ)
        .ok()?
        .get_value::<String, _>(SELECTION_ID)
        .ok()
}

/// A Windows effect pack is running in place of the endpoint's own effects,
/// so Equalizer APO is not being loaded for it. With a pack attached and
/// nothing chosen yet, Windows uses the pack.
pub fn overridden(guid: &str, capture: bool) -> bool {
    effect_pack(guid, capture).is_some() && !selected(guid, capture).is_some_and(|s| s.eq_ignore_ascii_case(DEVICE_EFFECTS_ID))
}

/// A name for the pack that is in the way, for messages.
pub fn pack_name(guid: &str, capture: bool) -> &'static str {
    match effect_pack(guid, capture) {
        Some(p) if p.to_ascii_lowercase().contains("voca") => "Windows Voice Clarity",
        _ => "a Windows audio enhancement",
    }
}

/// The part of the audio service's private configuration interface used here.
/// Its layout has been stable since Windows 7; only `SetPropertyValue` is
/// called, the other slots just keep it at the right position.
#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    unsafe fn GetMixFormat(&self, id: PCWSTR, format: *mut *mut c_void) -> HRESULT;
    unsafe fn GetDeviceFormat(&self, id: PCWSTR, default: i32, format: *mut *mut c_void) -> HRESULT;
    unsafe fn ResetDeviceFormat(&self, id: PCWSTR) -> HRESULT;
    unsafe fn SetDeviceFormat(&self, id: PCWSTR, endpoint: *const c_void, mix: *const c_void) -> HRESULT;
    unsafe fn GetProcessingPeriod(&self, id: PCWSTR, default: i32, period: *mut i64, minimum: *mut i64) -> HRESULT;
    unsafe fn SetProcessingPeriod(&self, id: PCWSTR, period: *const i64) -> HRESULT;
    unsafe fn GetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
    unsafe fn SetShareMode(&self, id: PCWSTR, mode: *const c_void) -> HRESULT;
    unsafe fn GetPropertyValue(&self, id: PCWSTR, fx_store: BOOL, key: *const PROPERTYKEY, value: *mut PropVariant) -> HRESULT;
    unsafe fn SetPropertyValue(&self, id: PCWSTR, fx_store: BOOL, key: *const PROPERTYKEY, value: *const PropVariant) -> HRESULT;
    unsafe fn SetDefaultEndpoint(&self, id: PCWSTR, role: ERole) -> HRESULT;
    unsafe fn SetEndpointVisibility(&self, id: PCWSTR, visible: BOOL) -> HRESULT;
}

const POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// PROPVARIANT, as far as the two value types written here need it.
#[repr(C)]
struct PropVariant {
    vt: u16,
    reserved: [u16; 3],
    value: PropValue,
}

#[repr(C)]
union PropValue {
    text: *const u16,
    blob: Blob,
    // PROPVARIANT's data is two pointers wide.
    _size: [usize; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Blob {
    len: u32,
    data: *const u8,
}

/// What `,3` holds: the enhancement's id and three flags. For the endpoint's
/// own effects Windows writes 0, 0, 1.
#[repr(C)]
struct Choice {
    id: GUID,
    flags: [u32; 3],
}

fn set(config: &IPolicyConfig, id: &HSTRING, pid: u32, value: &PropVariant) -> Result<(), String> {
    let key = PROPERTYKEY { fmtid: SELECTION, pid };
    unsafe { config.SetPropertyValue(PCWSTR(id.as_ptr()), false.into(), &key, value) }
        .ok()
        .map_err(|e| format!("Windows refused to change the audio enhancement ({})", e.message()))
}

fn policy_config() -> Result<IPolicyConfig, String> {
    crate::sys::com_init();
    unsafe { CoCreateInstance(&POLICY_CONFIG_CLIENT, None, CLSCTX_ALL) }
        .map_err(|e| format!("couldn't reach the Windows audio service ({})", e.message()))
}

/// Select "Device Default Effects" for an endpoint, so its own effects
/// (Equalizer APO) run instead of a Windows effect pack. Needs no admin
/// rights. Windows only acts on it when it next builds the endpoint's effect
/// chain: after [`reload`], or an audio service restart.
pub fn use_device_effects(guid: &str, capture: bool) -> Result<(), String> {
    select(guid, capture, DEVICE_EFFECTS, [0, 0, 1])
}

/// Make Windows rebuild an endpoint's effect chain now, by switching the
/// endpoint off and on again (what "Disable" and "Enable" do in Sound
/// settings). Apps using it see it vanish for a moment and reconnect; if it
/// was a default device it is made the default again.
pub fn reload(guid: &str, capture: bool) -> Result<(), String> {
    let config = policy_config()?;
    let id = HSTRING::from(crate::sys::endpoint_id(guid, capture));
    let id = PCWSTR(id.as_ptr());
    let roles = [eConsole, eMultimedia, eCommunications];
    let was_default: Vec<ERole> = unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| e.message())?;
        roles
            .into_iter()
            .filter(|&role| {
                en.GetDefaultAudioEndpoint(if capture { eCapture } else { eRender }, role)
                    .and_then(|d| d.GetId())
                    .is_ok_and(|p| {
                        let same = p
                            .to_string()
                            .is_ok_and(|s| crate::sys::endpoint_guid(&s) == guid.to_ascii_lowercase());
                        CoTaskMemFree(Some(p.0 as _));
                        same
                    })
            })
            .collect()
    };
    let switch = |on: bool| {
        unsafe { config.SetEndpointVisibility(id, on.into()) }
            .ok()
            .map_err(|e| format!("Windows refused to restart the device ({})", e.message()))
    };
    switch(false)?;
    std::thread::sleep(std::time::Duration::from_millis(400));
    // Whatever happens next, don't leave the device switched off.
    let on = switch(true);
    std::thread::sleep(std::time::Duration::from_millis(400));
    for role in was_default {
        unsafe {
            let _ = config.SetDefaultEndpoint(id, role);
        }
    }
    on
}

/// Select an enhancement by id, with the flags Windows stores next to it.
fn select(guid: &str, capture: bool, enhancement: GUID, flags: [u32; 3]) -> Result<(), String> {
    let config = policy_config()?;
    let id = HSTRING::from(crate::sys::endpoint_id(guid, capture));
    let name = HSTRING::from(format!("{{{enhancement:?}}}").to_ascii_uppercase());
    let choice = Choice { id: enhancement, flags };
    let text = PropVariant {
        vt: VT_LPWSTR,
        reserved: [0; 3],
        value: PropValue { text: name.as_ptr() },
    };
    let blob = PropVariant {
        vt: VT_BLOB,
        reserved: [0; 3],
        value: PropValue {
            blob: Blob {
                len: std::mem::size_of::<Choice>() as u32,
                data: &choice as *const Choice as *const u8,
            },
        },
    };
    set(&config, &id, 3, &blob)?;
    set(&config, &id, 2, &text)
}

/// When a Windows effect pack is in the way: select the endpoint's own effects
/// and reload the endpoint so that takes hold. Returns whether anything was changed.
pub fn fix(guid: &str, capture: bool) -> Result<bool, String> {
    if !overridden(guid, capture) {
        return Ok(false);
    }
    use_device_effects(guid, capture)?;
    reload(guid, capture)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_match_windows() {
        // PROPVARIANT: an 8-byte header and a two-pointer payload.
        assert_eq!(std::mem::size_of::<PropVariant>(), 8 + 2 * std::mem::size_of::<usize>());
        // The selection blob Windows writes is the id plus three 32-bit flags.
        assert_eq!(std::mem::size_of::<Choice>(), 28);
        assert_eq!(
            format!("{DEVICE_EFFECTS:?}").to_ascii_uppercase(),
            DEVICE_EFFECTS_ID.trim_matches(['{', '}'])
        );
    }
}
