# Screenshot, two-scroll navigation, search, Greek lyrics and personal program

User explicitly specifies behavior and retains prior glass direction. Implement directly in existing app. Two Home upward wheel steps hide without pause after Spotify→Home; normalize wheel modes, ignore tiny noise. Lyrics-up closes lyrics and goesHome. Center rail vertically at right, gear separate. Search icon toggles contained search view and results, with Escape/second icon click close. No external popup outside pill.

Screenshots: WDA_EXCLUDEFROMCAPTURE is necessary for live monitor refraction, so common screenshot shortcuts temporarily expose window and freeze background, then restore. Add manual screenshot mode for other capture tools; explain frozen backdrop while enabled. Shader blur preference0–100 changes existing fixedsample frost radius without extra passes.

Lyrics: keep precise metadata first; normalize title decorations/artist collaborators, search LRCLIB by title and filter ranked results against title/artist/duration. Greek-aware Unicode normalization/transliteration; never accept unrelated recordings or invent timestamps. Plain-only lyrics remain readable instead of hidden behind timed-unavailable message. Bound requests/abort/cache.

Program: editable local weekly reminders withstableID,label,detail,time,repeatdays,enabled. Preserve personal legacy routine untilfirstedit/import;custom weekly schedule repeats indefinitely without84day expiry. Dayfilter,new/edit/delete/undo,emptyprogramsupported. Customscheduleusesfixedtimes. Savedalarmprogress keyedbydate+entryID andremoved/disabledentriescannotkeep ringinglater.

Audio:download real licensedclock/alarm recordings,bundle smallMP3s forofflineplayback,preservecustomupload/volume/fade/stop. Recordsource/license andhashes. Search results require catalogueaccess:verifyavailablepublicmetadataandshowprovidertruthfully;Spotifyplayback/searchURI remainsnative untilOAuthconnectionexists.

Validate relevant regressiontests first, frontend/nativechecks, liveproviderprobeswithoutprintinglyrics, browserinteraction+fixedpillgeometry/reducedmotion/console, buildandnormalWindowslaunch. Preservepreexistingdirtywork,no commits/push.
