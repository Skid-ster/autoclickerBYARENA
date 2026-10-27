//! Virtual-key constants and validation for the configurable toggle hotkey.
//!
//! The module is platform-independent: it only describes Windows virtual-key
//! codes as plain integers so the rules and name mapping stay unit testable
//! on any host. The Win32 `GetAsyncKeyState` polling itself lives in
//! [`crate::win32`]. F8 is permanently reserved for the panic stop and can
//! never be assigned to the toggle action.

/// F6: the OP Auto Clicker 4.1 default start/stop key.
pub const VK_F6: i32 = 0x75;
/// F7: a common alternative start/stop binding.
pub const VK_F7: i32 = 0x76;
/// F8: permanently reserved for the panic stop action.
pub const VK_F8: i32 = 0x77;
/// Escape: cancels hotkey capture in the assignment dialog.
pub const VK_ESCAPE: i32 = 0x1B;

/// Default start/stop hotkey.
pub const DEFAULT_TOGGLE_VK: i32 = VK_F6;
/// Immutable panic-stop hotkey.
pub const PANIC_VK: i32 = VK_F8;

const VK_F1: i32 = 0x70;
const VK_F24: i32 = 0x87;
const VK_0: i32 = 0x30;
const VK_9: i32 = 0x39;
const VK_A: i32 = 0x41;
const VK_Z: i32 = 0x5A;
const VK_SPACE: i32 = 0x20;

/// Application-level hotkey preferences persisted between sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppPrefs {
    pub toggle_hotkey_vk: i32,
    pub hotkeys_enabled: bool,
}

impl Default for AppPrefs {
    fn default() -> Self {
        Self {
            toggle_hotkey_vk: DEFAULT_TOGGLE_VK,
            hotkeys_enabled: true,
        }
    }
}

/// True when the code may be assigned as the start/stop hotkey.
pub fn is_supported_toggle_vk(vk: i32) -> bool {
    if vk == PANIC_VK || vk == VK_ESCAPE {
        return false;
    }
    (VK_F1..=VK_F24).contains(&vk)
        || (VK_0..=VK_9).contains(&vk)
        || (VK_A..=VK_Z).contains(&vk)
        || vk == VK_SPACE
}

/// True only for the reserved panic-stop key.
pub fn is_reserved_panic_vk(vk: i32) -> bool {
    vk == PANIC_VK
}

/// Human-readable name shown in the hotkey dialog and the activity log.
pub fn vk_display_name(vk: i32) -> String {
    match vk {
        PANIC_VK => "F8".to_string(),
        VK_ESCAPE => "Esc".to_string(),
        VK_SPACE => "Space".to_string(),
        code if (VK_F1..=VK_F24).contains(&code) => {
            format!("F{}", code - VK_F1 + 1)
        }
        code if (VK_0..=VK_9).contains(&code) => char::from_u32((code - VK_0) as u32 + '0' as u32)
            .map(|c| c.to_string())
            .unwrap_or_else(|| format!("VK {code:#04X}")),
        code if (VK_A..=VK_Z).contains(&code) => char::from_u32((code - VK_A) as u32 + 'A' as u32)
            .map(|c| c.to_string())
            .unwrap_or_else(|| format!("VK {code:#04X}")),
        code => format!("VK {code:#04X}"),
    }
}

/// Every code the assignment dialog polls while listening for a key press.
pub fn capturable_vks() -> Vec<i32> {
    let mut codes: Vec<i32> = Vec::new();
    codes.extend(VK_F1..=VK_F24);
    codes.extend(VK_0..=VK_9);
    codes.extend(VK_A..=VK_Z);
    codes.push(VK_SPACE);
    codes
}
