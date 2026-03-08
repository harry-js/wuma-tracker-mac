use std::f32::consts::PI;
use std::ffi::CStr;
use std::mem;

use anyhow::{Context, Result, bail};
use libc::{c_int, c_void};

use crate::offsets::WuwaOffset;
use crate::types::NativeError::{PointerChainError, ValueReadError};
use crate::types::{FIntVector, FTransformDouble, NativeError, PlayerInfo};

type KernReturnT = c_int;
type MachPortT = u32;
type MachVmAddressT = u64;
type MachVmSizeT = u64;
type MachMsgTypeNumberT = u32;
type VmRegionFlavorT = c_int;
type TaskFlavorT = c_int;
type TaskInfoT = *mut c_int;

const KERN_SUCCESS: KernReturnT = 0;
const VM_REGION_BASIC_INFO_64: VmRegionFlavorT = 9;
const VM_REGION_BASIC_INFO_COUNT_64: MachMsgTypeNumberT = 9;
const VM_PROT_EXECUTE: i32 = 0x04;
const TASK_DYLD_INFO: TaskFlavorT = 17;
const TASK_DYLD_INFO_COUNT: MachMsgTypeNumberT = 5;

#[repr(C)]
struct VmRegionBasicInfoData64 {
    protection: i32,
    max_protection: i32,
    inheritance: u32,
    shared: u32,
    reserved: u32,
    offset: u64,
    behavior: u32,
    user_wired_count: u16,
}

#[repr(C)]
struct TaskDyldInfo {
    all_image_info_addr: u64,
    all_image_info_size: u64,
    all_image_info_format: i32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct DyldAllImageInfosHeader {
    version: u32,
    info_array_count: u32,
    info_array: u64,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct DyldImageInfo {
    image_load_address: u64,
    image_file_path: u64,
    image_file_mod_date: u64,
}

unsafe extern "C" {
    fn mach_task_self() -> MachPortT;
    fn task_for_pid(target_tport: MachPortT, pid: c_int, task: *mut MachPortT) -> KernReturnT;
    fn mach_port_deallocate(task: MachPortT, name: MachPortT) -> KernReturnT;
    fn mach_vm_read_overwrite(
        target_task: MachPortT,
        address: MachVmAddressT,
        size: MachVmSizeT,
        data: MachVmAddressT,
        outsize: *mut MachVmSizeT,
    ) -> KernReturnT;
    fn mach_vm_region(
        target_task: MachPortT,
        address: *mut MachVmAddressT,
        size: *mut MachVmSizeT,
        flavor: VmRegionFlavorT,
        info: *mut c_int,
        count: *mut MachMsgTypeNumberT,
        object_name: *mut MachPortT,
    ) -> KernReturnT;
    fn proc_listallpids(buffer: *mut c_void, buffersize: c_int) -> c_int;
    fn proc_name(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
    fn proc_pidpath(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
    fn task_info(
        target_task: MachPortT,
        flavor: TaskFlavorT,
        task_info_out: TaskInfoT,
        task_info_out_cnt: *mut MachMsgTypeNumberT,
    ) -> KernReturnT;
}

pub struct MacProc {
    pid: i32,
    task: MachPortT,
    pub base_addr: u64,
    offset: Option<WuwaOffset>,
}

impl MacProc {
    pub fn new(name: &str) -> Result<Self> {
        unsafe {
            let pid = Self::find_pid_by_name(name)
                .with_context(|| "게임이 실행 중이 아닙니다.".to_string())?;

            let mut task = 0u32;
            let kr = task_for_pid(mach_task_self(), pid, &mut task);
            if kr != KERN_SUCCESS || task == 0 {
                if kr == 5 {
                    bail!(
                        "게임 프로세스 권한 획득 실패(task_for_pid, kr=5). \
macOS 권한 거부 상태입니다.\n\
1) 앱을 종료하고, 터미널에서 sudo로 실행하세요.\n\
2) 시스템 설정 > 개인정보 보호 및 보안 > 개발자 도구에서 터미널(또는 iTerm)을 허용하세요.\n\
3) 터미널을 재시작한 뒤 다시 시도하세요.\n\
(pid={})",
                        pid
                    );
                }
                bail!("게임 프로세스 권한 획득 실패(task_for_pid). 관리자 권한으로 앱을 실행하세요. (pid={}, kr={})", pid, kr);
            }

            let base_addr = Self::find_main_image_base(task)?;
            log::info!(
                "Process '{}' connected! PID: {}, Base Address: {:X}",
                name,
                pid,
                base_addr
            );

            Ok(Self {
                pid,
                task,
                base_addr,
                offset: None,
            })
        }
    }

    pub fn is_alive(&self) -> bool {
        if self.pid <= 0 {
            return false;
        }

        let rc = unsafe { libc::kill(self.pid, 0) };
        if rc == 0 {
            return true;
        }
        matches!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EPERM)
        )
    }

    pub async fn get_location(
        &mut self,
        available_offsets: &Option<Vec<WuwaOffset>>,
    ) -> Result<PlayerInfo, NativeError> {
        if !self.is_alive() {
            return Err(NativeError::ProcessTerminated);
        }

        let Some(variants) = available_offsets else {
            return Err(PointerChainError {
                message: "오프셋 데이터를 불러오는 중입니다...".to_string(),
            });
        };

        if let Some(offset) = &self.offset {
            return self.get_location_with_offset(offset);
        }

        let mut failures = Vec::new();
        for (i, offset) in variants.iter().enumerate() {
            match self.get_location_with_offset(offset) {
                Ok(location) => {
                    log::info!(
                        "Offset variant #{} ({}) succeeded. Caching it.",
                        i + 1,
                        offset.name
                    );
                    self.offset = Some(offset.clone());
                    return Ok(location);
                }
                Err(e) => failures.push(format!("{}: {}", offset.name, e)),
            }
        }

        // 1차 시도가 모두 실패하면 global_gworld를 자동 보정 시도합니다.
        for offset in variants.iter() {
            if let Some(discovered) = self.discover_global_gworld(offset) {
                let mut adjusted = offset.clone();
                adjusted.global_gworld = discovered;
                if let Ok(location) = self.get_location_with_offset(&adjusted) {
                    log::info!(
                        "Offset '{}' recovered by gworld auto-discovery (0x{:X}).",
                        adjusted.name,
                        discovered
                    );
                    self.offset = Some(adjusted);
                    return Ok(location);
                }
            }
        }

        let summary = failures
            .iter()
            .take(2)
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ");
        Err(PointerChainError {
            message: format!(
                "사용 가능한 버전 값을 찾지 못했습니다. macOS 오프셋 불일치 가능성이 높습니다. 대표 실패: {}",
                summary
            ),
        })
    }

    pub fn get_active_offset_name(&self) -> Option<String> {
        self.offset.as_ref().map(|o| o.name.clone())
    }

    fn read_memory<T: Copy>(&self, address: u64) -> Option<T> {
        if address == 0 {
            return None;
        }

        unsafe {
            let mut output: T = mem::zeroed();
            let mut bytes_read: MachVmSizeT = 0;
            let kr = mach_vm_read_overwrite(
                self.task,
                address,
                mem::size_of::<T>() as MachVmSizeT,
                (&mut output as *mut T as *mut c_void) as MachVmAddressT,
                &mut bytes_read,
            );

            if kr == KERN_SUCCESS && bytes_read == mem::size_of::<T>() as MachVmSizeT {
                Some(output)
            } else {
                None
            }
        }
    }

    fn normalize_pointer(raw: u64) -> u64 {
        // arm64e 환경에서 상위 비트(PAC/TBI)가 섞여 들어오는 경우를 제거합니다.
        let masked = raw & 0x0000_FFFF_FFFF_FFFF;
        if masked < 0x1_0000 {
            return 0;
        }
        masked
    }

    fn read_pointer(&self, address: u64, label: &str) -> Result<u64, NativeError> {
        let raw = self.read_memory::<u64>(address).ok_or_else(|| PointerChainError {
            message: format!("'{}' 위치 ({:X})의 주소 값을 읽지 못했습니다.", label, address),
        })?;

        let normalized = Self::normalize_pointer(raw);
        if normalized == 0 {
            return Err(PointerChainError {
                message: format!(
                    "'{}' 포인터가 유효하지 않습니다. raw={:X}, normalized={:X}",
                    label, raw, normalized
                ),
            });
        }

        Ok(normalized)
    }

    fn read_pointer_quiet(&self, address: u64) -> Option<u64> {
        let raw = self.read_memory::<u64>(address)?;
        let normalized = Self::normalize_pointer(raw);
        if normalized == 0 {
            return None;
        }
        Some(normalized)
    }

    fn quick_chain_probe(&self, global_gworld: u64, offset: &WuwaOffset) -> bool {
        let gworld = self.read_pointer_quiet(self.base_addr + global_gworld);
        let Some(gworld) = gworld else { return false; };

        let game_instance = self.read_pointer_quiet(gworld + offset.uworld_owninggameinstance);
        let Some(game_instance) = game_instance else { return false; };

        let local_players = self.read_pointer_quiet(game_instance + offset.ugameinstance_localplayers);
        let Some(local_players) = local_players else { return false; };

        let local_player = self.read_pointer_quiet(local_players);
        let Some(local_player) = local_player else { return false; };

        let player_controller = self.read_pointer_quiet(local_player + offset.uplayer_playercontroller);
        let Some(player_controller) = player_controller else { return false; };

        let pawn = self.read_pointer_quiet(player_controller + offset.aplayercontroller_acknowlegedpawn);
        let Some(pawn) = pawn else { return false; };

        let root_component = self.read_pointer_quiet(pawn + offset.aactor_rootcomponent);
        let Some(root_component) = root_component else { return false; };

        let transform_addr = root_component + offset.uscenecomponent_componenttoworld;
        self.read_memory::<FTransformDouble>(transform_addr).is_some()
    }

    fn discover_global_gworld(&self, offset: &WuwaOffset) -> Option<u64> {
        const MAX_DELTA: i64 = 0x0060_0000; // +/- 6MB
        const STEP: i64 = 0x8;

        let base = offset.global_gworld as i64;
        let max_steps = MAX_DELTA / STEP;

        for step_idx in 0..=max_steps {
            let delta = step_idx * STEP;
            let candidates = if delta == 0 {
                vec![base]
            } else {
                vec![base + delta, base - delta]
            };

            for c in candidates {
                if c <= 0 {
                    continue;
                }
                let candidate = c as u64;
                if self.quick_chain_probe(candidate, offset) {
                    log::info!(
                        "Discovered macOS global_gworld candidate: 0x{:X} (base 0x{:X})",
                        candidate,
                        offset.global_gworld
                    );
                    return Some(candidate);
                }
            }
        }

        None
    }

    fn read_memory_from_task<T: Copy>(task: MachPortT, address: u64) -> Option<T> {
        if address == 0 {
            return None;
        }

        unsafe {
            let mut output: T = mem::zeroed();
            let mut bytes_read: MachVmSizeT = 0;
            let kr = mach_vm_read_overwrite(
                task,
                address,
                mem::size_of::<T>() as MachVmSizeT,
                (&mut output as *mut T as *mut c_void) as MachVmAddressT,
                &mut bytes_read,
            );

            if kr == KERN_SUCCESS && bytes_read == mem::size_of::<T>() as MachVmSizeT {
                Some(output)
            } else {
                None
            }
        }
    }

    fn normalize_name(value: &str) -> String {
        value
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect::<String>()
    }

    fn candidate_match(candidate: &str, proc_name: &str, proc_basename: Option<&str>) -> bool {
        let cand = Self::normalize_name(candidate);
        let name = Self::normalize_name(proc_name);

        if !cand.is_empty() && (name == cand || name.contains(&cand) || cand.contains(&name)) {
            return true;
        }

        if let Some(base) = proc_basename {
            let base_n = Self::normalize_name(base);
            if !base_n.is_empty() && (base_n == cand || base_n.contains(&cand) || cand.contains(&base_n)) {
                return true;
            }
        }

        false
    }

    unsafe fn get_process_basename(pid: i32) -> Option<String> {
        let mut path_buf = [0i8; 4096];
        let len = unsafe { proc_pidpath(pid, path_buf.as_mut_ptr() as *mut c_void, path_buf.len() as u32) };
        if len <= 0 {
            return None;
        }

        let full_path = unsafe { CStr::from_ptr(path_buf.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        std::path::Path::new(&full_path)
            .file_name()
            .map(|v| v.to_string_lossy().into_owned())
    }

    unsafe fn find_pid_by_name(name: &str) -> Option<i32> {
        let mut pids = vec![0i32; 4096];
        let listed = unsafe {
            proc_listallpids(
                pids.as_mut_ptr() as *mut c_void,
                (pids.len() * mem::size_of::<i32>()) as c_int,
            )
        };
        if listed <= 0 {
            return None;
        }

        for pid in pids.into_iter().take(listed as usize) {
            if pid <= 0 {
                continue;
            }

            let mut name_buf = [0i8; 1024];
            let len = unsafe {
                proc_name(
                    pid,
                    name_buf.as_mut_ptr() as *mut c_void,
                    name_buf.len() as u32,
                )
            };
            if len <= 0 {
                continue;
            }

            let proc_name = unsafe { CStr::from_ptr(name_buf.as_ptr()) }.to_string_lossy().into_owned();
            let proc_basename = unsafe { Self::get_process_basename(pid) };

            if Self::candidate_match(name, &proc_name, proc_basename.as_deref()) {
                return Some(pid);
            }
        }

        None
    }

    unsafe fn find_main_image_base(task: MachPortT) -> Result<u64> {
        let mut dyld_info = TaskDyldInfo {
            all_image_info_addr: 0,
            all_image_info_size: 0,
            all_image_info_format: 0,
        };
        let mut count = TASK_DYLD_INFO_COUNT;
        let kr = unsafe {
            task_info(
                task,
                TASK_DYLD_INFO,
                (&mut dyld_info as *mut TaskDyldInfo).cast::<c_int>(),
                &mut count,
            )
        };
        if kr != KERN_SUCCESS || dyld_info.all_image_info_addr == 0 {
            bail!("dyld 정보 조회 실패(task_info, kr={}).", kr);
        }

        let infos = Self::read_memory_from_task::<DyldAllImageInfosHeader>(
            task,
            dyld_info.all_image_info_addr,
        )
        .ok_or_else(|| anyhow::anyhow!("dyld_all_image_infos 헤더를 읽지 못했습니다."))?;

        if infos.info_array_count == 0 || infos.info_array == 0 {
            bail!("dyld 이미지 목록이 비어 있습니다.");
        }

        // 첫 번째 엔트리는 일반적으로 메인 실행 파일입니다.
        let first_image = Self::read_memory_from_task::<DyldImageInfo>(task, infos.info_array)
            .ok_or_else(|| anyhow::anyhow!("dyld 이미지 엔트리를 읽지 못했습니다."))?;

        if first_image.image_load_address != 0 {
            return Ok(first_image.image_load_address);
        }

        // 안전망: dyld 정보가 비정상적일 때는 기존 방식(첫 실행 메모리 영역)으로 폴백합니다.
        unsafe { Self::find_executable_base_fallback(task) }
    }

    unsafe fn find_executable_base_fallback(task: MachPortT) -> Result<u64> {
        let mut address: MachVmAddressT = 1;
        let mut region_size: MachVmSizeT = 0;

        loop {
            let mut info = VmRegionBasicInfoData64 {
                protection: 0,
                max_protection: 0,
                inheritance: 0,
                shared: 0,
                reserved: 0,
                offset: 0,
                behavior: 0,
                user_wired_count: 0,
            };
            let mut count = VM_REGION_BASIC_INFO_COUNT_64;
            let mut object_name = 0u32;

            let kr = unsafe {
                mach_vm_region(
                    task,
                    &mut address,
                    &mut region_size,
                    VM_REGION_BASIC_INFO_64,
                    &mut info as *mut VmRegionBasicInfoData64 as *mut c_int,
                    &mut count,
                    &mut object_name,
                )
            };

            if kr != KERN_SUCCESS {
                break;
            }

            if (info.protection & VM_PROT_EXECUTE) != 0 {
                return Ok(address);
            }

            let next = address.saturating_add(region_size.max(1));
            if next <= address {
                break;
            }
            address = next;
        }

        bail!("실행 가능한 메모리 영역을 찾지 못했습니다.");
    }

    fn quat_to_euler(x: f32, y: f32, z: f32, w: f32) -> (f32, f32, f32) {
        let sinr_cosp = 2.0 * (w * x + y * z);
        let cosr_cosp = 1.0 - 2.0 * (x * x + y * y);
        let roll = sinr_cosp.atan2(cosr_cosp);

        let sinp = 2.0 * (w * y - z * x);
        let pitch = if sinp.abs() >= 1.0 {
            (PI / 2.0).copysign(sinp)
        } else {
            sinp.asin()
        };

        let siny_cosp = 2.0 * (w * z + x * y);
        let cosy_cosp = 1.0 - 2.0 * (y * y + z * z);
        let yaw = siny_cosp.atan2(cosy_cosp);

        ((roll * 180.0 / PI), (pitch * 180.0 / PI), (yaw * 180.0 / PI))
    }

    fn get_location_with_offset(&self, offset: &WuwaOffset) -> Result<PlayerInfo, NativeError> {
        let targets = [
            ("GWorld", offset.global_gworld),
            ("OwningGameInstance", offset.uworld_owninggameinstance),
            ("TArray<*LocalPlayers>", offset.ugameinstance_localplayers),
            ("LocalPlayer", 0),
            ("PlayerController", offset.uplayer_playercontroller),
            ("APawn", offset.aplayercontroller_acknowlegedpawn),
            ("RootComponent", offset.aactor_rootcomponent),
        ];

        let mut last_addr = self.base_addr;
        for t in targets {
            let target = last_addr + t.1;
            last_addr = self.read_pointer(target, t.0)?;
        }

        let target = last_addr + offset.uscenecomponent_componenttoworld;
        let location =
            self.read_memory::<FTransformDouble>(target)
                .ok_or_else(|| ValueReadError {
                    message: format!("FTransform 위치 ({:X})의 값을 읽지 못했습니다.", target),
                })?;

        let (roll, pitch, yaw) =
            Self::quat_to_euler(location.rot_x, location.rot_y, location.rot_z, location.rot_w);

        let target_worldorigin = [
            ("GWorld", offset.global_gworld),
            ("PersistentLevel", offset.uworld_persistentlevel),
        ];

        last_addr = self.base_addr;
        for t in target_worldorigin {
            let target = last_addr + t.1;
            last_addr = self.read_pointer(target, t.0)?;
        }

        let target = last_addr + offset.ulevel_lastworldorigin;
        let root_location = self
            .read_memory::<FIntVector>(target)
            .ok_or_else(|| ValueReadError {
                message: format!("LastWorldOrigin 위치 ({:X})의 값을 읽지 못했습니다.", target),
            })?;

        Ok(PlayerInfo {
            x: location.loc_x + (root_location.x as f32),
            y: location.loc_y + (root_location.y as f32),
            z: location.loc_z + (root_location.z as f32),
            pitch,
            yaw,
            roll,
        })
    }
}

impl Drop for MacProc {
    fn drop(&mut self) {
        if self.task != 0 {
            log::info!("Closing task port for PID {}", self.pid);
            unsafe {
                let _ = mach_port_deallocate(mach_task_self(), self.task);
            }
        }
    }
}

unsafe impl Send for MacProc {}
