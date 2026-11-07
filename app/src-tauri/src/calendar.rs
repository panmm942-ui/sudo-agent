//! Calendar subscriptions: downloads the user's iCalendar (.ics) feeds — the private address from
//! Google Calendar, Outlook or iCloud. Parsing and reminders happen in the island (calendar.ts).
use std::io::Read;
use std::time::Duration;

const MAX_BYTES: u64 = 6_000_000;

pub(crate) fn feed_url(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    let url = if let Some(rest) = raw.strip_prefix("webcal://") { format!("https://{rest}") } else { raw.to_string() };
    let parsed = reqwest::Url::parse(&url).map_err(|_| "That isn't a calendar address.".to_string())?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" { return Err("Use the calendar's https:// or webcal:// address.".into()); }
    if parsed.host_str().is_none_or(|h| h == "localhost" || h.starts_with("127.") || h.starts_with("10.") || h.starts_with("192.168.")) {
        return Err("Use the calendar's public address.".into());
    }
    Ok(url)
}

#[tauri::command]
pub async fn calendar_fetch(url: String) -> Result<String, String> {
    let url = feed_url(&url)?;
    tauri::async_runtime::spawn_blocking(move || {
        let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(20)).connect_timeout(Duration::from_secs(8))
            .user_agent("SudoIsland calendar (Windows)").build().map_err(|_| "Calendar unavailable.".to_string())?;
        let response = client.get(&url).send().map_err(|_| "The calendar didn't answer. Check the address and your connection.".to_string())?;
        if !response.status().is_success() { return Err(format!("The calendar answered {}. Check the address.", response.status().as_u16())); }
        let mut text = String::new();
        response.take(MAX_BYTES).read_to_string(&mut text).map_err(|_| "The calendar sent something that isn't text.".to_string())?;
        if !text.contains("BEGIN:VCALENDAR") { return Err("That address isn't an iCalendar (.ics) feed.".into()); }
        Ok(text)
    }).await.map_err(|_| "Calendar unavailable.".to_string())?
}

#[cfg(test)]
mod tests {
    #[test]
    fn webcal_becomes_https_and_local_addresses_are_refused() {
        assert_eq!(super::feed_url("webcal://p01-caldav.icloud.com/x.ics").unwrap(), "https://p01-caldav.icloud.com/x.ics");
        assert!(super::feed_url("file:///C:/x.ics").is_err());
        assert!(super::feed_url("http://127.0.0.1/x.ics").is_err());
    }
}
