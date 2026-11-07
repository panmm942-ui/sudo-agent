//! CPU, memory and GPU load for the island (Windows performance counters; nothing leaves the PC).
use serde::Serialize;

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SystemStats {
    /// Percent 0..100 since the previous reading (None on the very first one).
    pub cpu: Option<f32>,
    pub memory: f32,
    pub memory_used_gb: f32,
    pub memory_total_gb: f32,
    /// Busiest GPU's 3D engine load, percent (None when Windows offers no GPU counters).
    pub gpu: Option<f32>,
}

#[cfg(windows)]
mod platform {
    use super::SystemStats;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use windows::core::w;
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE,
        PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
    };
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows::Win32::System::Threading::GetSystemTimes;

    struct Sampler { cpu: Option<(u64, u64)>, gpu: Option<(usize, usize)>, gpu_failed: bool }
    static SAMPLER: Mutex<Sampler> = Mutex::new(Sampler { cpu: None, gpu: None, gpu_failed: false });

    fn ticks(t: FILETIME) -> u64 { ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64 }

    fn cpu(previous: &mut Option<(u64, u64)>) -> Option<f32> {
        let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
        unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()? };
        // Kernel time includes idle time.
        let (busy_total, idle) = (ticks(kernel) + ticks(user), ticks(idle));
        let result = previous.and_then(|(total0, idle0)| {
            let total = busy_total.checked_sub(total0)?;
            let idle = idle.checked_sub(idle0)?;
            (total > 0).then(|| (100.0 * (total.saturating_sub(idle)) as f32 / total as f32).clamp(0.0, 100.0))
        });
        *previous = Some((busy_total, idle));
        result
    }

    fn gpu(sampler: &mut Sampler) -> Option<f32> {
        if sampler.gpu_failed { return None; }
        unsafe {
            if sampler.gpu.is_none() {
                let mut query = PDH_HQUERY::default();
                if PdhOpenQueryW(None, 0, &mut query) != 0 { sampler.gpu_failed = true; return None; }
                let mut counter = PDH_HCOUNTER::default();
                if PdhAddEnglishCounterW(query, w!("\\GPU Engine(*engtype_3D)\\Utilization Percentage"), 0, &mut counter) != 0 { sampler.gpu_failed = true; return None; }
                let _ = PdhCollectQueryData(query);
                sampler.gpu = Some((query.0 as usize, counter.0 as usize));
                return None;
            }
            let (query, counter) = sampler.gpu?;
            let (query, counter) = (PDH_HQUERY(query as *mut _), PDH_HCOUNTER(counter as *mut _));
            if PdhCollectQueryData(query) != 0 { return None; }
            let (mut bytes, mut count) = (0u32, 0u32);
            if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut bytes, &mut count, None) != PDH_MORE_DATA || bytes == 0 { return Some(0.0); }
            let mut buffer = vec![0u8; bytes as usize + 16];
            let items = buffer.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
            if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut bytes, &mut count, Some(items)) != 0 { return None; }
            // Sum the 3D engines per adapter ("…_luid_0x…_phys_0_eng_3_engtype_3D") and report the busiest.
            let mut adapters: HashMap<String, f64> = HashMap::new();
            for item in std::slice::from_raw_parts(items, count as usize) {
                if item.FmtValue.CStatus != 0 { continue; }
                let name = item.szName.to_string().unwrap_or_default();
                let luid = name.split("_luid_").nth(1).and_then(|rest| rest.split("_phys").next()).unwrap_or("").to_string();
                *adapters.entry(luid).or_default() += item.FmtValue.Anonymous.doubleValue;
            }
            Some(adapters.values().cloned().fold(0.0, f64::max).clamp(0.0, 100.0) as f32)
        }
    }

    pub fn stats() -> SystemStats {
        let mut sampler = SAMPLER.lock().unwrap_or_else(|p| p.into_inner());
        let cpu = cpu(&mut sampler.cpu);
        let gpu = gpu(&mut sampler);
        let mut memory = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        let _ = unsafe { GlobalMemoryStatusEx(&mut memory) };
        let gb = |bytes: u64| bytes as f32 / 1_073_741_824.0;
        SystemStats {
            cpu, gpu,
            memory: memory.dwMemoryLoad as f32,
            memory_total_gb: gb(memory.ullTotalPhys),
            memory_used_gb: gb(memory.ullTotalPhys.saturating_sub(memory.ullAvailPhys)),
        }
    }
}

#[tauri::command]
pub async fn system_stats() -> Result<SystemStats, String> {
    #[cfg(windows)] { tauri::async_runtime::spawn_blocking(platform::stats).await.map_err(|_| "System stats are unavailable.".to_string()) }
    #[cfg(not(windows))] { Err("System stats work on Windows.".into()) }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    #[ignore]
    fn reads_live_stats() {
        for _ in 0..3 { println!("{:?}", super::platform::stats()); std::thread::sleep(std::time::Duration::from_millis(700)); }
    }
}
