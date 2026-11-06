# Faster loudness response

User explicitly requests more reactivity. Root cause: seven cascaded attack/release filters suppress brief measured peaks before later bars move. Change all seven to direct measured peak with16–31msattack/65–101msrelease; increase quiet-level log sensitivity. Keep bar layout, real scalar meter,20Hzsampling,frame loop,allocation-free storage andmetadata-stablegate.

Regressionoldchainfailedlastbar80msresponse;newfiltertestrequiresallbars>0.9by80msafterpeakreceiptand<0.23after160msquiet.209frontendtestspass,check0errors/0warnings;buildandnormalrelaunch. No claimed end-to-end audiolatency or native CPU measurements.
