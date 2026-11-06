//! Hold-to-talk hotkey (Phase 1 Task 8): Right Ctrl by default, plus any configured triggers.
//!
//! A `WH_KEYBOARD_LL` + `WH_MOUSE_LL` hook thread feeds the pure [`HoldDetector`] and emits the
//! Tauri events `hotkey_down` / `hotkey_up` / `hotkey_cancel`.
//!
//! Triggers come from `hotkeys.json` in the app config dir (`%APPDATA%\dev.sudo.island\`), read
//! once at startup; a missing file means Right Ctrl only:
//!
//! ```json
//! { "hold": ["RControl", "F13", "XButton1", "MiddleButton", "vk:0x87"], "swallow": ["F13"] }
//! ```
//!
//! - `hold`: key names (see [`parse_trigger`]), `MiddleButton` / `XButton1` / `XButton2`, or a raw
//!   virtual-key code `vk:0x87` / `vk:135`. Unknown names are skipped with a warning.
//! - `swallow`: hold triggers whose down *and* up are eaten (the hook returns 1), so a mouse button
//!   mapped to F13 or an XButton does not also fire an app action. A swallowed trigger is gone for
//!   every other app, taps included, and the release watchdog cannot see its physical state.
//!   Right Ctrl is never swallowed: it must keep working as a modifier.
//!
//! Injected (synthetic) input is ignored, so remappers, macro tools and sudo's own future typing
//! neither open the island nor cancel a physical hold. The one exception: a configured trigger
//! other than Right Ctrl is accepted even when injected, because mouse vendor software usually
//! sends its remapped keys with `SendInput` (if it sends a tap on press rather than mirroring the
//! button, a hold cannot be detected at all).

/// `VK_RCONTROL`.
pub const VK_RCONTROL: u16 = 0xA3;
/// Pseudo-VK for momentary input that has no release (the mouse wheel): counts as "other input"
/// but is never tracked as held.
pub const VK_WHEEL: u16 = 0x00;
/// A clean hold this long opens the island.
pub const HOLD_MS: u64 = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldEvent {
    Down,
    Up,
    Cancel,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Idle,
    /// Trigger `vk` went down at this time and nothing else has been pressed since.
    Pending(u16, u64),
    /// `Down` was emitted; waiting for the release of trigger `vk`.
    Held(u16),
    /// Trigger `vk` is being used as a modifier (or was pressed during other input); inert until
    /// it is released.
    Aborted(u16),
}

/// A mouse button usable as a hold trigger. Left/right are excluded: they would break clicking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Middle,
    X1,
    X2,
}

/// A hold-to-talk trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// A keyboard virtual-key code.
    Key(u16),
    Mouse(MouseButton),
}

impl Trigger {
    /// The VK the hooks report for this trigger (`VK_MBUTTON`, `VK_XBUTTON1/2` for mouse buttons).
    pub const fn vk(self) -> u16 {
        match self {
            Trigger::Key(vk) => vk,
            Trigger::Mouse(MouseButton::Middle) => 0x04,
            Trigger::Mouse(MouseButton::X1) => 0x05,
            Trigger::Mouse(MouseButton::X2) => 0x06,
        }
    }

    fn from_vk(vk: u16) -> Option<Self> {
        match vk {
            0x04 => Some(Trigger::Mouse(MouseButton::Middle)),
            0x05 => Some(Trigger::Mouse(MouseButton::X1)),
            0x06 => Some(Trigger::Mouse(MouseButton::X2)),
            // Left/right buttons, VK_CANCEL, out of range.
            0x00..=0x03 | 0xFF.. => None,
            vk => Some(Trigger::Key(vk)),
        }
    }
}

/// Right Ctrl only.
pub const DEFAULT_TRIGGERS: [Trigger; 1] = [Trigger::Key(VK_RCONTROL)];

/// Named keys accepted in `hotkeys.json` (case-insensitive), besides `F1`..`F24` and `vk:`.
const KEY_NAMES: &[(&str, u16)] = &[
    ("RControl", VK_RCONTROL),
    ("RCtrl", VK_RCONTROL),
    ("LControl", 0xA2),
    ("LCtrl", 0xA2),
    ("RShift", 0xA1),
    ("LShift", 0xA0),
    ("RAlt", 0xA5),
    ("LAlt", 0xA4),
    ("RWin", 0x5C),
    ("LWin", 0x5B),
    ("Apps", 0x5D),
    ("CapsLock", 0x14),
    ("ScrollLock", 0x91),
    ("Pause", 0x13),
    ("Insert", 0x2D),
    ("BrowserBack", 0xA6),
    ("BrowserForward", 0xA7),
    ("BrowserHome", 0xAC),
    ("VolumeMute", 0xAD),
    ("VolumeDown", 0xAE),
    ("VolumeUp", 0xAF),
    ("MediaNext", 0xB0),
    ("MediaPrev", 0xB1),
    ("MediaStop", 0xB2),
    ("MediaPlayPause", 0xB3),
    ("LaunchMail", 0xB4),
    ("LaunchApp1", 0xB6),
    ("LaunchApp2", 0xB7),
    ("MiddleButton", 0x04),
    ("MButton", 0x04),
    ("XButton1", 0x05),
    ("XButton2", 0x06),
];

/// Parses one trigger name: an entry of [`KEY_NAMES`], `F1`..`F24`, or `vk:<0xhex | decimal>`.
pub fn parse_trigger(name: &str) -> Option<Trigger> {
    let name = name.trim();
    let lower = name.to_ascii_lowercase();
    if let Some(code) = lower.strip_prefix("vk:") {
        let code = code.trim();
        let vk = match code.strip_prefix("0x") {
            Some(hex) => u16::from_str_radix(hex, 16).ok()?,
            None => code.parse().ok()?,
        };
        return Trigger::from_vk(vk);
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u16>().ok())
        && (1..=24).contains(&n)
    {
        return Some(Trigger::Key(0x70 + n - 1));
    }
    KEY_NAMES.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).and_then(|&(_, vk)| Trigger::from_vk(vk))
}

/// Parsed `hotkeys.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyConfig {
    pub hold: Vec<Trigger>,
    /// Subset of `hold`, never Right Ctrl.
    pub swallow: Vec<Trigger>,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self { hold: DEFAULT_TRIGGERS.to_vec(), swallow: Vec::new() }
    }
}

/// Parses one `hotkeys.json` list; `None` when the key is absent or not an array.
fn parse_list(root: &serde_json::Value, key: &str, warnings: &mut Vec<String>) -> Option<Vec<Trigger>> {
    let Some(entries) = root.get(key)?.as_array() else {
        warnings.push(format!("`{key}` must be an array of names, ignored"));
        return None;
    };
    let mut out = Vec::new();
    for entry in entries {
        match entry.as_str().map(|name| (name, parse_trigger(name))) {
            Some((_, Some(t))) if !out.contains(&t) => out.push(t),
            Some((_, Some(_))) => {}
            Some((name, None)) => warnings.push(format!("unknown {key} trigger {name:?}, skipped")),
            None => warnings.push(format!("{key} entry {entry} is not a name, skipped")),
        }
    }
    Some(out)
}

/// Parses `hotkeys.json`. Never fails: bad entries are skipped and reported in the warnings, and
/// a missing, empty or fully invalid `hold` falls back to [`DEFAULT_TRIGGERS`].
pub fn parse_config(json: &str) -> (HotkeyConfig, Vec<String>) {
    let root: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(err) => return (HotkeyConfig::default(), vec![format!("invalid JSON, using defaults: {err}")]),
    };
    let mut warnings = Vec::new();
    let hold = match parse_list(&root, "hold", &mut warnings) {
        Some(hold) if hold.is_empty() => {
            warnings.push("no valid hold trigger, using Right Ctrl".into());
            DEFAULT_TRIGGERS.to_vec()
        }
        Some(hold) => hold,
        None => DEFAULT_TRIGGERS.to_vec(),
    };
    let mut swallow = Vec::new();
    for t in parse_list(&root, "swallow", &mut warnings).unwrap_or_default() {
        if t == Trigger::Key(VK_RCONTROL) {
            warnings.push("Right Ctrl is never swallowed, ignored".into());
        } else if !hold.contains(&t) {
            warnings.push(format!("swallow {t:?} is not a hold trigger, ignored"));
        } else {
            swallow.push(t);
        }
    }
    (HotkeyConfig { hold, swallow }, warnings)
}

/// The set of VKs (keys and mouse buttons) currently believed down.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct KeySet([u64; 4]);

impl KeySet {
    fn bit(vk: u16) -> (usize, u64) {
        let vk = vk & 0xFF;
        (usize::from(vk >> 6), 1 << (vk & 63))
    }

    fn set(&mut self, vk: u16, on: bool) {
        let (i, b) = Self::bit(vk);
        if on { self.0[i] |= b } else { self.0[i] &= !b }
    }

    fn contains(&self, vk: u16) -> bool {
        let (i, b) = Self::bit(vk);
        self.0[i] & b != 0
    }

    fn any_except(&self, vk: u16) -> bool {
        let mut rest = *self;
        rest.set(vk, false);
        rest.0 != [0; 4]
    }
}

/// Turns raw key events into hold events. Any one trigger held `HOLD_MS` with no other input opens
/// the island. Trigger auto-repeat is ignored; a trigger only arms when nothing else is down, and
/// any other key/button *down* while it is held aborts or cancels (mouse movement is never fed).
/// A release whose press was never seen is ignored. Times are ms on any monotonic clock.
#[derive(Debug)]
pub struct HoldDetector {
    triggers: Vec<u16>,
    phase: Phase,
    down: KeySet,
}

impl Default for HoldDetector {
    fn default() -> Self {
        Self::with_triggers(&DEFAULT_TRIGGERS)
    }
}

impl HoldDetector {
    /// A detector armed by any of `triggers`.
    pub fn with_triggers(triggers: &[Trigger]) -> Self {
        Self { triggers: triggers.iter().map(|t| t.vk()).collect(), phase: Phase::Idle, down: KeySet::default() }
    }

    /// Feeds one key (or mouse button) event. The caller must `poll(t_ms)` first so a hold that
    /// matured before this event emits `Down` before it.
    pub fn on_key(&mut self, vk: u16, down: bool, t_ms: u64) -> Option<HoldEvent> {
        let repeat = down && self.down.contains(vk);
        if vk != VK_WHEEL {
            self.down.set(vk, down);
        }
        let (next, event) = match self.phase {
            Phase::Idle if down && !repeat && self.triggers.contains(&vk) => {
                if self.down.any_except(vk) {
                    (Phase::Aborted(vk), None)
                } else {
                    (Phase::Pending(vk, t_ms), None)
                }
            }
            Phase::Pending(a, _) | Phase::Held(a) | Phase::Aborted(a) if vk == a => match (down, self.phase) {
                (true, phase) => (phase, None),
                (false, Phase::Held(_)) => (Phase::Idle, Some(HoldEvent::Up)),
                (false, _) => (Phase::Idle, None),
            },
            Phase::Pending(a, _) if down => (Phase::Aborted(a), None),
            Phase::Held(a) if down => (Phase::Aborted(a), Some(HoldEvent::Cancel)),
            phase => (phase, None),
        };
        self.phase = next;
        event
    }

    /// Emits `Down` once the pending hold has lasted `HOLD_MS`.
    pub fn poll(&mut self, t_ms: u64) -> Option<HoldEvent> {
        match self.phase {
            Phase::Pending(vk, since) if t_ms >= since + HOLD_MS => {
                self.phase = Phase::Held(vk);
                Some(HoldEvent::Down)
            }
            _ => None,
        }
    }

    /// When `poll` must next run, if a hold is pending.
    pub fn deadline(&self) -> Option<u64> {
        match self.phase {
            Phase::Pending(_, since) => Some(since + HOLD_MS),
            _ => None,
        }
    }

    /// The trigger believed down (pending, held or aborted), if any.
    pub fn active(&self) -> Option<u16> {
        match self.phase {
            Phase::Idle => None,
            Phase::Pending(vk, _) | Phase::Held(vk) | Phase::Aborted(vk) => Some(vk),
        }
    }

    /// Whether a trigger is believed to be down.
    pub fn is_pressed(&self) -> bool {
        self.active().is_some()
    }

    /// Forgets held keys for which `still_down` is false, so a release lost while another window
    /// (secure desktop, elevated app) had the input cannot block every future hold.
    pub fn retain_down(&mut self, mut still_down: impl FnMut(u16) -> bool) {
        for vk in 1..=0xFF {
            if self.down.contains(vk) && !still_down(vk) {
                self.down.set(vk, false);
            }
        }
    }
}

/// While a trigger is down, re-check its physical state this often so a lost release
/// (Win+L, UAC secure desktop, hook timeout) cannot leave the island stuck listening.
pub const WATCHDOG_MS: u64 = 100;
/// Physical state is not trusted this soon after the trigger's own event: the async key state is
/// only updated after the low-level hook returns.
pub const SETTLE_MS: u64 = 50;
/// Consecutive "up" reads needed before a release is assumed (`GetAsyncKeyState` also reads 0
/// while an elevated window has the focus).
const WATCHDOG_STRIKES: u8 = 2;

/// Schedules and debounces the physical-state check for the active trigger.
#[derive(Debug, Default)]
pub struct Watchdog {
    last_event: u64,
    next_check: Option<u64>,
    strikes: u8,
}

impl Watchdog {
    /// The active trigger produced an event (press or auto-repeat) at `t`: it is down for sure.
    pub fn saw_trigger(&mut self, t: u64) {
        self.last_event = t;
        self.strikes = 0;
    }

    /// Runs a due check. `armed` = a trigger is down and its physical state is observable.
    /// Returns true when the trigger should be treated as released.
    pub fn tick(&mut self, t: u64, armed: bool, physically_down: impl FnOnce() -> bool) -> bool {
        if !armed {
            self.next_check = None;
            self.strikes = 0;
            return false;
        }
        let due = *self.next_check.get_or_insert(t + WATCHDOG_MS);
        if t < due {
            return false;
        }
        self.next_check = Some(t + WATCHDOG_MS);
        if t < self.last_event + SETTLE_MS {
            return false;
        }
        if physically_down() {
            self.strikes = 0;
            return false;
        }
        self.strikes += 1;
        if self.strikes < WATCHDOG_STRIKES {
            return false;
        }
        self.strikes = 0;
        self.next_check = None;
        true
    }

    /// When `tick` must next run, if armed.
    pub fn next_wake(&self) -> Option<u64> {
        self.next_check
    }
}

/// Read `hotkeys.json` and install the low-level hook thread. Called once from `lib.rs` setup.
pub fn init(app: &tauri::AppHandle) {
    let config = load_config(app);
    #[cfg(windows)]
    hook::spawn(app.clone(), config);
    #[cfg(not(windows))]
    let _ = config;
}

fn load_config(app: &tauri::AppHandle) -> HotkeyConfig {
    use tauri::Manager;
    let path = match app.path().app_config_dir() {
        Ok(dir) => dir.join("hotkeys.json"),
        Err(err) => {
            eprintln!("hotkey: no app config dir, using Right Ctrl: {err}");
            return HotkeyConfig::default();
        }
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return HotkeyConfig::default(),
        Err(err) => {
            eprintln!("hotkey: cannot read {}, using Right Ctrl: {err}", path.display());
            return HotkeyConfig::default();
        }
    };
    let (config, warnings) = parse_config(&text);
    for w in warnings {
        eprintln!("hotkey: {}: {w}", path.display());
    }
    config
}

#[cfg(windows)]
mod hook {
    use super::{HoldDetector, HoldEvent, HotkeyConfig, KeySet, VK_RCONTROL, VK_WHEEL, Watchdog};
    use std::cell::RefCell;
    use std::sync::mpsc::{Sender, channel};
    use std::time::Instant;
    use tauri::{AppHandle, Emitter};
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::{
        GetCurrentThread, INFINITE, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED,
        MSG, MSLLHOOKSTRUCT, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, QS_ALLINPUT,
        SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HHOOK, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP,
        WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE,
        WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
        WM_XBUTTONDOWN, WM_XBUTTONUP,
    };

    struct Ctx {
        detector: HoldDetector,
        watchdog: Watchdog,
        /// Triggers whose events are eaten.
        swallow: KeySet,
        /// Triggers accepted even when injected (all configured ones but Right Ctrl).
        allow_injected: KeySet,
        start: Instant,
        tx: Sender<HoldEvent>,
    }

    impl Ctx {
        fn now(&self) -> u64 {
            self.start.elapsed().as_millis() as u64
        }
    }

    thread_local! {
        /// Hook callbacks run on the thread that installed them, so no locking is needed.
        static CTX: RefCell<Option<Ctx>> = const { RefCell::new(None) };
    }

    fn send(tx: &Sender<HoldEvent>, e: Option<HoldEvent>) {
        if let Some(e) = e {
            let _ = tx.send(e);
        }
    }

    pub fn spawn(app: AppHandle, config: HotkeyConfig) {
        let (tx, rx) = channel::<HoldEvent>();
        // Emitting happens off the hook thread so the callback stays well under the LL-hook timeout.
        let emitter = std::thread::Builder::new().name("hotkey-emit".into()).spawn(move || {
            for e in rx {
                let name = match e {
                    HoldEvent::Down => "hotkey_down",
                    HoldEvent::Up => "hotkey_up",
                    HoldEvent::Cancel => "hotkey_cancel",
                };
                if let Err(err) = app.emit(name, ()) {
                    eprintln!("hotkey: emit {name} failed: {err}");
                }
            }
        });
        let hook = std::thread::Builder::new().name("hotkey-hook".into()).spawn(move || run(tx, config));
        if let Err(err) = emitter.and(hook) {
            eprintln!("hotkey: cannot start thread: {err}");
        }
    }

    fn run(tx: Sender<HoldEvent>, config: HotkeyConfig) {
        let (mut swallow, mut allow_injected) = (KeySet::default(), KeySet::default());
        for t in &config.swallow {
            swallow.set(t.vk(), t.vk() != VK_RCONTROL);
        }
        for t in &config.hold {
            allow_injected.set(t.vk(), t.vk() != VK_RCONTROL);
        }
        let ctx = Ctx {
            detector: HoldDetector::with_triggers(&config.hold),
            watchdog: Watchdog::default(),
            swallow,
            allow_injected,
            start: Instant::now(),
            tx,
        };
        CTX.set(Some(ctx));
        // Every system-wide input event waits on this thread; keep it ahead of busy foreground work
        // so the LL-hook timeout (after which Windows silently drops the hook) is never hit.
        // SAFETY: plain Win32 calls; the procs below have the HOOKPROC signature.
        if let Err(err) = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) } {
            eprintln!("hotkey: cannot raise hook thread priority: {err}");
        }
        let hmod = unsafe { GetModuleHandleW(None) }.ok().map(Into::into);
        let keyboard = match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hmod, 0) } {
            Ok(hook) => hook,
            Err(err) => { eprintln!("hotkey: keyboard hook failed: {err}"); return; }
        };
        let mouse = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hmod, 0) }
            .inspect_err(|err| eprintln!("hotkey: mouse hook failed (clicks will not cancel, mouse triggers inert): {err}"))
            .ok();
        let mut hooks: (HHOOK, Option<HHOOK>) = (keyboard, mouse);
        // Windows silently removes a low-level hook whose callback once ran past the system
        // timeout (a busy moment is enough), and never says so: the hidden island then no longer
        // comes back on scroll and hold-to-talk stops. Re-install both hooks regularly, so any
        // such loss heals within seconds.
        const REINSTALL: std::time::Duration = std::time::Duration::from_secs(5);
        let mut reinstall_at = Instant::now() + REINSTALL;

        let mut msg = MSG::default();
        loop {
            if Instant::now() >= reinstall_at {
                reinstall_at = Instant::now() + REINSTALL;
                if let Ok(keyboard) = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hmod, 0) } {
                    let _ = unsafe { UnhookWindowsHookEx(hooks.0) };
                    hooks.0 = keyboard;
                }
                if let Ok(mouse) = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hmod, 0) } {
                    if let Some(old) = hooks.1 { let _ = unsafe { UnhookWindowsHookEx(old) }; }
                    hooks.1 = Some(mouse);
                }
            }
            let timeout = on_timer().min(REINSTALL.as_millis() as u32);
            // Hook callbacks are delivered while this thread waits for / peeks messages.
            unsafe { MsgWaitForMultipleObjects(None, false, timeout, QS_ALLINPUT) };
            while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                if msg.message == WM_QUIT {
                    return;
                }
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }

    fn physically_down(vk: u16) -> bool {
        // SAFETY: no preconditions.
        let state = unsafe { GetAsyncKeyState(i32::from(vk)) };
        state < 0
    }

    /// Feeds one event; returns whether the hook must swallow it.
    fn on_input(vk: u16, down: bool, injected: bool) -> bool {
        // Print Screen or Win+Shift+S: allow the real island in the user's capture.
        if down && (vk==0x2c || (vk==0x53 && (physically_down(0x5b)||physically_down(0x5c)) && (physically_down(0xa0)||physically_down(0xa1)))) {
            // Off the hook thread: the callback must return within the LL-hook timeout.
            std::thread::spawn(crate::capture::screenshot_shortcut);
        }
        CTX.with_borrow_mut(|ctx| {
            let Some(c) = ctx.as_mut() else { return false };
            if injected && !c.allow_injected.contains(vk) {
                return false;
            }
            let t = c.now();
            send(&c.tx, c.detector.poll(t));
            if down && !c.detector.is_pressed() {
                // Other keys' async state is settled (their events were processed earlier);
                // swallowed triggers never show up in it.
                let swallow = c.swallow;
                c.detector.retain_down(|k| k == vk || swallow.contains(k) || physically_down(k));
            }
            send(&c.tx, c.detector.on_key(vk, down, t));
            if down && c.detector.active() == Some(vk) {
                c.watchdog.saw_trigger(t);
            }
            vk != VK_WHEEL && c.swallow.contains(vk)
        })
    }

    /// Fires a due `Down`, runs a due release-watchdog check, and returns the next wait in ms.
    /// Scheduled by absolute time, so a stream of mouse moves cannot starve it.
    fn on_timer() -> u32 {
        CTX.with_borrow_mut(|ctx| {
            let Some(c) = ctx.as_mut() else { return INFINITE };
            let t = c.now();
            // A swallowed trigger never reaches the async key state, so it cannot be watched.
            let active = c.detector.active().filter(|&vk| !c.swallow.contains(vk));
            if c.watchdog.tick(t, active.is_some(), || active.is_some_and(physically_down))
                && let Some(vk) = active
            {
                send(&c.tx, c.detector.on_key(vk, false, t));
            }
            send(&c.tx, c.detector.poll(t));
            match [c.detector.deadline(), c.watchdog.next_wake()].into_iter().flatten().min() {
                Some(at) => at.saturating_sub(t) as u32,
                None => INFINITE,
            }
        })
    }

    unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            // SAFETY: for HC_ACTION, lparam points to a KBDLLHOOKSTRUCT.
            let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            let injected = (kb.flags & LLKHF_INJECTED).0 != 0;
            let down = match wparam.0 as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => Some(true),
                WM_KEYUP | WM_SYSKEYUP => Some(false),
                _ => None,
            };
            if let Some(down) = down
                && on_input(kb.vkCode as u16, down, injected)
            {
                return LRESULT(1);
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let msg = wparam.0 as u32;
        // Hot path: every pointer move system-wide lands here. Pass it straight on.
        if code != HC_ACTION as i32 || msg == WM_MOUSEMOVE {
            return unsafe { CallNextHookEx(None, code, wparam, lparam) };
        }
        // SAFETY: for HC_ACTION, lparam points to an MSLLHOOKSTRUCT.
        let ms = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        // Scrolling down (negative wheel delta) where the hidden island sits brings it back.
        if msg == WM_MOUSEWHEEL && ((ms.mouseData >> 16) as u16 as i16) < 0 && crate::hittest::wheel_down() {
            return LRESULT(1);
        }
        // VK_XBUTTON1 / VK_XBUTTON2 by the high word of mouseData (XBUTTON1 = 1, XBUTTON2 = 2).
        let xbutton = if ms.mouseData >> 16 == 2 { 0x06 } else { 0x05 };
        let input = match msg {
            WM_LBUTTONDOWN => Some((0x01, true)),
            WM_LBUTTONUP => Some((0x01, false)),
            WM_RBUTTONDOWN => Some((0x02, true)),
            WM_RBUTTONUP => Some((0x02, false)),
            WM_MBUTTONDOWN => Some((0x04, true)),
            WM_MBUTTONUP => Some((0x04, false)),
            WM_XBUTTONDOWN => Some((xbutton, true)),
            WM_XBUTTONUP => Some((xbutton, false)),
            // Ctrl+scroll (zoom) uses the modifier too.
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => Some((VK_WHEEL, true)),
            _ => None,
        };
        if let Some((vk, down)) = input
            && on_input(vk, down, ms.flags & LLMHF_INJECTED != 0)
        {
            return LRESULT(1);
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use HoldEvent::*;

    const VK_LCONTROL: u16 = 0xA2;
    const VK_LSHIFT: u16 = 0xA0;
    const VK_C: u16 = 0x43;
    const VK_A: u16 = 0x41;
    const VK_LBUTTON: u16 = 0x01;

    /// Feeds `keys` like the hook thread: before each key (and at `until`), the timer fires at any
    /// deadline already due. Returns `(t_ms, event)` pairs.
    fn drive(keys: &[(u64, u16, bool)], until: u64) -> Vec<(u64, HoldEvent)> {
        drive_with(HoldDetector::default(), keys, until)
    }

    fn drive_with(mut d: HoldDetector, keys: &[(u64, u16, bool)], until: u64) -> Vec<(u64, HoldEvent)> {
        let mut out = Vec::new();
        let fire_due = |d: &mut HoldDetector, t: u64, out: &mut Vec<(u64, HoldEvent)>| {
            while let Some(at) = d.deadline().filter(|&at| at <= t) {
                match d.poll(at) {
                    Some(e) => out.push((at, e)),
                    None => break,
                }
            }
        };
        for &(t, vk, down) in keys {
            fire_due(&mut d, t, &mut out);
            if let Some(e) = d.on_key(vk, down, t) {
                out.push((t, e));
            }
        }
        fire_due(&mut d, until, &mut out);
        out
    }

    fn events(v: Vec<(u64, HoldEvent)>) -> Vec<HoldEvent> {
        v.into_iter().map(|(_, e)| e).collect()
    }

    #[test]
    fn hold_200_then_release_is_down_up() {
        let out = drive(&[(0, VK_RCONTROL, true), (200, VK_RCONTROL, false)], 500);
        assert_eq!(out, vec![(HOLD_MS, Down), (200, Up)]);
    }

    #[test]
    fn exactly_threshold_counts() {
        let out = drive(&[(0, VK_RCONTROL, true), (HOLD_MS, VK_RCONTROL, false)], 500);
        assert_eq!(events(out), vec![Down, Up]);
    }

    #[test]
    fn hold_100_then_release_is_nothing() {
        let out = drive(&[(0, VK_RCONTROL, true), (100, VK_RCONTROL, false)], 500);
        assert_eq!(out, vec![]);
    }

    #[test]
    fn right_ctrl_c_never_opens() {
        let out = drive(&[(0, VK_RCONTROL, true), (50, VK_C, true), (80, VK_C, false)], 300);
        assert_eq!(out, vec![]);
        let out = drive(
            &[(0, VK_RCONTROL, true), (50, VK_C, true), (80, VK_C, false), (400, VK_RCONTROL, false)],
            600,
        );
        assert_eq!(out, vec![]);
    }

    #[test]
    fn other_key_after_down_cancels_once() {
        let out = drive(
            &[(0, VK_RCONTROL, true), (250, VK_C, true), (260, VK_C, false), (400, VK_RCONTROL, false)],
            600,
        );
        assert_eq!(out, vec![(HOLD_MS, Down), (250, Cancel)]);
    }

    #[test]
    fn left_ctrl_hold_is_nothing() {
        let out = drive(&[(0, VK_LCONTROL, true), (300, VK_LCONTROL, false)], 600);
        assert_eq!(out, vec![]);
    }

    #[test]
    fn auto_repeat_neither_resets_nor_cancels() {
        // Typematic: repeats before the threshold, then every ~33 ms after the ~250 ms delay.
        let mut keys = vec![(0, VK_RCONTROL, true), (100, VK_RCONTROL, true), (170, VK_RCONTROL, true)];
        keys.extend((0..10).map(|i| (250 + i * 33, VK_RCONTROL, true)));
        keys.push((700, VK_RCONTROL, false));
        let out = drive(&keys, 900);
        assert_eq!(out, vec![(HOLD_MS, Down), (700, Up)]);
    }

    #[test]
    fn release_without_press_is_nothing() {
        let out = drive(&[(0, VK_RCONTROL, false), (300, VK_RCONTROL, false)], 600);
        assert_eq!(out, vec![]);
    }

    #[test]
    fn unseen_key_release_does_not_abort() {
        // A key whose press predates the detector (or was lost) is released during the hold.
        let out = drive(&[(0, VK_RCONTROL, true), (40, VK_A, false), (300, VK_RCONTROL, false)], 600);
        assert_eq!(events(out), vec![Down, Up]);
    }

    #[test]
    fn trigger_pressed_while_other_key_down_aborts() {
        // Shift held, then Right Ctrl: a Ctrl+Shift chord, not a hold — even after Shift lets go.
        let out = drive(
            &[(0, VK_LSHIFT, true), (10, VK_RCONTROL, true), (100, VK_LSHIFT, false), (400, VK_RCONTROL, false)],
            600,
        );
        assert_eq!(out, vec![]);
        // Typing rollover: `A` still down when Right Ctrl goes down.
        let out = drive(&[(0, VK_A, true), (5, VK_RCONTROL, true), (40, VK_A, false), (300, VK_RCONTROL, false)], 600);
        assert_eq!(out, vec![]);
    }

    #[test]
    fn trigger_during_drag_aborts_and_rearms_after() {
        let out = drive(
            &[
                (0, VK_LBUTTON, true),
                (100, VK_RCONTROL, true),
                (500, VK_RCONTROL, false),
                (600, VK_LBUTTON, false),
                (1000, VK_RCONTROL, true),
                (1300, VK_RCONTROL, false),
            ],
            1500,
        );
        assert_eq!(out, vec![(1000 + HOLD_MS, Down), (1300, Up)]);
    }

    #[test]
    fn retain_down_forgets_lost_releases() {
        let mut d = HoldDetector::default();
        d.on_key(VK_LSHIFT, true, 0);
        // Shift's release went to the secure desktop; the hook thread prunes before the next press.
        d.retain_down(|_| false);
        let out = drive_with(d, &[(1000, VK_RCONTROL, true), (1300, VK_RCONTROL, false)], 1500);
        assert_eq!(events(out), vec![Down, Up]);
    }

    #[test]
    fn mouse_button_counts_as_other_input() {
        let out = drive(&[(0, VK_RCONTROL, true), (60, VK_LBUTTON, true), (300, VK_RCONTROL, false)], 600);
        assert_eq!(out, vec![]);
        let out = drive(&[(0, VK_RCONTROL, true), (220, VK_LBUTTON, true), (300, VK_RCONTROL, false)], 600);
        assert_eq!(events(out), vec![Down, Cancel]);
    }

    #[test]
    fn wheel_counts_as_other_input_but_is_never_held() {
        let out = drive(
            &[
                (0, VK_RCONTROL, true),
                (60, VK_WHEEL, true),
                (300, VK_RCONTROL, false),
                (1000, VK_RCONTROL, true),
                (1300, VK_RCONTROL, false),
            ],
            1500,
        );
        assert_eq!(out, vec![(1000 + HOLD_MS, Down), (1300, Up)]);
    }

    #[test]
    fn aborted_hold_rearms_on_next_press() {
        let out = drive(
            &[
                (0, VK_RCONTROL, true),
                (50, VK_C, true),
                (60, VK_C, false),
                (90, VK_RCONTROL, false),
                (1000, VK_RCONTROL, true),
                (1250, VK_RCONTROL, false),
            ],
            1500,
        );
        assert_eq!(out, vec![(1000 + HOLD_MS, Down), (1250, Up)]);
    }

    const VK_F13: u16 = 0x7C;
    const VK_XBUTTON1: u16 = 0x05;
    const VK_RBUTTON: u16 = 0x02;

    #[test]
    fn mouse_trigger_hold_is_down_up() {
        let d = HoldDetector::with_triggers(&[Trigger::Mouse(MouseButton::X1)]);
        let out = drive_with(d, &[(0, VK_XBUTTON1, true), (400, VK_XBUTTON1, false)], 600);
        assert_eq!(out, vec![(HOLD_MS, Down), (400, Up)]);
    }

    #[test]
    fn mouse_trigger_is_cancelled_by_keys_and_other_buttons() {
        let x1 = [Trigger::Mouse(MouseButton::X1)];
        let out = drive_with(
            HoldDetector::with_triggers(&x1),
            &[(0, VK_XBUTTON1, true), (250, VK_C, true), (260, VK_C, false), (400, VK_XBUTTON1, false)],
            600,
        );
        assert_eq!(out, vec![(HOLD_MS, Down), (250, Cancel)]);
        let out = drive_with(
            HoldDetector::with_triggers(&x1),
            &[(0, VK_XBUTTON1, true), (100, VK_RBUTTON, true), (110, VK_RBUTTON, false), (400, VK_XBUTTON1, false)],
            600,
        );
        assert_eq!(out, vec![]);
    }

    #[test]
    fn only_configured_triggers_arm() {
        let d = HoldDetector::with_triggers(&[Trigger::Mouse(MouseButton::Middle)]);
        let out = drive_with(d, &[(0, VK_RCONTROL, true), (400, VK_RCONTROL, false)], 600);
        assert_eq!(out, vec![]);
    }

    #[test]
    fn any_trigger_in_a_list_arms() {
        let both = [Trigger::Key(VK_RCONTROL), Trigger::Key(VK_F13)];
        let out = drive_with(
            HoldDetector::with_triggers(&both),
            &[(0, VK_F13, true), (300, VK_F13, false), (1000, VK_RCONTROL, true), (1300, VK_RCONTROL, false)],
            1500,
        );
        assert_eq!(out, vec![(HOLD_MS, Down), (300, Up), (1000 + HOLD_MS, Down), (1300, Up)]);
        // A second trigger pressed during a hold is just other input.
        let out = drive_with(
            HoldDetector::with_triggers(&both),
            &[(0, VK_F13, true), (250, VK_RCONTROL, true), (300, VK_RCONTROL, false), (400, VK_F13, false)],
            600,
        );
        assert_eq!(out, vec![(HOLD_MS, Down), (250, Cancel)]);
    }

    #[test]
    fn parse_full_example() {
        let (cfg, warnings) = parse_config(
            r#"{"hold": ["RControl", "F13", "XButton1", "MiddleButton", "vk:0x87"], "swallow": ["F13"]}"#,
        );
        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(
            cfg.hold,
            vec![
                Trigger::Key(VK_RCONTROL),
                Trigger::Key(VK_F13),
                Trigger::Mouse(MouseButton::X1),
                Trigger::Mouse(MouseButton::Middle),
                Trigger::Key(0x87),
            ]
        );
        assert_eq!(cfg.swallow, vec![Trigger::Key(VK_F13)]);
    }

    #[test]
    fn parse_trigger_names() {
        assert_eq!(parse_trigger("rctrl"), Some(Trigger::Key(VK_RCONTROL)));
        assert_eq!(parse_trigger(" F1 "), Some(Trigger::Key(0x70)));
        assert_eq!(parse_trigger("f24"), Some(Trigger::Key(0x87)));
        assert_eq!(parse_trigger("F25"), None);
        assert_eq!(parse_trigger("F0"), None);
        assert_eq!(parse_trigger("vk:135"), Some(Trigger::Key(0x87)));
        assert_eq!(parse_trigger("VK:0xB3"), Some(Trigger::Key(0xB3)));
        assert_eq!(parse_trigger("MediaPlayPause"), Some(Trigger::Key(0xB3)));
        assert_eq!(parse_trigger("xbutton2"), Some(Trigger::Mouse(MouseButton::X2)));
        assert_eq!(parse_trigger("vk:0x04"), Some(Trigger::Mouse(MouseButton::Middle)));
        // Left/right buttons would break clicking; junk and out-of-range codes are rejected.
        assert_eq!(parse_trigger("vk:0x01"), None);
        assert_eq!(parse_trigger("vk:2"), None);
        assert_eq!(parse_trigger("vk:0x100"), None);
        assert_eq!(parse_trigger("vk:zz"), None);
        assert_eq!(parse_trigger("LeftButton"), None);
    }

    #[test]
    fn parse_skips_unknown_names_with_warnings() {
        let (cfg, warnings) = parse_config(r#"{"hold": ["F13", "Banana", 7, "F13"]}"#);
        assert_eq!(cfg.hold, vec![Trigger::Key(VK_F13)]);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].contains("Banana"));
    }

    #[test]
    fn parse_falls_back_to_right_ctrl() {
        for json in ["{}", r#"{"swallow": []}"#, r#"{"hold": []}"#, r#"{"hold": ["Nope"]}"#, r#"{"hold": "F13"}"#, "not json"] {
            let (cfg, _) = parse_config(json);
            assert_eq!(cfg, HotkeyConfig::default(), "{json}");
        }
        let (_, warnings) = parse_config("not json");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn parse_never_swallows_right_ctrl_or_non_triggers() {
        let (cfg, warnings) =
            parse_config(r#"{"hold": ["RControl", "XButton2"], "swallow": ["RControl", "F13", "XButton2"]}"#);
        assert_eq!(cfg.swallow, vec![Trigger::Mouse(MouseButton::X2)]);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
    }

    #[test]
    fn poll_fires_once_at_the_deadline() {
        let mut d = HoldDetector::default();
        assert_eq!(d.deadline(), None);
        assert_eq!(d.on_key(VK_RCONTROL, true, 1000), None);
        assert_eq!(d.deadline(), Some(1000 + HOLD_MS));
        assert_eq!(d.poll(1000 + HOLD_MS - 1), None);
        assert_eq!(d.poll(1000 + HOLD_MS), Some(Down));
        assert_eq!(d.deadline(), None);
        assert_eq!(d.poll(5000), None);
        assert!(d.is_pressed());
        assert_eq!(d.active(), Some(VK_RCONTROL));
        assert_eq!(d.on_key(VK_RCONTROL, false, 5000), Some(Up));
        assert!(!d.is_pressed());
    }

    #[test]
    fn late_poll_still_fires_down() {
        let mut d = HoldDetector::default();
        d.on_key(VK_RCONTROL, true, 0);
        assert_eq!(d.poll(900), Some(Down));
    }

    /// Runs the watchdog like the hook thread: ticks at every wake-up until `until`, with `reads(t)`
    /// as the physical state. Returns the release time, if any.
    fn watch(events: &[u64], reads: impl Fn(u64) -> bool, until: u64) -> Option<u64> {
        let mut w = Watchdog::default();
        w.saw_trigger(0);
        assert!(!w.tick(0, true, || reads(0)));
        let mut t = 0;
        loop {
            let next_event = events.iter().copied().find(|&e| e > t);
            let wake = w.next_wake()?;
            match next_event {
                Some(e) if e < wake => {
                    t = e;
                    w.saw_trigger(e);
                }
                _ => {
                    t = wake;
                    if t > until {
                        return None;
                    }
                    if w.tick(t, true, || reads(t)) {
                        return Some(t);
                    }
                }
            }
        }
    }

    #[test]
    fn watchdog_ignores_stale_up_read_right_after_press() {
        // Async key state still reads "up" just after the down callback: no check runs then.
        let mut w = Watchdog::default();
        w.saw_trigger(0);
        assert!(!w.tick(0, true, || panic!("must not read physical state at the press")));
        assert!(!w.tick(1, true, || panic!("must not read physical state before the period")));
        assert_eq!(w.next_wake(), Some(WATCHDOG_MS));
    }

    #[test]
    fn watchdog_needs_two_consecutive_up_reads() {
        // One transient "up" (e.g. an elevated window briefly focused) does not release.
        assert_eq!(watch(&[], |t| t != WATCHDOG_MS, 2000), None);
        // A really lost release is recovered after two periods.
        assert_eq!(watch(&[], |_| false, 2000), Some(2 * WATCHDOG_MS));
        // Up, down, up: never two in a row.
        assert_eq!(watch(&[], |t| (t / WATCHDOG_MS).is_multiple_of(2), 2000), None);
    }

    #[test]
    fn watchdog_skips_checks_within_settle_of_an_auto_repeat() {
        // Auto-repeat lands 10 ms before each check: physical state is never consulted.
        let repeats: Vec<u64> = (1..20).map(|i| i * WATCHDOG_MS - 10).collect();
        assert_eq!(watch(&repeats, |_| false, 1900), None);
    }

    #[test]
    fn watchdog_disarms_when_idle() {
        let mut w = Watchdog::default();
        w.tick(0, true, || true);
        assert!(w.next_wake().is_some());
        assert!(!w.tick(10, false, || unreachable!()));
        assert_eq!(w.next_wake(), None);
    }
}
