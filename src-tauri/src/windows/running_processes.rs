use serde::Serialize;
use windows::Win32::{
    Foundation::CloseHandle,
    System::{
        ProcessStatus::EnumProcesses,
        Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ},
    },
};

#[derive(Debug, Clone, Serialize)]
pub struct RunningProcess {
    pub process_id: u32,
    pub process_name: String,
    pub process_path: Option<String>,
}

pub fn list_running_processes() -> Result<Vec<RunningProcess>, String> {
    let mut process_ids = vec![0u32; 2048];
    let mut bytes_needed = 0u32;

    unsafe {
        EnumProcesses(
            process_ids.as_mut_ptr(),
            (process_ids.len() * std::mem::size_of::<u32>()) as u32,
            &mut bytes_needed,
        )
    }
    .map_err(|error| error.to_string())?;

    let count = bytes_needed as usize / std::mem::size_of::<u32>();
    let mut processes = process_ids[..count]
        .iter()
        .copied()
        .filter(|process_id| *process_id != 0)
        .filter_map(read_running_process)
        .collect::<Vec<_>>();

    processes.sort_by(|left, right| left.process_name.cmp(&right.process_name).then(left.process_id.cmp(&right.process_id)));
    processes.dedup_by(|left, right| left.process_name.eq_ignore_ascii_case(&right.process_name));
    Ok(processes)
}

fn read_running_process(process_id: u32) -> Option<RunningProcess> {
    let process_path = read_process_path(process_id)?;
    let process_name = std::path::Path::new(&process_path)
        .file_name()
        .and_then(|name| name.to_str())?
        .to_string();

    Some(RunningProcess {
        process_id,
        process_name,
        process_path: Some(process_path),
    })
}

fn read_process_path(process_id: u32) -> Option<String> {
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
            false,
            process_id,
        )
    }
    .ok()?;

    let result = std::env::current_exe().ok();
    let mut buffer = vec![0u16; 32768];
    let copied = unsafe { windows::Win32::System::ProcessStatus::K32GetModuleFileNameExW(Some(handle), None, &mut buffer) };
    unsafe {
        let _ = CloseHandle(handle);
    }

    if copied == 0 {
        return None;
    }

    let process_path = String::from_utf16_lossy(&buffer[..copied as usize]);
    if let Some(current_exe) = result {
        if current_exe.to_string_lossy().eq_ignore_ascii_case(&process_path) {
            return None;
        }
    }

    Some(process_path)
}
