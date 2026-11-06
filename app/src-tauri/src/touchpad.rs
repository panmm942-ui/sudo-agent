//! Precision touchpads: Windows turns a two-finger scroll straight into scroll messages for the
//! window under the cursor, so the low-level mouse hook (`hotkey.rs`) never sees it, and the
//! hidden island (click-through) never gets it. Sudo therefore reads the touchpad's own finger
//! reports through Raw Input (the documented way to receive them in the background) and treats a
//! two-finger vertical swipe like a scroll: where the hidden island sits, it brings it back.

/// Finger positions from one touchpad frame: (contact id, y), touching fingers only.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Default)]
pub struct Gesture {
    /// Fingers currently down (from the last report that said how many).
    fingers: u32,
    /// Latest y per contact id.
    ys: Vec<(u32, i64)>,
    /// Mean y when the two-finger touch began.
    start: Option<f64>,
    fired: bool,
}

#[cfg_attr(not(windows), allow(dead_code))]
impl Gesture {
    /// Feeds one report: `count` is the reported contact count (0 in the follow-up reports of
    /// touchpads that send one finger per report), `contacts` the touching fingers in it.
    /// `range` is the touchpad's y range. Returns true once per swipe, when two fingers have
    /// moved vertically by about a tenth of the pad.
    pub fn feed(&mut self, count: u32, contacts: &[(u32, i64, bool)], range: i64) -> bool {
        if count > 0 { self.fingers = count; }
        for &(id, y, touching) in contacts {
            self.ys.retain(|(known, _)| *known != id);
            if touching { self.ys.push((id, y)); }
        }
        if self.fingers < 2 || self.ys.len() < 2 {
            if self.fingers < 2 { self.start = None; self.fired = false; self.ys.clear(); }
            return false;
        }
        let mean = self.ys.iter().map(|(_, y)| *y as f64).sum::<f64>() / self.ys.len() as f64;
        let start = *self.start.get_or_insert(mean);
        if !self.fired && (mean - start).abs() >= range.max(1) as f64 * 0.1 {
            self.fired = true;
            return true;
        }
        false
    }
}

/// Starts the background reader (once). Without a precision touchpad it simply never fires.
pub fn init() {
    #[cfg(windows)]
    { let _ = std::thread::Builder::new().name("touchpad".into()).spawn(reader::run); }
}

#[cfg(windows)]
mod reader {
    use super::Gesture;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use windows::core::w;
    use windows::Win32::Devices::HumanInterfaceDevice::{
        HidP_GetCaps, HidP_GetUsageValue, HidP_GetUsages, HidP_GetValueCaps, HidP_Input, HIDP_CAPS, HIDP_STATUS_SUCCESS,
        HIDP_VALUE_CAPS, PHIDP_PREPARSED_DATA,
    };
    use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::{
        GetRawInputData, GetRawInputDeviceInfoW, RegisterRawInputDevices, HRAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER,
        RIDEV_INPUTSINK, RIDI_PREPARSEDDATA, RID_INPUT, RIM_TYPEHID,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW, HWND_MESSAGE, MSG, WINDOW_EX_STYLE,
        WINDOW_STYLE, WM_INPUT, WNDCLASSW,
    };

    const DIGITIZER: u16 = 0x0D;
    const TOUCH_PAD: u16 = 0x05;
    const TIP_SWITCH: u16 = 0x42;
    const CONTACT_ID: u16 = 0x51;
    const CONTACT_COUNT: u16 = 0x54;
    const GENERIC_DESKTOP: u16 = 0x01;
    const Y: u16 = 0x31;

    /// What a touchpad's reports look like, read once per device.
    struct Device { preparsed: Vec<u8>, fingers: Vec<(u16, i64)>, gesture: Gesture }

    thread_local! { static DEVICES: RefCell<HashMap<isize, Option<Device>>> = RefCell::new(HashMap::new()); }

    fn describe(handle: HANDLE) -> Option<Device> {
        let mut size = 0u32;
        // SAFETY: size query, then a buffer of exactly that size.
        unsafe { GetRawInputDeviceInfoW(Some(handle), RIDI_PREPARSEDDATA, None, &mut size) };
        if size == 0 || size > 1 << 20 { return None; }
        let mut preparsed = vec![0u8; size as usize];
        let got = unsafe { GetRawInputDeviceInfoW(Some(handle), RIDI_PREPARSEDDATA, Some(preparsed.as_mut_ptr().cast()), &mut size) };
        if got == u32::MAX || got == 0 { return None; }
        let data = PHIDP_PREPARSED_DATA(preparsed.as_ptr() as isize);
        let mut caps = HIDP_CAPS::default();
        if unsafe { HidP_GetCaps(data, &mut caps) } != HIDP_STATUS_SUCCESS || caps.UsagePage != DIGITIZER || caps.Usage != TOUCH_PAD { return None; }
        let mut count = caps.NumberInputValueCaps;
        let mut values = vec![HIDP_VALUE_CAPS::default(); count as usize];
        if unsafe { HidP_GetValueCaps(HidP_Input, values.as_mut_ptr(), &mut count, data) } != HIDP_STATUS_SUCCESS { return None; }
        // One Y value per finger slot, each in its own link collection.
        let fingers: Vec<(u16, i64)> = values.iter().take(count as usize)
            // SAFETY: NotRange is the active union member when IsRange is false.
            .filter(|v| v.UsagePage == GENERIC_DESKTOP && !v.IsRange && unsafe { v.Anonymous.NotRange.Usage } == Y)
            .map(|v| (v.LinkCollection, i64::from(v.LogicalMax) - i64::from(v.LogicalMin)))
            .collect();
        (!fingers.is_empty()).then_some(Device { preparsed, fingers, gesture: Gesture::default() })
    }

    fn report(device: &mut Device, report: &mut [u8]) -> bool {
        let data = PHIDP_PREPARSED_DATA(device.preparsed.as_ptr() as isize);
        let mut count = 0u32;
        // SAFETY: the report bytes and preparsed data belong to the same device.
        unsafe { let _ = HidP_GetUsageValue(HidP_Input, DIGITIZER, None, CONTACT_COUNT, &mut count, data, report); }
        let mut contacts = Vec::new();
        let range = device.fingers.first().map_or(1, |f| f.1);
        for &(link, _) in &device.fingers {
            let (mut y, mut id) = (0u32, 0u32);
            if unsafe { HidP_GetUsageValue(HidP_Input, GENERIC_DESKTOP, Some(link), Y, &mut y, data, report) } != HIDP_STATUS_SUCCESS { continue; }
            unsafe { let _ = HidP_GetUsageValue(HidP_Input, DIGITIZER, Some(link), CONTACT_ID, &mut id, data, report); }
            let mut usages = [0u16; 16];
            let mut length = usages.len() as u32;
            let touching = unsafe { HidP_GetUsages(HidP_Input, DIGITIZER, Some(link), usages.as_mut_ptr(), &mut length, data, report) } == HIDP_STATUS_SUCCESS
                && usages[..length as usize].contains(&TIP_SWITCH);
            contacts.push((id, i64::from(y), touching));
        }
        device.gesture.feed(count, &contacts, range)
    }

    unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if message == WM_INPUT {
            let handle = HRAWINPUT(lparam.0 as *mut core::ffi::c_void);
            let header = std::mem::size_of::<RAWINPUTHEADER>() as u32;
            let mut size = 0u32;
            unsafe { GetRawInputData(handle, RID_INPUT, None, &mut size, header) };
            if size > 0 && size < 1 << 16 {
                let mut buffer = vec![0u8; size as usize];
                if unsafe { GetRawInputData(handle, RID_INPUT, Some(buffer.as_mut_ptr().cast()), &mut size, header) } == size {
                    // SAFETY: the buffer starts with a RAWINPUTHEADER (read unaligned).
                    let head: RAWINPUTHEADER = unsafe { std::ptr::read_unaligned(buffer.as_ptr().cast()) };
                    let start = header as usize;
                    if head.dwType == RIM_TYPEHID.0 && buffer.len() >= start + 8 {
                        let each = u32::from_le_bytes(buffer[start..start + 4].try_into().unwrap_or_default()) as usize;
                        let reports = u32::from_le_bytes(buffer[start + 4..start + 8].try_into().unwrap_or_default()) as usize;
                        let key = head.hDevice.0 as isize;
                        let mut swiped = false;
                        DEVICES.with_borrow_mut(|devices| {
                            if let Some(device) = devices.entry(key).or_insert_with(|| describe(head.hDevice)).as_mut() {
                                for index in 0..reports {
                                    let from = start + 8 + index * each;
                                    let Some(bytes) = buffer.get_mut(from..from + each) else { break };
                                    swiped |= report(device, bytes);
                                }
                            }
                        });
                        // Same rule as a wheel scroll: only where the hidden island sits.
                        if swiped { crate::hittest::wheel_down(); }
                    }
                }
            }
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    pub fn run() {
        // SAFETY: plain Win32 calls on this thread's own message-only window.
        unsafe {
            let Ok(module) = GetModuleHandleW(None) else { return };
            let class = WNDCLASSW { lpfnWndProc: Some(window_proc), hInstance: module.into(), lpszClassName: w!("SudoTouchpadReader"), ..Default::default() };
            if RegisterClassW(&class) == 0 { return; }
            let Ok(window) = CreateWindowExW(WINDOW_EX_STYLE(0), w!("SudoTouchpadReader"), w!(""), WINDOW_STYLE(0), 0, 0, 0, 0,
                Some(HWND_MESSAGE), None, Some(module.into()), None) else { return };
            let device = RAWINPUTDEVICE { usUsagePage: DIGITIZER, usUsage: TOUCH_PAD, dwFlags: RIDEV_INPUTSINK, hwndTarget: window };
            if RegisterRawInputDevices(&[device], std::mem::size_of::<RAWINPUTDEVICE>() as u32).is_err() { return; }
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).as_bool() { DispatchMessageW(&message); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Gesture;
    #[test] fn a_two_finger_swipe_fires_once_either_direction() {
        let mut g = Gesture::default();
        assert!(!g.feed(2, &[(1, 500, true), (2, 520, true)], 1000));
        assert!(!g.feed(2, &[(1, 540, true), (2, 560, true)], 1000)); // 4%: not yet
        assert!(g.feed(2, &[(1, 620, true), (2, 640, true)], 1000));  // 12%: fires
        assert!(!g.feed(2, &[(1, 800, true), (2, 820, true)], 1000)); // only once per swipe
        assert!(!g.feed(0, &[(1, 800, false), (2, 820, false)], 1000));
        assert!(!g.feed(1, &[(1, 800, false)], 1000));
        let mut up = Gesture::default();
        up.feed(2, &[(1, 600, true), (2, 600, true)], 1000);
        assert!(up.feed(2, &[(1, 480, true), (2, 480, true)], 1000));
    }
    #[test] fn one_finger_moving_the_pointer_never_fires() {
        let mut g = Gesture::default();
        for y in (0..1000).step_by(50) { assert!(!g.feed(1, &[(1, y, true)], 1000)); }
    }
    #[test] fn one_finger_per_report_touchpads_still_work() {
        // Hybrid reporting: the first report says 2 fingers, later ones carry count 0.
        let mut g = Gesture::default();
        assert!(!g.feed(2, &[(1, 500, true)], 1000));
        assert!(!g.feed(0, &[(2, 500, true)], 1000));
        assert!(!g.feed(2, &[(1, 560, true)], 1000));
        assert!(g.feed(0, &[(2, 660, true)], 1000));
    }
}
