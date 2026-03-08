#[cfg(target_os = "macos")]
pub use crate::mac_proc::MacProc as PlatformProc;
#[cfg(target_os = "windows")]
pub use crate::win_proc::WinProc as PlatformProc;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub struct PlatformProc;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
impl PlatformProc {
    pub fn new(_name: &str) -> anyhow::Result<Self> {
        anyhow::bail!("현재 OS는 지원되지 않습니다.");
    }

    pub async fn get_location(
        &mut self,
        _available_offsets: &Option<Vec<crate::offsets::WuwaOffset>>,
    ) -> Result<crate::types::PlayerInfo, crate::types::NativeError> {
        Err(crate::types::NativeError::PointerChainError {
            message: "현재 OS는 지원되지 않습니다.".to_string(),
        })
    }

    pub fn get_active_offset_name(&self) -> Option<String> {
        None
    }
}
