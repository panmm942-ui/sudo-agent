# Connect Sudo alarms to iPhone Clock

Open **Settings → Alarm → iPhone Clock** in the Windows Sudo app. Enable **Connect to iPhone Clock**, then scan the QR using your iPhone Camera and open the private setup page in Safari. Keep your PC and phone on the same trusted home network.

The setup page contains your connection values and the complete guided Shortcuts recipe. Create it once in the iPhone Shortcuts app using Apple’s built-in Clock and web-request actions. No unsigned `.shortcut` file is presented as an installable shortcut. Native iPhone import still needs to be verified on the actual phone; Windows-side tests cannot substitute for that.

## Refresh behavior

- Sudo is the schedule source. The phone Shortcut reads the next 24 hours and creates non-repeating Clock alarms.
- Alarms less than a minute away are excluded so setup does not accidentally schedule a passed time for the following day. Use the existing PC alarm for an imminent reminder.
- Use both a daily Time of Day automation and a home Wi-Fi automation, configured to run automatically. Run the Shortcut manually after editing alarms and before leaving home. There is no constant background polling on the phone and no instant PC-to-Clock push.
- Keep PC and phone in the same time zone. The feed blocks imports across differing UTC offsets during daylight-saving changes; refresh after the time change.
- The PC must be awake, Sudo running, and the connection enabled during each sync. Imported alarms can ring later without the PC or Wi-Fi. Missing a refresh means later alarms have not been scheduled.
- The Shortcut captures only alarms matching this installation’s exact Sudo prefix. It creates replacements before deleting that captured old set. If the network fetch, validation or creation fails, stop the Shortcut; do not delete the previous set or acknowledge success. A partially failed creation can leave duplicates; run the complete Shortcut again to replace the Sudo set.
- Leave the Clock action’s Repeat option off. Other Clock alarms, Sleep schedules, ringer volume, sounds, snooze, and dismissal are managed on the iPhone separately.
- Turning off the PC connection closes the listener. Alarms already imported on the phone remain until removed in Clock or replaced by another successful sync. To clear them through the connection, disable Program alarms in Sudo, successfully run the Shortcut once to import an empty schedule, then disable the connection.

## Connection troubleshooting

The local service listens on TCP **43894** only when enabled. Windows Firewall may require allowing Sudo on **Private networks**. Sudo does not automatically add firewall rules, open router ports, or request administrator access. Guest Wi-Fi, VPNs and router client isolation can prevent devices from reaching one another. If your PC has multiple network addresses, choose another address under the QR in Settings. After your PC’s IP address changes, reconnect the option and update the Shortcut URL.

The feed uses a private pairing key over local HTTP. Treat the QR, setup URL and key as private; use a trusted home network. The server rejects public-address clients, unknown hosts, unauthenticated feed access, browser-origin API calls, oversized/ambiguous requests and stale schedules. It exposes no command execution or files.

**Schedule fetched** means the phone requested the data. **Imported into Clock** changes only after the Shortcut’s final acknowledgement succeeds. An acknowledgement is a report from the paired Shortcut, not an independent inspection of the iPhone’s Clock database.

## Apple references

- [Shortcuts Clock actions](https://support.apple.com/en-ca/101583)
- [Find Alarm and Delete Alarms](https://support.apple.com/en-ca/111098)
- [iOS 27 automation triggers and running while locked](https://support.apple.com/en-om/guide/shortcuts/apdfbdbd7123/10.0/ios/27)

AlarmKit creates alarms for an iOS app; this integration deliberately uses Clock actions in Shortcuts to populate the iPhone’s existing Clock app.
