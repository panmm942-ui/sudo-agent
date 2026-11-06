//! On-demand Spotify process loopback. PCM lives only in a fixed memory window;
//! only seven frequency-band amplitudes are sent to the UI. No files or network.
use std::sync::atomic::{AtomicU32, Ordering};
static GENERATION: AtomicU32 = AtomicU32::new(0);
// Rapid toggles must not leave overlapping audio capture workers running.
static CAPTURE_WORKER: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[path = "spectrum.rs"]
mod spectrum;

#[cfg(windows)]
pub(crate) mod platform {
    use super::spectrum::{Spectrum, SAMPLE_RATE};
    use std::{collections::HashMap, mem::ManuallyDrop, sync::mpsc, time::{Duration, Instant}};
    use windows::core::{implement, Interface, Ref, Result, HRESULT, Error};
    use windows::Win32::Foundation::{CloseHandle, HANDLE, E_FAIL};
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED, BLOB};
    use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0};
    use windows::Win32::System::Variant::VT_BLOB;
    use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};

    struct Com;
    impl Com {
        fn new() -> Result<Self> { unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok()?; Ok(Self) }
    }
    impl Drop for Com { fn drop(&mut self) { unsafe { CoUninitialize(); } } }
    struct Handle(HANDLE);
    impl Drop for Handle { fn drop(&mut self) { let _ = unsafe { CloseHandle(self.0) }; } }

    /// Spotify renders through child processes. Capture its outermost Spotify process tree.
    fn spotify_pid() -> Result<Option<u32>> {
        let snapshot = Handle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? });
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut processes = HashMap::new();
        let mut next = unsafe { Process32FirstW(snapshot.0, &mut entry) };
        while next.is_ok() {
            let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..end]).eq_ignore_ascii_case("Spotify.exe") {
                processes.insert(entry.th32ProcessID, entry.th32ParentProcessID);
            }
            next = unsafe { Process32NextW(snapshot.0, &mut entry) };
        }
        Ok(processes.iter().filter(|(_, parent)| !processes.contains_key(parent)).map(|(&pid, _)| pid).min())
    }

    // windows-rs' implement macro supplies IAgileObject/free-threaded marshaling.
    // The callback only signals completion; the IAudioClient remains on our MTA worker.
    #[implement(IActivateAudioInterfaceCompletionHandler)]
    struct Activation {
        completed: mpsc::SyncSender<()>,
        // Windows retains the callback until activation completes, even on our timeout.
        // Keep the blob payload alive for that entire asynchronous operation.
        _params: Box<AUDIOCLIENT_ACTIVATION_PARAMS>,
    }
    impl IActivateAudioInterfaceCompletionHandler_Impl for Activation_Impl {
        fn ActivateCompleted(&self, _operation: Ref<IActivateAudioInterfaceAsyncOperation>) -> Result<()> {
            let _ = self.completed.try_send(());
            Ok(())
        }
    }

    struct Capture {
        client: IAudioClient,
        capture: IAudioCaptureClient,
        event: Handle,
        analyzer: Box<Spectrum>,
        last_packet: Instant,
        frames: u64,
    }
    impl Drop for Capture { fn drop(&mut self) { let _ = unsafe { self.client.Stop() }; } }
    impl Capture {
        fn open(pid: u32) -> Result<Self> {
            let mut params = Box::new(AUDIOCLIENT_ACTIVATION_PARAMS {
                ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
                Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 { ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                    TargetProcessId: pid,
                    ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
                } },
            });
            // The VT_BLOB borrows callback-owned data; do not run PropVariantClear on it.
            let variant = ManuallyDrop::new(PROPVARIANT { Anonymous: PROPVARIANT_0 { Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_BLOB,
                Anonymous: PROPVARIANT_0_0_0 { blob: BLOB {
                    cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                    pBlobData: (&mut *params as *mut AUDIOCLIENT_ACTIVATION_PARAMS).cast(),
                } },
                ..Default::default()
            }) } });
            let (send, receive) = mpsc::sync_channel(1);
            let handler: IActivateAudioInterfaceCompletionHandler = Activation { completed: send, _params: params }.into();
            let operation = unsafe { ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&*variant), &handler)? };
            receive.recv_timeout(Duration::from_secs(3)).map_err(|_| Error::new(E_FAIL, "Spotify audio activation timed out"))?;
            let mut status = HRESULT(0);
            let mut activated = None;
            unsafe { operation.GetActivateResult(&mut status, &mut activated)?; }
            status.ok()?;
            let client: IAudioClient = activated.ok_or_else(|| Error::new(E_FAIL, "No process audio interface"))?.cast()?;
            // Explicit fixed PCM format, as in Microsoft's ApplicationLoopback sample.
            let format = WAVEFORMATEX { wFormatTag: 1, nChannels: 2, nSamplesPerSec: SAMPLE_RATE,
                nAvgBytesPerSec: SAMPLE_RATE * 4, nBlockAlign: 4, wBitsPerSample: 16, cbSize: 0 };
            let event = Handle(unsafe { CreateEventW(None, false, false, None)? });
            unsafe {
                client.Initialize(AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
                    0, 0, &format, None)?;
                let capture = client.GetService::<IAudioCaptureClient>()?;
                client.SetEventHandle(event.0)?;
                client.Start()?;
                Ok(Self { client, capture, event, analyzer: Box::new(Spectrum::new()), last_packet: Instant::now(), frames: 0 })
            }
        }
        fn drain(&mut self) -> Result<()> {
            // Ten milliseconds bounds stop latency; WASAPI wakes us as soon as samples arrive.
            unsafe { WaitForSingleObject(self.event.0, 10); }
            // Bound work even if Windows supplies a backlog after the computer resumes.
            for _ in 0..64 {
                if unsafe { self.capture.GetNextPacketSize()? } == 0 { break; }
                let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0, 0);
                unsafe { self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?; }
                if flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0 { self.analyzer.clear(); }
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                    for _ in 0..frames { self.analyzer.push(0., 0.); }
                } else {
                    let pcm = unsafe { std::slice::from_raw_parts(data.cast::<i16>(), frames as usize * 2) };
                    for pair in pcm.chunks_exact(2) { self.analyzer.push(pair[0] as f32 / 32768., pair[1] as f32 / 32768.); }
                }
                unsafe { self.capture.ReleaseBuffer(frames)?; }
                self.frames += frames as u64;
                self.last_packet = Instant::now();
            }
            Ok(())
        }
        fn bands(&mut self) -> [f32; 7] {
            if self.last_packet.elapsed() > Duration::from_millis(150) { self.analyzer.clear(); return [0.; 7]; }
            self.analyzer.bands()
        }
    }

    pub fn watch(id: u32, channel: tauri::ipc::Channel<Option<[f32; 7]>>) {
        let Ok(_worker) = super::CAPTURE_WORKER.lock() else { let _ = channel.send(None); return; };
        if super::GENERATION.load(super::Ordering::Relaxed) != id { return; }
        let Ok(_com) = Com::new() else { let _ = channel.send(None); return; };
        let mut active: Option<(u32, Capture)> = None;
        let mut refresh = Instant::now();
        let mut publish = Instant::now();
        while super::GENERATION.load(super::Ordering::Relaxed) == id {
            if Instant::now() >= refresh {
                let pid = spotify_pid().ok().flatten();
                if active.as_ref().map(|(pid, _)| *pid) != pid { active = None; }
                if active.is_none() {
                    active = pid.and_then(|pid| match Capture::open(pid) {
                        Ok(capture) => Some((pid, capture)),
                        Err(error) => { eprintln!("Spotify spectrum unavailable: {error}"); None }
                    });
                }
                refresh = Instant::now() + Duration::from_secs(2);
            }
            if let Some((_, capture)) = active.as_mut() {
                if let Err(error) = capture.drain() {
                    eprintln!("Spotify spectrum interrupted: {error}"); active = None;
                }
            } else { std::thread::sleep(Duration::from_millis(20)); }
            if Instant::now() >= publish {
                if super::GENERATION.load(super::Ordering::Relaxed) != id { break; }
                let bands = active.as_mut().map(|(_, capture)| capture.bands());
                if channel.send(bands).is_err() { break; }
                publish = Instant::now() + Duration::from_millis(33);
            }
        }
    }

    #[cfg(test)]
    pub fn smoke() -> Result<()> {
        let _com = Com::new()?;
        let pid = spotify_pid()?.ok_or_else(|| Error::new(E_FAIL, "Spotify is not running"))?;
        let mut capture = Capture::open(pid)?;
        let until = Instant::now() + Duration::from_secs(3);
        let mut maxima = [0.0_f32; 7];
        let mut previous = [0.0_f32; 7];
        let mut changes = 0;
        while Instant::now() < until {
            capture.drain()?;
            let bands = capture.bands();
            for i in 0..7 { maxima[i] = maxima[i].max(bands[i]); }
            if bands.iter().zip(previous).any(|(a,b)| (a-b).abs() > 0.0001) { changes += 1; }
            previous = bands;
        }
        println!("Spotify process-tree PID {pid}; PCM frames {}; maximum seven bands {maxima:?}; changing frames {changes}", capture.frames);
        assert!(capture.frames > 0, "No loopback PCM packets arrived");
        assert!(maxima.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
        assert!(maxima.iter().any(|v| *v > 0.0001), "Play local Spotify audio for this live test (Connect/casting may render elsewhere)");
        assert!(changes > 2, "Expected changing real PCM band amplitudes");
        Ok(())
    }
}

#[tauri::command]
pub fn start_spotify_meter(channel: tauri::ipc::Channel<Option<[f32; 7]>>) -> Result<u32, String> {
    let id = GENERATION.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    #[cfg(windows)]
    std::thread::Builder::new().name("spotify-spectrum".into()).spawn(move || platform::watch(id, channel)).map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    let _ = channel.send(None);
    Ok(id)
}

#[tauri::command]
pub fn stop_spotify_meter(id: u32) { let _ = GENERATION.compare_exchange(id, id.wrapping_add(1), Ordering::Relaxed, Ordering::Relaxed); }

#[cfg(all(test, windows))]
mod tests {
    #[test]
    #[ignore = "captures three seconds of local Spotify PCM in memory; play Spotify and run explicitly"]
    fn spotify_process_spectrum_smoke() { super::platform::smoke().expect("Windows process-loopback spectrum failed"); }
}
