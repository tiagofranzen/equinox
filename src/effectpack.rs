//! "Equinox" in the Audio enhancements box of Sound settings.
//!
//! Since 24H2, that box lists Windows effect packs (Voice Clarity is one) next
//! to the device's own effects. An effect pack is a small driver package: a
//! software component of class AudioProcessingObject whose registry key holds
//! an `EffectPackRegistration` entry naming the APO to run. equinox installs
//! one that runs Equalizer APO's post-mix APO, so picking "Equinox" replaces
//! every other effect on the microphone with the EQ.
//!
//! Windows only installs signed packages. There is no binary in this one (it
//! points at Equalizer APO's DLL), so each PC signs its own copy: a fresh
//! certificate is made, used once, trusted on this machine only, and its
//! private key deleted, so nothing else can ever be signed with it.
//!
//! The name shown is `Equinox — <EQ name>`. Users may rewrite that one value
//! (and nothing else in the package's key), so equinox updates it without a
//! UAC prompt. Windows reads it when the audio service starts, so a new name
//! shows after the next restart.

use winreg::RegKey;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_READ, KEY_SET_VALUE};

/// The effect pack's id: what the Audio enhancements box stores when it is picked.
pub const PACK: &str = "{0B4D7114-EBD7-47CC-95CE-FE23DFD0F4F6}";
/// The software component the pack lives on.
const COMPONENT: &str = r"SWC\VEN_EQNX&AUDIO_EFFECTPACK_EQUINOX";
/// Driver keys of APO components.
const APO_CLASS: &str = r"SYSTEM\CurrentControlSet\Control\Class\{5989fce8-9cd0-467d-8a6a-5419e31529d4}";
/// PKEY_ItemNameDisplay: the name in the box.
const NAME: &str = "{B725F130-47EF-101A-A5F1-02608C9EEBAC},10";
/// First Windows build with effect packs (24H2).
const MIN_BUILD: u32 = 26100;
pub const BASE_NAME: &str = "Equinox";

/// Attaches the component to the computer device, as Voice Clarity's does.
const EXT_INF: &str = r#"[Version]
Signature   = "$WINDOWS NT$"
Class       = Extension
ClassGuid   = {e2f84ce7-8efa-411c-aa69-97454ca4cb57}
Provider    = %Prov%
ExtensionId = {4F5A83F6-0104-4EE4-A2B5-E6AA7F6F38BE}
CatalogFile = equinox-ext.cat
DriverVer   = 11/26/2026,1.0.0.0
PnpLockDown = 1

[Manufacturer]
%Prov% = Eqx,NTamd64

[Eqx.NTamd64]
%Desc% = Eqx_Install,COMPUTER\Generic

[Eqx_Install]

[Eqx_Install.Components]
AddComponent = EquinoxEffectPack,,Eqx_Component

[Eqx_Component]
ComponentIDs = VEN_EQNX&AUDIO_EFFECTPACK_EQUINOX

[Strings]
Prov = "Equinox"
Desc = "Equinox Effect Pack"
"#;

/// The component itself: registers the effect pack for microphones.
const FX_INF: &str = r#"[Version]
Signature   = "$WINDOWS NT$"
Class       = AudioProcessingObject
ClassGuid   = {5989fce8-9cd0-467d-8a6a-5419e31529d4}
Provider    = %Prov%
CatalogFile = equinox-fx.cat
DriverVer   = 11/26/2026,1.0.0.0
PnpLockDown = 1

[Manufacturer]
%Prov% = EqxFx,NTamd64

[EqxFx.NTamd64]
%Desc% = EqxFx_Install, SWC\VEN_EQNX&AUDIO_EFFECTPACK_EQUINOX

[EqxFx_Install.NTamd64]
AddReg = EqxFx_AddReg

[EqxFx_Install.NTamd64.Services]
AddService = ,2

[EqxFx_AddReg]
HKR,,FriendlyName,,%Desc%
HKR,Classes\CLSID\%APO%,,,"EqualizerAPO Post-Mix Class"
HKR,Classes\CLSID\%APO%\InProcServer32,,%REG_EXPAND_SZ%,"%16422%\EqualizerAPO\EqualizerAPO.dll"
HKR,Classes\CLSID\%APO%\InProcServer32,ThreadingModel,,"Both"
HKR,AudioEngine\AudioProcessingObjects\%APO%,"FriendlyName",,%Name%
HKR,AudioEngine\AudioProcessingObjects\%APO%,"Copyright",,%Author%
HKR,AudioEngine\AudioProcessingObjects\%APO%,"MajorVersion",%REG_DWORD%,1
HKR,AudioEngine\AudioProcessingObjects\%APO%,"MinorVersion",%REG_DWORD%,0
HKR,AudioEngine\AudioProcessingObjects\%APO%,"Flags",%REG_DWORD%,0xd
HKR,AudioEngine\AudioProcessingObjects\%APO%,"MinInputConnections",%REG_DWORD%,1
HKR,AudioEngine\AudioProcessingObjects\%APO%,"MaxInputConnections",%REG_DWORD%,1
HKR,AudioEngine\AudioProcessingObjects\%APO%,"MinOutputConnections",%REG_DWORD%,1
HKR,AudioEngine\AudioProcessingObjects\%APO%,"MaxOutputConnections",%REG_DWORD%,1
HKR,AudioEngine\AudioProcessingObjects\%APO%,"MaxInstances",%REG_DWORD%,0xffffffff
HKR,AudioEngine\AudioProcessingObjects\%APO%,"NumAPOInterfaces",%REG_DWORD%,1
HKR,AudioEngine\AudioProcessingObjects\%APO%,"APOInterface0",,"{FD7F2B29-24D0-4B5C-B177-592C39F9CA10}"

HKR,EffectPackRegistration\%PACK%,"{D04E05A6-594B-4FB6-A80D-01AF5EED7D1D},29",,"{7abf23d9-727e-4d0b-86a3-dd501d260101}"
HKR,EffectPackRegistration\%PACK%,"{B725F130-47EF-101A-A5F1-02608C9EEBAC},10",,%Name%
HKR,EffectPackRegistration\%PACK%,"{D04E05A6-594B-4FB6-A80D-01AF5EED7D1D},26",,%Author%
HKR,EffectPackRegistration\%PACK%,"{D04E05A6-594B-4FB6-A80D-01AF5EED7D1D},24",%REG_DWORD%,1
HKR,EffectPackRegistration\%PACK%,"{D04E05A6-594B-4FB6-A80D-01AF5EED7D1D},25",%REG_DWORD%,0
HKR,EffectPackRegistration\%PACK%\FxProperties,"{D04E05A6-594B-4FB6-A80D-01AF5EED7D1D},33",%REG_DWORD%,1
HKR,EffectPackRegistration\%PACK%\FxProperties,"{D04E05A6-594B-4FB6-A80D-01AF5EED7D1D},23",%REG_MULTI_SZ%,"*"
HKR,EffectPackRegistration\%PACK%\FxProperties,"{D04E05A6-594B-4fb6-A80D-01AF5EED7D1D},0",%REG_MULTI_SZ%,"{DFF21BE1-F70F-11D0-B917-00A0C9223196}","{DFF21BE2-F70F-11D0-B917-00A0C9223196}","{DFF21BE3-F70F-11D0-B917-00A0C9223196}","{DFF21BE4-F70F-11D0-B917-00A0C9223196}","{DFF21BE5-F70F-11D0-B917-00A0C9223196}","{DFF21BE6-F70F-11D0-B917-00A0C9223196}","{DFF21DE2-F70F-11D0-B917-00A0C9223196}","{A79D957D-5CD5-458A-B3F1-AF8E789B1F3B}"
HKR,EffectPackRegistration\%PACK%\FxProperties,"{D04E05A6-594B-4FB6-A80D-01AF5EED7D1D},6",,%APO%
HKR,EffectPackRegistration\%PACK%\FxProperties,"{D04E05A6-594B-4fb6-A80D-01AF5EED7D1D},14",%REG_MULTI_SZ%,%APO%
HKR,EffectPackRegistration\%PACK%\FxProperties,"{D3993A3F-99C2-4402-B5EC-A92A0367664B},6",%REG_MULTI_SZ%,"{C18E2F7E-933D-4965-B7D1-1EEF228D2AF3}","{98951333-B9CD-48B1-A0A3-FF40682D73F7}","{FC1CFC9B-B9D6-4CFA-B5E0-4BB2166878B2}","{9E90EA20-B493-4FD1-A1A8-7E1361A956CF}"

[Strings]
Prov   = "Equinox"
Desc   = "Equinox"
Name   = "Equinox"
Author = "Equinox"
PACK   = "{0B4D7114-EBD7-47CC-95CE-FE23DFD0F4F6}"
APO    = "{EC1CC9CE-FAED-4822-828A-82A81A6F018F}"
REG_MULTI_SZ  = 0x00010000
REG_DWORD     = 0x00010001
REG_EXPAND_SZ = 0x00020000
"#;

/// Sign both catalogs with a throwaway certificate trusted on this PC only,
/// install the packages, and let users rename the pack. `$dir` holds the INFs.
const INSTALL_PS: &str = r#"
$ErrorActionPreference = 'Stop'
$dir = $args[0]
$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=Equinox effect pack (this PC only)' -CertStoreLocation Cert:\LocalMachine\My -NotAfter (Get-Date).AddYears(30)
try {
  foreach ($n in 'fx','ext') {
    New-FileCatalog -Path "$dir\$n\equinox-$n.inf" -CatalogFilePath "$dir\$n\equinox-$n.cat" -CatalogVersion 2 | Out-Null
    $s = Set-AuthenticodeSignature "$dir\$n\equinox-$n.cat" -Certificate $cert -HashAlgorithm SHA256
    if ($s.Status -ne 'Valid' -and $s.Status -ne 'UnknownError') { throw "signing failed: $($s.StatusMessage)" }
  }
  Export-Certificate -Cert $cert -FilePath "$dir\pc.cer" | Out-Null
  foreach ($store in 'Root','TrustedPublisher') { Import-Certificate -FilePath "$dir\pc.cer" -CertStoreLocation "Cert:\LocalMachine\$store" | Out-Null }
} finally {
  Remove-Item "Cert:\LocalMachine\My\$($cert.Thumbprint)" -DeleteKey -ErrorAction SilentlyContinue
}
& pnputil.exe /add-driver "$dir\fx\equinox-fx.inf" /install | Out-Null
& pnputil.exe /add-driver "$dir\ext\equinox-ext.inf" /install | Out-Null
$key = $null
for ($i = 0; $i -lt 50 -and -not $key; $i++) {
  Get-ChildItem 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{5989fce8-9cd0-467d-8a6a-5419e31529d4}' -ErrorAction SilentlyContinue | ForEach-Object {
    if ((Get-ItemProperty $_.PSPath).MatchingDeviceId -eq 'SWC\VEN_EQNX&AUDIO_EFFECTPACK_EQUINOX') { $key = "$($_.PSPath)\EffectPackRegistration\{0B4D7114-EBD7-47CC-95CE-FE23DFD0F4F6}" }
  }
  if (-not $key) { Start-Sleep -Milliseconds 200 }
}
if (-not $key) { throw 'the effect pack did not install' }
$acl = Get-Acl $key
$users = New-Object Security.Principal.SecurityIdentifier 'S-1-5-32-545'
$acl.AddAccessRule((New-Object Security.AccessControl.RegistryAccessRule($users, 'QueryValues,SetValue', 'None', 'None', 'Allow')))
Set-Acl $key $acl
"#;

fn windows_build() -> u32 {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", KEY_READ)
        .and_then(|k| k.get_value::<String, _>("CurrentBuildNumber"))
        .ok()
        .and_then(|b| b.parse().ok())
        .unwrap_or(0)
}

/// This Windows can list effect packs and this build can install ours.
pub fn supported() -> bool {
    cfg!(target_arch = "x86_64") && windows_build() >= MIN_BUILD
}

/// Registry path (under HKLM) of the installed pack's registration.
fn registration() -> Option<String> {
    let class = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(APO_CLASS, KEY_READ).ok()?;
    class.enum_keys().flatten().find_map(|sub| {
        let k = class.open_subkey_with_flags(&sub, KEY_READ).ok()?;
        let id: String = k.get_value("MatchingDeviceId").ok()?;
        id.eq_ignore_ascii_case(COMPONENT)
            .then(|| format!(r"{APO_CLASS}\{sub}\EffectPackRegistration\{PACK}"))
    })
}

pub fn installed() -> bool {
    registration().is_some()
}

/// Install the effect pack. Needs admin; a no-op when it is already there.
pub fn install() -> Result<(), String> {
    if installed() {
        return Ok(());
    }
    if !supported() {
        return Err("needs 64-bit Windows 11 24H2 or later".into());
    }
    let dir = std::env::temp_dir().join(format!("equinox-pack-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let write = |sub: &str, name: &str, text: &str| -> std::io::Result<()> {
        std::fs::create_dir_all(dir.join(sub))?;
        std::fs::write(dir.join(sub).join(name), text)
    };
    write("fx", "equinox-fx.inf", FX_INF)
        .and_then(|_| write("ext", "equinox-ext.inf", EXT_INF))
        .and_then(|_| std::fs::write(dir.join("install.ps1"), INSTALL_PS))
        .map_err(|e| e.to_string())?;
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(dir.join("install.ps1"))
        .arg(&dir)
        .creation_flags_hidden()
        .output();
    let _ = std::fs::remove_dir_all(&dir);
    let out = out.map_err(|e| format!("couldn't run PowerShell: {e}"))?;
    if !installed() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("unknown error");
        return Err(format!("Windows didn't take the effect pack: {line}"));
    }
    Ok(())
}

/// Hide the console window of a child process.
trait Hidden {
    fn creation_flags_hidden(&mut self) -> &mut Self;
}

impl Hidden for std::process::Command {
    fn creation_flags_hidden(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
    }
}

/// The name to show for an EQ called `eq` (none: just "Equinox").
pub fn display_name(eq: Option<&str>) -> String {
    match eq.map(str::trim).filter(|s| !s.is_empty()) {
        Some(eq) => format!("{BASE_NAME} \u{2014} {eq}"),
        None => BASE_NAME.to_string(),
    }
}

/// Rename the pack. No admin needed; Windows shows it after the audio service
/// next starts. Returns whether the stored name changed.
pub fn set_name(name: &str) -> Result<bool, String> {
    let path = registration().ok_or("the effect pack isn't installed")?;
    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(&path, KEY_QUERY_VALUE | KEY_SET_VALUE)
        .map_err(|e| e.to_string())?;
    if key.get_value::<String, _>(NAME).is_ok_and(|n| n == name) {
        return Ok(false);
    }
    key.set_value(NAME, &name).map_err(|e| e.to_string())?;
    Ok(true)
}

/// What a chain is called: its profile and/or correction, e.g. "Warm + SoloCast".
pub fn eq_name(state: &crate::model::State, chain: &crate::model::Chain) -> Option<String> {
    let profile = chain
        .profile
        .and_then(|id| state.profiles.iter().find(|p| p.id == id))
        .map(|p| p.name.trim().to_string());
    let correction = chain.correction.as_ref().map(|c| c.name.trim().to_string());
    let parts: Vec<String> = profile.into_iter().chain(correction).filter(|s| !s.is_empty()).collect();
    (!parts.is_empty()).then(|| parts.join(" + "))
}

/// Keep the pack's name on the EQ of the microphone running it (the first
/// one with "Equinox" selected). Quietly does nothing without the pack.
pub fn sync_name(state: &crate::model::State) {
    if !installed() {
        return;
    }
    let eq = if state.mic_bypass {
        Some("Off".to_string())
    } else {
        state
            .mics
            .iter()
            .find(|m| crate::enhance::equinox_selected(&m.id, true))
            .and_then(|m| eq_name(state, &m.chain))
    };
    let _ = set_name(&display_name(eq.as_deref()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(display_name(None), "Equinox");
        assert_eq!(display_name(Some("  ")), "Equinox");
        assert_eq!(display_name(Some("Warm")), "Equinox \u{2014} Warm");
    }

    #[test]
    fn inf_matches_constants() {
        assert!(FX_INF.contains(PACK));
        assert!(FX_INF.contains(COMPONENT));
        assert!(EXT_INF.contains(COMPONENT.trim_start_matches(r"SWC\")));
    }
}
