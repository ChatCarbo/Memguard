//! Couche Windows : tout le `unsafe` est confiné ici.
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, LUID};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::ProcessStatus::{
    EnumProcesses, GetProcessImageFileNameW, K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION, PROCESS_SET_QUOTA,
};
use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
};

unsafe extern "system" fn find_window_cb(hwnd: HWND, lparam: LPARAM) -> i32 {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == GetCurrentProcessId() {
            let mut buf = [0u16; 64];
            let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            if n > 0 && String::from_utf16_lossy(&buf[..n as usize]) == "MemGuard" {
                *(lparam as *mut HWND) = hwnd;
                return 0; // stop
            }
        }
        1
    }
}

/// Réaffiche la fenêtre principale directement via Win32, sans dépendre
/// d'une frame egui (une fenêtre cachée peut ne plus être redessinée).
pub fn show_main_window() -> bool {
    unsafe {
        let mut found: HWND = null_mut();
        EnumWindows(Some(find_window_cb), &mut found as *mut HWND as LPARAM);
        if found.is_null() {
            return false;
        }
        ShowWindow(found, SW_SHOW);
        ShowWindow(found, SW_RESTORE);
        SetForegroundWindow(found);
        true
    }
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn K32EmptyWorkingSet(process: HANDLE) -> i32;
    fn SetProcessInformation(
        process: HANDLE,
        class: i32,
        info: *const c_void,
        size: u32,
    ) -> i32;
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQuerySystemInformation(class: u32, info: *mut c_void, len: u32, ret: *mut u32) -> i32;
    fn NtSetSystemInformation(class: u32, info: *const c_void, len: u32) -> i32;
}

const SYSTEM_MEMORY_LIST_INFORMATION: u32 = 80;
const MEMORY_PURGE_LOW_PRIORITY_STANDBY_LIST: u32 = 5;
const MEMORY_PURGE_STANDBY_LIST: u32 = 4;
const PROCESS_MEMORY_PRIORITY: i32 = 0;
pub const MEMORY_PRIORITY_VERY_LOW: u32 = 1;
const PAGE_SIZE: f64 = 4096.0;
const MB: f64 = 1024.0 * 1024.0;

#[repr(C)]
struct MemoryListInfo {
    zero: usize,
    free: usize,
    modified: usize,
    modified_no_write: usize,
    bad: usize,
    standby_by_priority: [usize; 8],
    repurposed_by_priority: [usize; 8],
    modified_page_file: usize,
}

#[repr(C)]
struct MemoryPriorityInfo {
    memory_priority: u32,
}

#[derive(Debug, Clone, Default)]
pub struct MemStats {
    pub total_mb: f64,
    pub avail_mb: f64,
    pub load_pct: u32,
    /// Valeurs réelles lues via NtQuerySystemInformation (0 si indisponible)
    pub standby_mb: f64,
    pub modified_mb: f64,
    pub commit_used_mb: f64,
    pub commit_limit_mb: f64,
}

#[derive(Debug, Clone)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
    pub ws_mb: f32,
}

/// Active un privilège dans le jeton du processus. Vérifie réellement
/// `GetLastError` (AdjustTokenPrivileges renvoie TRUE même sans succès).
pub fn enable_privilege(name: &str) -> bool {
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        ) == 0
        {
            return false;
        }
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let mut luid: LUID = zeroed();
        let mut granted = false;
        if LookupPrivilegeValueW(null(), wide.as_ptr(), &mut luid) != 0 {
            let tp = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES {
                    Luid: luid,
                    Attributes: SE_PRIVILEGE_ENABLED,
                }],
            };
            let ok = AdjustTokenPrivileges(
                token,
                0,
                &tp,
                size_of::<TOKEN_PRIVILEGES>() as u32,
                null_mut(),
                null_mut(),
            );
            granted = ok != 0 && GetLastError() == 0;
        }
        CloseHandle(token);
        granted
    }
}

pub fn enable_memory_privilege() -> bool {
    enable_privilege("SeProfileSingleProcessPrivilege")
}

pub fn self_pid() -> u32 {
    unsafe { GetCurrentProcessId() }
}

pub fn foreground_pid() -> u32 {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0u32;
        if !hwnd.is_null() {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        pid
    }
}

/// Statistiques mémoire globales (valeurs réelles, aucune estimation).
pub fn memory_stats() -> MemStats {
    unsafe {
        let mut ms: MEMORYSTATUSEX = zeroed();
        ms.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut ms) == 0 {
            return MemStats::default();
        }
        let mut s = MemStats {
            total_mb: ms.ullTotalPhys as f64 / MB,
            avail_mb: ms.ullAvailPhys as f64 / MB,
            load_pct: ms.dwMemoryLoad,
            commit_limit_mb: ms.ullTotalPageFile as f64 / MB,
            commit_used_mb: (ms.ullTotalPageFile - ms.ullAvailPageFile) as f64 / MB,
            ..Default::default()
        };

        let mut ml: MemoryListInfo = zeroed();
        let mut ret = 0u32;
        let st = NtQuerySystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &mut ml as *mut _ as *mut c_void,
            size_of::<MemoryListInfo>() as u32,
            &mut ret,
        );
        if st == 0 {
            let standby: usize = ml.standby_by_priority.iter().sum();
            s.standby_mb = standby as f64 * PAGE_SIZE / MB;
            s.modified_mb = ml.modified as f64 * PAGE_SIZE / MB;
        }
        s
    }
}

/// Remplit `buf` (réutilisé, agrandi si besoin) et renvoie le nombre de PID.
fn enum_pids(buf: &mut Vec<u32>) -> usize {
    if buf.is_empty() {
        buf.resize(1024, 0);
    }
    loop {
        let bytes = (buf.len() * size_of::<u32>()) as u32;
        let mut needed = 0u32;
        if unsafe { EnumProcesses(buf.as_mut_ptr(), bytes, &mut needed) } == 0 {
            return 0;
        }
        if needed < bytes {
            return needed as usize / size_of::<u32>();
        }
        let n = buf.len() * 2;
        buf.resize(n, 0);
    }
}

fn process_name(h: HANDLE) -> Option<String> {
    let mut name = [0u16; 260];
    let len = unsafe { GetProcessImageFileNameW(h, name.as_mut_ptr(), name.len() as u32) } as usize;
    if len == 0 {
        return None;
    }
    let full = String::from_utf16_lossy(&name[..len]);
    Some(full.rsplit('\\').next().unwrap_or(&full).to_string())
}

/// Liste les processus accessibles, triés par working set décroissant.
pub fn list_processes(pid_buf: &mut Vec<u32>) -> Vec<ProcInfo> {
    let count = enum_pids(pid_buf);
    let mut out = Vec::with_capacity(count);
    for &pid in &pid_buf[..count] {
        if pid == 0 || pid == 4 {
            continue;
        }
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                continue;
            }
            if let Some(name) = process_name(h) {
                let mut pmc: PROCESS_MEMORY_COUNTERS = zeroed();
                pmc.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
                let ws = if K32GetProcessMemoryInfo(h, &mut pmc, pmc.cb) != 0 {
                    pmc.WorkingSetSize as f32 / MB as f32
                } else {
                    0.0
                };
                out.push(ProcInfo { pid, name, ws_mb: ws });
            }
            CloseHandle(h);
        }
    }
    out.sort_by(|a, b| b.ws_mb.total_cmp(&a.ws_mb));
    out
}

/// Allège un processus : priorité mémoire "très basse" (Windows le rogne en
/// premier sous pression, sans le pénaliser s'il est actif) puis, si demandé,
/// un trim immédiat de son working set.
pub fn lighten_process(pid: u32, trim: bool) -> bool {
    unsafe {
        let h = OpenProcess(
            PROCESS_SET_INFORMATION | PROCESS_SET_QUOTA | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        );
        if h.is_null() {
            return false;
        }
        let info = MemoryPriorityInfo {
            memory_priority: MEMORY_PRIORITY_VERY_LOW,
        };
        let a = SetProcessInformation(
            h,
            PROCESS_MEMORY_PRIORITY,
            &info as *const _ as *const c_void,
            size_of::<MemoryPriorityInfo>() as u32,
        ) != 0;
        let b = trim && K32EmptyWorkingSet(h) != 0;
        CloseHandle(h);
        a || b
    }
}

/// Réduit le working set de memguard lui-même.
pub fn trim_self() {
    unsafe {
        K32EmptyWorkingSet(GetCurrentProcess());
    }
}

/// Purge la Standby List. `low_priority_only = true` est la version douce.
/// Nécessite les droits administrateur + SeProfileSingleProcessPrivilege.
pub fn purge_standby(low_priority_only: bool) -> bool {
    let cmd: u32 = if low_priority_only {
        MEMORY_PURGE_LOW_PRIORITY_STANDBY_LIST
    } else {
        MEMORY_PURGE_STANDBY_LIST
    };
    unsafe {
        NtSetSystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &cmd as *const u32 as *const c_void,
            size_of::<u32>() as u32,
        ) == 0
    }
}
