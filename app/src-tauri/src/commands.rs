//! Sudo AI fast commands: things a PC assistant should do instantly, without the language model.
//!
//! Level 0 is an exact phrase parser (Greek, English and Greeklish): "take a screenshot",
//! "άνοιξε το Spotify", "next song". Level 1 is the GLiNER router (`router.rs`), which turns
//! paraphrased requests into the same [`Action`]s. Anything else goes to Qwen.
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Screenshot,
    OpenApp(String),
    TypeText(String),
    PlayPause,
    NextTrack,
    PreviousTrack,
    VolumeUp,
    VolumeDown,
    Mute,
}

impl Action {
    /// Builds an action from a router intent and its extracted details (`None` when incomplete).
    pub fn from_intent(intent: &str, app: Option<&str>, text: Option<&str>) -> Option<Self> {
        let clean = |value: Option<&str>| value.map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
        Some(match intent {
            "screenshot" => Self::Screenshot,
            "open_app" => Self::OpenApp(clean(app)?),
            "type_text" => Self::TypeText(clean(text)?),
            "play_pause" => Self::PlayPause,
            "next_track" => Self::NextTrack,
            "previous_track" => Self::PreviousTrack,
            "volume_up" => Self::VolumeUp,
            "volume_down" => Self::VolumeDown,
            "mute" => Self::Mute,
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Normalisation

/// Lowercase, Greek accents removed (ά→α, ϊ→ι), final sigma as ς, punctuation dropped, single spaces.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'ά' => 'α', 'έ' => 'ε', 'ή' => 'η', 'ί' | 'ϊ' | 'ΐ' => 'ι', 'ό' => 'ο', 'ύ' | 'ϋ' | 'ΰ' => 'υ', 'ώ' => 'ω',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        out.push(c);
    }
    // "ΟΘΟΝΗΣ" lowercases to "οθονησ": a sigma that ends a word is written ς.
    let chars: Vec<char> = out.chars().collect();
    let out: String = chars.iter().enumerate()
        .map(|(i, c)| if (*c == 'σ' || *c == 'ς') && chars.get(i + 1).is_none_or(|n| !n.is_alphabetic()) && i > 0 && chars[i - 1].is_alphabetic() { 'ς' } else if *c == 'ς' { 'σ' } else { *c })
        .collect();
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_politeness(text: &str) -> &str {
    let mut t = text;
    for prefix in ["please ", "hey sudo ", "sudo ", "can you ", "could you ", "σε παρακαλω ", "μπορεις να ", "σουντο "] {
        t = t.strip_prefix(prefix).unwrap_or(t);
    }
    for suffix in [" please", " for me", " σε παρακαλω", " παρακαλω", " now", " τωρα"] {
        t = t.strip_suffix(suffix).unwrap_or(t);
    }
    t.trim()
}

fn one_of(text: &str, phrases: &[&str]) -> bool { phrases.contains(&text) }

/// Level 0: the exact, unambiguous phrasings. Anything looser is left to the router.
pub fn parse(original: &str) -> Option<Action> {
    let norm = normalize(original);
    let t = strip_politeness(&norm);
    if one_of(t, &[
        "take a screenshot", "take screenshot", "screenshot", "grab a screenshot", "capture the screen", "capture my screen",
        "take a screen shot", "παρε screenshot", "παρε ενα screenshot", "κανε screenshot", "βγαλε screenshot",
        "βγαλε στιγμιοτυπο", "βγαλε στιγμιοτυπο οθονης", "στιγμιοτυπο οθονης", "pare screenshot", "pare ena screenshot", "kane screenshot",
    ]) { return Some(Action::Screenshot); }
    if one_of(t, &["pause", "pause the music", "pause music", "stop the music", "play", "resume", "resume the music", "play music",
        "παυση", "σταματα τη μουσικη", "σταματα την μουσικη", "συνεχισε", "συνεχισε τη μουσικη", "βαλε μουσικη"]) { return Some(Action::PlayPause); }
    if one_of(t, &["next", "next song", "next track", "skip", "skip this song", "skip song", "επομενο", "επομενο τραγουδι",
        "αλλαξε τραγουδι", "epomeno", "epomeno tragoudi"]) { return Some(Action::NextTrack); }
    if one_of(t, &["previous", "previous song", "previous track", "last song", "go back a song", "προηγουμενο", "προηγουμενο τραγουδι",
        "proigoumeno", "proigoumeno tragoudi"]) { return Some(Action::PreviousTrack); }
    if one_of(t, &["volume up", "louder", "turn it up", "turn up the volume", "turn the volume up", "δυναμωσε", "δυναμωσε την ενταση",
        "δυναμωσε τον ηχο", "πιο δυνατα", "dynamose"]) { return Some(Action::VolumeUp); }
    if one_of(t, &["volume down", "quieter", "turn it down", "turn down the volume", "turn the volume down", "χαμηλωσε", "χαμηλωσε την ενταση",
        "χαμηλωσε τον ηχο", "πιο σιγα", "xamilose"]) { return Some(Action::VolumeDown); }
    if one_of(t, &["mute", "mute the sound", "unmute", "σιγαση", "κλεισε τον ηχο", "ανοιξε τον ηχο"]) { return Some(Action::Mute); }
    // "open X" / "άνοιξε το X": only when X really is an installed app (checked when it runs).
    for verb in ["open ", "launch ", "start ", "ανοιξε ", "ξεκινα ", "τρεξε ", "anoixe ", "anoikse "] {
        if let Some(rest) = t.strip_prefix(verb) {
            let rest = ["the ", "το ", "την ", "τον ", "τα ", "to "].iter().fold(rest, |r, a| r.strip_prefix(a).unwrap_or(r));
            let rest = rest.strip_suffix(" app").unwrap_or(rest).trim();
            if !rest.is_empty() && rest.split(' ').count() <= 4 && find_app(rest).is_some() { return Some(Action::OpenApp(rest.into())); }
        }
    }
    // "type X" keeps the user's own casing and punctuation (only "type"/"πληκτρολόγησε": "write" is often a request to compose).
    let trimmed = original.trim();
    let lower = trimmed.to_lowercase();
    for verb in ["type ", "πληκτρολόγησε ", "πληκτρολογησε "] {
        if lower.starts_with(verb) {
            let text: String = trimmed.chars().skip(verb.chars().count()).collect();
            let text = text.trim();
            if !text.is_empty() { return Some(Action::TypeText(text.to_string())); }
        }
    }
    None
}

/// Questions and requests to explain go to the language model, never to an action, even when
/// they mention one ("what is a screenshot?", "how do I open a stuck jar?").
pub fn looks_like_question(original: &str) -> bool {
    if original.trim_end().ends_with(['?', ';']) { return true; } // ';' is the Greek question mark
    let t = normalize(original);
    let t = strip_politeness(&t);
    const STARTS: &[&str] = &[
        "what", "whats", "how", "why", "when", "where", "who", "which", "is ", "are ", "do ", "does ", "did ", "explain", "tell me", "describe",
        "τι ", "πως", "γιατι", "ποτε", "που ", "ποιο", "ποια", "ποιος", "ποσο", "ειναι", "εξηγησε", "πες μου", "περιεγραψε",
    ];
    STARTS.iter().any(|s| t.starts_with(s))
}

/// Words that are part of asking, not of an app's name.
const FILLER: &[&str] = &[
    "open", "launch", "start", "run", "fire", "up", "the", "app", "application", "program", "please", "for", "me", "can", "you",
    "could", "would", "i", "want", "to", "like", "my", "a", "hey", "sudo", "now", "quickly",
    "ανοιξε", "ανοιξεις", "ανοιγεις", "ξεκινα", "ξεκινησε", "τρεξε", "το", "την", "τον", "τα", "να", "θελω", "μπορεις", "σε", "παρακαλω",
    "μου", "εφαρμογη", "εφαρμογης", "προγραμμα", "τωρα", "σουντο", "anoixe", "anoikse", "to",
];

/// The installed app named in a sentence ("θέλω να ανοίξεις το Steam" → Steam).
fn app_in_sentence(original: &str) -> Option<App> {
    let norm = normalize(original);
    let words: Vec<&str> = norm.split(' ').filter(|w| !FILLER.contains(w)).collect();
    for size in (1..=words.len().min(4)).rev() {
        for window in words.windows(size) {
            if let Some(app) = find_app(&window.join(" ")) { return Some(app); }
        }
    }
    None
}

/// What to type: everything after "type" / "γράψε" in the user's own words. "Write me a poem" is a
/// request to compose, not to type, so it is left to the language model.
fn text_to_type(original: &str) -> Option<String> {
    let chars: Vec<char> = original.trim().chars().collect();
    let lower: String = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    let lower_chars: Vec<char> = lower.chars().collect();
    for verb in ["type in ", "type out ", "type ", "write down ", "write ", "πληκτρολόγησε ", "πληκτρολογησε ", "γράψε ", "γραψε ", "grapse "] {
        let verb_chars: Vec<char> = verb.chars().collect();
        let Some(start) = (0..=lower_chars.len().saturating_sub(verb_chars.len())).find(|i| lower_chars[*i..].starts_with(&verb_chars)) else { continue };
        if start > 0 && !lower_chars[start - 1].is_whitespace() { continue; }
        let rest: String = chars[start + verb_chars.len()..].iter().collect();
        let rest = rest.trim().trim_matches(['"', '\'', '«', '»', '“', '”']).trim();
        let rest_norm = normalize(rest);
        let composing = ["me ", "a poem", "an email", "a story", "a letter", "an essay", "μου ", "ενα ποιημα", "ενα email", "μια ιστορια", "ενα γραμμα"];
        if rest.is_empty() || composing.iter().any(|c| rest_norm.starts_with(c)) { return None; }
        return Some(rest.to_string());
    }
    None
}

/// Level 1: turns the router's intent into a checked action, or None to ask the language model.
pub fn resolve(intent: &str, original: &str) -> Option<Action> {
    match intent {
        "open_app" => app_in_sentence(original).map(|app| Action::OpenApp(app.name)),
        "type_text" => text_to_type(original).map(Action::TypeText),
        other => Action::from_intent(other, None, None),
    }
}

// ---------------------------------------------------------------------------------------------
// Installed apps (the Start menu, as Windows lists it)

#[derive(Clone, Debug)]
struct App { name: String, key: String, id: String }

fn apps() -> &'static Mutex<(Vec<App>, Option<Instant>)> {
    static APPS: OnceLock<Mutex<(Vec<App>, Option<Instant>)>> = OnceLock::new();
    APPS.get_or_init(|| Mutex::new((Vec::new(), None)))
}
fn cache_path() -> PathBuf { crate::vocals::data_root().join("jarvis-apps.json") }

fn parse_apps(value: &Value) -> Vec<App> {
    let list = match value { Value::Array(items) => items.clone(), Value::Object(_) => vec![value.clone()], _ => Vec::new() };
    list.iter().filter_map(|item| {
        let name = item["Name"].as_str()?.trim().to_string();
        let id = item["AppID"].as_str()?.trim().to_string();
        (!name.is_empty() && !id.is_empty() && id.len() < 1000).then(|| App { key: normalize(&name), name, id })
    }).collect()
}

/// Loads the cached Start menu list at once, then refreshes it in the background (Get-StartApps
/// takes about a second, so it never runs while the user waits).
pub fn warm_app_index() {
    if let Ok(text) = std::fs::read_to_string(cache_path()) {
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            if let Ok(mut guard) = apps().lock() { if guard.0.is_empty() { guard.0 = parse_apps(&value); } }
        }
    }
    let stale = apps().lock().map(|g| g.1.is_none_or(|at| at.elapsed() > Duration::from_secs(600))).unwrap_or(false);
    if !stale { return; }
    if let Ok(mut guard) = apps().lock() { guard.1 = Some(Instant::now()); }
    std::thread::spawn(|| {
        let mut command = std::process::Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-StartApps | Select-Object Name,AppID | ConvertTo-Json -Compress"]);
        #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x0800_0000); }
        let Ok(output) = command.output() else { return };
        let Ok(value) = serde_json::from_slice::<Value>(&output.stdout) else { return };
        let list = parse_apps(&value);
        if list.is_empty() { return; }
        let _ = std::fs::write(cache_path(), value.to_string());
        if let Ok(mut guard) = apps().lock() { guard.0 = list; }
    });
}

/// Greek (and common) names for Windows' own apps, which Windows lists in English here.
const ALIASES: &[(&str, &str)] = &[
    ("υπολογιστη τσεπης", "calculator"), ("υπολογιστης τσεπης", "calculator"), ("αριθμομηχανη", "calculator"), ("calc", "calculator"),
    ("σημειωματαριο", "notepad"), ("ζωγραφικη", "paint"), ("ρυθμισεις", "settings"), ("εξερευνηση αρχειων", "file explorer"),
    ("αρχεια", "file explorer"), ("explorer", "file explorer"), ("καμερα", "camera"), ("ρολοι", "clock"), ("φωτογραφιες", "photos"),
    ("τερματικο", "terminal"), ("vs code", "visual studio code"), ("vscode", "visual studio code"), ("code", "visual studio code"),
    ("chrome", "google chrome"), ("σποτιφαι", "spotify"), ("ντισκορντ", "discord"), ("στιμ", "steam"), ("word", "word"),
];

fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut previous = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let current = row[j];
            row[j] = (row[j] + 1).min(row[j - 1] + 1).min(previous + usize::from(a[i - 1] != b[j - 1]));
            previous = current;
        }
    }
    row[b.len()]
}

/// The installed app the user means, or None: exact name, then a whole-word prefix, then a
/// close spelling. Uninstallers and help links never match.
fn find_app(spoken: &str) -> Option<App> {
    let mut key = normalize(spoken);
    if let Some((_, alias)) = ALIASES.iter().find(|(from, _)| *from == key) { key = (*alias).to_string(); }
    if key.is_empty() { return None; }
    let guard = apps().lock().ok()?;
    let usable = |app: &&App| !["uninstall", "help", "readme", "documentation", "release notes", "website", "on the web"].iter().any(|w| app.key.contains(w));
    let list: Vec<&App> = guard.0.iter().filter(usable).collect();
    if let Some(app) = list.iter().find(|a| a.key == key) { return Some((*app).clone()); }
    if let Some(app) = list.iter().filter(|a| a.key.starts_with(&format!("{key} ")) || a.key.ends_with(&format!(" {key}"))).min_by_key(|a| a.key.len()) {
        return Some((*app).clone());
    }
    let best = list.iter().map(|a| (edit_distance(&a.key, &key), a)).min_by_key(|(d, a)| (*d, a.key.len()))?;
    // A typo or a misheard name: about one wrong letter in three, for names of five letters or more.
    (key.chars().count() >= 5 && best.0 <= key.chars().count() / 3).then(|| (*best.1).clone())
}

// ---------------------------------------------------------------------------------------------
// Execution

/// What Sudo says after doing it, in the user's language.
fn said(lang: &str, en: &str, el: &str) -> String { if lang == "el" { el.into() } else { en.into() } }

/// Runs the action. Ok(reply) to speak, or Err(reason) to hand the request to the model instead
/// (for example an app that isn't installed).
pub fn execute(action: &Action, lang: &str) -> Result<String, String> {
    match action {
        Action::Screenshot => {
            // Win+Print Screen: Windows saves the full screen to Pictures\Screenshots and flashes it.
            keys::chord(&[keys::LWIN, keys::SNAPSHOT]);
            Ok(said(lang, "Screenshot saved to Pictures, Screenshots.", "Το στιγμιότυπο αποθηκεύτηκε στις Εικόνες."))
        }
        Action::OpenApp(name) => {
            let app = find_app(name).ok_or_else(|| format!("No installed app called {name}."))?;
            let mut command = std::process::Command::new("explorer.exe");
            command.arg(format!("shell:AppsFolder\\{}", app.id));
            command.spawn().map_err(|e| e.to_string())?;
            Ok(said(lang, &format!("Opening {}.", app.name), &format!("Ανοίγω το {}.", app.name)))
        }
        Action::TypeText(text) => {
            keys::focus_user_window();
            std::thread::sleep(Duration::from_millis(120));
            keys::type_text(text);
            Ok(said(lang, "Typed it.", "Το έγραψα."))
        }
        Action::PlayPause => { keys::tap(keys::MEDIA_PLAY_PAUSE); Ok(said(lang, "Done.", "Έγινε.")) }
        Action::NextTrack => { keys::tap(keys::MEDIA_NEXT); Ok(said(lang, "Next song.", "Επόμενο τραγούδι.")) }
        Action::PreviousTrack => { keys::tap(keys::MEDIA_PREV); Ok(said(lang, "Previous song.", "Προηγούμενο τραγούδι.")) }
        // Each volume key is one Windows step (2%): five steps is a noticeable change.
        Action::VolumeUp => { for _ in 0..5 { keys::tap(keys::VOLUME_UP); } Ok(said(lang, "Louder.", "Δυνάμωσα την ένταση.")) }
        Action::VolumeDown => { for _ in 0..5 { keys::tap(keys::VOLUME_DOWN); } Ok(said(lang, "Quieter.", "Χαμήλωσα την ένταση.")) }
        Action::Mute => { keys::tap(keys::VOLUME_MUTE); Ok(said(lang, "Sound toggled.", "Ο ήχος άλλαξε.")) }
    }
}

/// A short record for metrics and debugging.
pub fn describe(action: &Action) -> Value {
    match action {
        Action::OpenApp(name) => json!({"action":"open_app","app":name}),
        Action::TypeText(text) => json!({"action":"type_text","chars":text.chars().count()}),
        other => json!({"action":format!("{other:?}")}),
    }
}

#[cfg(windows)]
mod keys {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow};
    use std::sync::atomic::{AtomicIsize, Ordering};

    pub const LWIN: u16 = 0x5B;
    pub const SNAPSHOT: u16 = 0x2C;
    pub const MEDIA_NEXT: u16 = 0xB0;
    pub const MEDIA_PREV: u16 = 0xB1;
    pub const MEDIA_PLAY_PAUSE: u16 = 0xB3;
    pub const VOLUME_MUTE: u16 = 0xAD;
    pub const VOLUME_DOWN: u16 = 0xAE;
    pub const VOLUME_UP: u16 = 0xAF;

    fn key(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
    }
    fn send(inputs: &[INPUT]) {
        // SAFETY: a plain array of fully initialised INPUT structs.
        unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    }
    pub fn tap(vk: u16) { send(&[key(vk, 0, KEYBD_EVENT_FLAGS(0)), key(vk, 0, KEYEVENTF_KEYUP)]); }
    pub fn chord(vks: &[u16]) {
        let mut inputs: Vec<INPUT> = vks.iter().map(|vk| key(*vk, 0, KEYBD_EVENT_FLAGS(0))).collect();
        inputs.extend(vks.iter().rev().map(|vk| key(*vk, 0, KEYEVENTF_KEYUP)));
        send(&inputs);
    }
    /// Types any text (Greek too) as Unicode key events into the focused window.
    pub fn type_text(text: &str) {
        for unit in text.encode_utf16().collect::<Vec<_>>().chunks(32) {
            let inputs: Vec<INPUT> = unit.iter().flat_map(|u| [key(0, *u, KEYEVENTF_UNICODE), key(0, *u, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)]).collect();
            send(&inputs);
        }
    }

    /// The last foreground window that isn't Sudo's (updated by the hit-test poll).
    static LAST_USER_WINDOW: AtomicIsize = AtomicIsize::new(0);
    fn ours(hwnd: HWND) -> bool {
        let mut pid = 0u32;
        // SAFETY: read-only query into a local.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        pid == std::process::id()
    }
    pub fn note_foreground() {
        // SAFETY: no preconditions.
        let hwnd = unsafe { GetForegroundWindow() };
        if !hwnd.is_invalid() && !ours(hwnd) { LAST_USER_WINDOW.store(hwnd.0 as isize, Ordering::Relaxed); }
    }
    /// Typing from the Sudo tab: give the keyboard back to the window the user was in.
    pub fn focus_user_window() {
        // SAFETY: no preconditions; a stale handle just fails.
        let current = unsafe { GetForegroundWindow() };
        if !current.is_invalid() && !ours(current) { return; }
        let last = LAST_USER_WINDOW.load(Ordering::Relaxed);
        if last != 0 { unsafe { let _ = SetForegroundWindow(HWND(last as *mut core::ffi::c_void)); } }
    }
}

#[cfg(not(windows))]
mod keys {
    pub const LWIN: u16 = 0; pub const SNAPSHOT: u16 = 0; pub const MEDIA_NEXT: u16 = 0; pub const MEDIA_PREV: u16 = 0;
    pub const MEDIA_PLAY_PAUSE: u16 = 0; pub const VOLUME_MUTE: u16 = 0; pub const VOLUME_DOWN: u16 = 0; pub const VOLUME_UP: u16 = 0;
    pub fn tap(_: u16) {} pub fn chord(_: &[u16]) {} pub fn type_text(_: &str) {} pub fn note_foreground() {} pub fn focus_user_window() {}
}
pub use keys::note_foreground;

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn greek_accents_and_punctuation_normalise() {
        assert_eq!(normalize("Άνοιξε το Spotify, σε παρακαλώ!"), "ανοιξε το spotify σε παρακαλω");
        assert_eq!(normalize("Πάρε ένα screenshot."), "παρε ενα screenshot");
    }
    #[test] fn exact_phrases_in_both_languages() {
        for text in ["Take a screenshot", "πάρε ένα screenshot", "Βγάλε στιγμιότυπο οθόνης", "please take a screenshot", "pare ena screenshot"] {
            assert_eq!(parse(text), Some(Action::Screenshot), "{text}");
        }
        assert_eq!(parse("Next song"), Some(Action::NextTrack));
        assert_eq!(parse("Επόμενο τραγούδι"), Some(Action::NextTrack));
        assert_eq!(parse("δυνάμωσε την ένταση"), Some(Action::VolumeUp));
        assert_eq!(parse("σταμάτα τη μουσική"), Some(Action::PlayPause));
    }
    #[test] fn questions_and_loose_phrasing_are_not_commands() {
        for text in ["can you explain what a screenshot is", "write me a short poem", "how do I open a jar", "τι είναι το screenshot", "tell me a joke"] {
            assert_eq!(parse(text), None, "{text}");
        }
    }
    #[test] fn type_keeps_the_users_text() {
        assert_eq!(parse("Type Hello, World!"), Some(Action::TypeText("Hello, World!".into())));
        assert_eq!(parse("πληκτρολόγησε Καλημέρα σε όλους"), Some(Action::TypeText("Καλημέρα σε όλους".into())));
    }
    /// Both app tests share this list (tests run in parallel against one global index).
    fn test_apps() {
        if let Ok(mut guard) = apps().lock() {
            guard.0 = parse_apps(&json!([
                {"Name":"Spotify","AppID":"Spotify.exe"},{"Name":"Calculator","AppID":"Microsoft.WindowsCalculator!App"},
                {"Name":"Visual Studio Code","AppID":"code"},{"Name":"Uninstall Spotify","AppID":"u"},{"Name":"Discord","AppID":"d"},
                {"Name":"Steam","AppID":"s"}]));
            guard.1 = Some(Instant::now());
        }
    }
    #[test] fn apps_resolve_by_name_alias_and_spelling() {
        test_apps();
        assert_eq!(find_app("spotify").map(|a| a.id), Some("Spotify.exe".into()));
        assert_eq!(find_app("υπολογιστή τσέπης").map(|a| a.name), Some("Calculator".into()));
        assert_eq!(find_app("vs code").map(|a| a.name), Some("Visual Studio Code".into()));
        assert_eq!(find_app("discrod").map(|a| a.name), Some("Discord".into()));
        assert!(find_app("photoshop").is_none());
        assert_eq!(parse("Άνοιξε το Spotify"), Some(Action::OpenApp("spotify".into())));
        assert_eq!(parse("open photoshop"), None);
    }
    #[test] fn questions_are_never_commands() {
        for text in ["can you explain what a screenshot is", "how do I open a jar that's stuck", "What's the capital of Australia?",
                     "τι είναι το screenshot;", "πώς ανοίγω ένα αρχείο", "Is Spotify free?"] {
            assert!(looks_like_question(text), "{text}");
        }
        for text in ["take a screenshot", "open spotify", "γράψε καλημέρα", "next song"] { assert!(!looks_like_question(text), "{text}"); }
    }
    #[test] fn router_details_come_from_the_sentence() {
        test_apps();
        assert_eq!(resolve("open_app", "θέλω να ανοίξεις το Steam"), Some(Action::OpenApp("Steam".into())));
        assert_eq!(resolve("open_app", "could you start visual studio code for me"), Some(Action::OpenApp("Visual Studio Code".into())));
        assert_eq!(resolve("open_app", "can you open the calculator"), Some(Action::OpenApp("Calculator".into())));
        assert_eq!(resolve("open_app", "open a jar"), None);
        assert_eq!(resolve("type_text", "Γράψε Καλημέρα σε όλους!"), Some(Action::TypeText("Καλημέρα σε όλους!".into())));
        assert_eq!(resolve("type_text", "write see you tomorrow at nine"), Some(Action::TypeText("see you tomorrow at nine".into())));
        assert_eq!(resolve("type_text", "type in \"thanks for the update\""), Some(Action::TypeText("thanks for the update".into())));
        assert_eq!(resolve("type_text", "write me a short poem about the sea"), None);
        assert_eq!(resolve("type_text", "γράψε μου ένα ποίημα για τη θάλασσα"), None);
        assert_eq!(resolve("screenshot", "snap my screen real quick"), Some(Action::Screenshot));
        assert_eq!(resolve("chat", "tell me a joke"), None);
    }
    #[test] fn router_intents_map_to_actions() {
        assert_eq!(Action::from_intent("open_app", Some(" Steam "), None), Some(Action::OpenApp("Steam".into())));
        assert_eq!(Action::from_intent("type_text", None, Some("")), None);
        assert_eq!(Action::from_intent("chat", None, None), None);
    }
}
