'use strict';

(() => {
  const storageKey = 'sudo.companion.phoneKey';
  const $ = (id) => document.getElementById(id);
  const isGuide = document.body.dataset.view === 'clock';
  let phoneKey = '';
  try { phoneKey = localStorage.getItem(storageKey) || ''; } catch { /* Current-session pairing remains possible. */ }
  let state = null;
  let busy = false;
  let fetching = false;
  let generation = 0;
  let readRevision = 0;
  let serverOffset = 0;
  let noteDirty = false;
  let minutesDirty = false;
  let pendingPair = new URLSearchParams(location.hash.slice(1)).get('pair');
  if (location.hash) history.replaceState(null, '', location.pathname + location.search);
  const values = { actionUrl: `${location.origin}/v1/action`, feedUrl: `${location.origin}/v1/alarms`, ackUrl: `${location.origin}/v1/ack`, phoneKey, ownerPrefix: '' };
  const now = () => Date.now() + serverOffset;
  const dateText = (value) => {
    if (!Number.isFinite(value) || value <= 0) return 'Not available yet';
    return new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' }).format(value);
  };
  const feedback = (id, text, error = false) => {
    const element = $(id);
    if (!element) return;
    element.textContent = text;
    element.dataset.error = String(error);
  };
  function pairedView(paired) {
    if ($('pair-view')) $('pair-view').hidden = paired;
    if ($('companion-view')) $('companion-view').hidden = !paired;
    if ($('guide-content')) $('guide-content').hidden = !paired;
    if ($('guide-unpaired')) $('guide-unpaired').hidden = paired;
    fillValues();
  }
  function fillValues() {
    values.phoneKey = phoneKey;
    document.querySelectorAll('[data-value]').forEach((element) => {
      const name = element.dataset.value;
      element.textContent = name === 'ownerPrefix' ? (values[name] || 'Unavailable until a valid alarm feed is published.') : (values[name] || '');
    });
    if ($('phone-key')) $('phone-key').value = phoneKey;
    document.querySelectorAll('[data-copy="ownerPrefix"]').forEach((button) => { button.disabled = !values.ownerPrefix; });
  }
  function clearKey(message) {
    phoneKey = '';
    generation += 1;
    try { localStorage.removeItem(storageKey); } catch { /* Already forgotten in memory. */ }
    values.ownerPrefix = '';
    pairedView(false);
    feedback('pair-status', message);
    feedback('guide-unpaired', message);
    if ($('pair-retry')) $('pair-retry').hidden = true;
  }
  async function request(path, body, unauthenticated = false) {
    if (!isSecureContext) throw new Error('Open your companion using its secure HTTPS address.');
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 10000);
    try {
      const response = await fetch(path, {
        method: body === undefined ? 'GET' : 'POST',
        headers: { ...(unauthenticated ? {} : { Authorization: `Bearer ${phoneKey}` }), ...(body === undefined ? {} : { 'Content-Type': 'application/json' }) },
        body: body === undefined ? undefined : JSON.stringify(body),
        credentials: 'omit', cache: 'no-store', redirect: 'error', signal: controller.signal,
      });
      const serverDate = Date.parse(response.headers.get('Date') || '');
      if (Number.isFinite(serverDate)) serverOffset = serverDate - Date.now();
      let result;
      try { result = await response.json(); } catch { result = null; }
      if (response.status === 401 && !unauthenticated) {
        clearKey('This phone’s access has expired or been revoked. Pair again from Online companion in Sudo on your PC. Your unsaved text is kept on this page.');
        throw new Error('Pair this phone again from Sudo on your PC.');
      }
      if (!response.ok) {
        const messages = { 400: 'This request could not be accepted. Check the values and try again.', 403: 'This phone cannot perform that action.', 409: 'The schedule is not ready for this import. Check Sudo on your PC.', 410: 'Stored schedule coverage has expired. Open Sudo on your PC to publish a fresh schedule.', 413: 'This text is too long. Keep it within 2,000 characters.', 429: 'Too many requests. Wait a moment and try again.', 503: 'The schedule is not available yet. Open Sudo on your PC to refresh it.' };
        throw new Error(messages[response.status] || 'The companion service could not complete this request. Try again.');
      }
      return result;
    } catch (error) {
      if (error.name === 'AbortError') throw new Error('The connection timed out. Your unsaved text is still here.');
      if (error instanceof TypeError) throw new Error('Could not reach your companion. Check your connection; your unsaved text is still here.');
      throw error;
    } finally { clearTimeout(timeout); }
  }
  function connectionStatus(text, status) {
    if (!$('connection-status')) return;
    $('connection-status').textContent = text;
    $('connection-status').dataset.state = status;
  }
  function render(next) {
    if (!next || typeof next !== 'object') throw new Error('The companion returned an unreadable response. Try again.');
    state = next;
    if (isGuide) return;
    if (!noteDirty) {
      $('note').value = next.note?.text || '';
      feedback('note-feedback', next.note?.updatedAt ? `Saved ${dateText(next.note.updatedAt)}` : 'Shared with Sudo on your PC.');
    }
    $('note-save').disabled = busy || !noteDirty;
    if (!minutesDirty && next.focus?.minutes) $('focus-minutes').value = next.focus.minutes;
    $('last-contact').textContent = next.lastDesktopAt ? dateText(next.lastDesktopAt) : 'Waiting for your PC';
    $('source-updated').textContent = dateText(next.sourceUpdatedAt);
    const covered = Number.isFinite(next.coverageUntil) && next.coverageUntil > now();
    $('coverage').textContent = next.coverageUntil ? `${covered ? 'Until' : 'Expired'} ${dateText(next.coverageUntil)}` : 'No schedule published yet';
    $('last-import').textContent = next.lastImportedAt ? `${dateText(next.lastImportedAt)} · ${next.importedCount || 0} alarms reported` : 'No completed import reported';
    const battery = next.battery;
    $('battery-status').textContent = battery ? `${battery.percent}% · ${battery.charging ? 'charging' : 'not charging'} · reported ${dateText(battery.updatedAt)}` : 'No battery report yet';
    const count = Array.isArray(next.inbox) ? next.inbox.length : 0;
    $('inbox-status').textContent = count ? `${count} ${count === 1 ? 'item' : 'items'} waiting in your PC inbox.` : 'Your PC inbox is clear.';
    const contactAge = now() - (next.lastDesktopAt || 0);
    connectionStatus(contactAge < 45000 ? 'PC checked in recently' : covered ? 'Using stored PC schedule' : 'Waiting for a PC update', 'connected');
    $('global-error').hidden = true;
    renderCountdown();
  }
  function renderCountdown() {
    if (isGuide || !state || !phoneKey || document.hidden) return;
    const focus = state.focus || {};
    const running = Number.isFinite(focus.deadline) && focus.deadline > 0;
    const remaining = Math.max(0, running ? focus.deadline - now() : (focus.remaining ?? (focus.minutes || 25) * 60000));
    const seconds = Math.ceil(remaining / 1000);
    $('countdown').textContent = `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`;
    $('countdown').setAttribute('aria-label', `${Math.floor(seconds / 60)} minutes ${seconds % 60} seconds remaining`);
    $('focus-state').textContent = running && remaining > 0 ? 'In focus' : remaining === 0 ? 'Complete' : 'Focus';
    $('focus-caption').textContent = running && remaining > 0 ? `Until ${new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' }).format(focus.deadline)}` : remaining === 0 ? 'A moment to pause. Ready for another?' : 'Choose a little uninterrupted time.';
    if (!busy) $('focus-start').textContent = running && remaining > 0 ? 'Pause' : 'Start focus';
    $('focus-minutes').disabled = busy || (running && remaining > 0);
  }
  async function refresh() {
    if (!phoneKey || fetching || busy || document.hidden || isGuide) return;
    fetching = true;
    const session = generation;
    const revision = readRevision;
    $('refresh').disabled = true;
    try { const next = await request('/v1/state'); if (session === generation && revision === readRevision) render(next); }
    catch (error) {
      if (phoneKey) {
        connectionStatus('Connection unavailable', 'error');
        $('global-error').hidden = false;
        $('global-error').textContent = error.message;
      }
    } finally { fetching = false; $('refresh').disabled = false; }
  }
  async function action(body, button, feedbackId, during, onSuccess) {
    if (busy || !phoneKey) return;
    busy = true;
    readRevision += 1;
    const label = button.textContent;
    const session = generation;
    button.textContent = during;
    [$('note-save'), $('focus-start'), $('focus-reset'), $('send-button'), $('refresh')].forEach((element) => { if (element) element.disabled = true; });
    feedback(feedbackId, during);
    try {
      const next = await request('/v1/action', body);
      if (session !== generation) return;
      if (onSuccess) onSuccess();
      if (body.type === 'focus') minutesDirty = false;
      render(next);
      feedback(feedbackId, body.type === 'send' ? 'Sent. It will appear in Sudo’s PC inbox.' : body.type === 'note' ? (noteDirty ? 'Saved. You have new unsaved edits.' : 'Saved to your shared note.') : 'Focus updated.');
    } catch (error) { feedback(feedbackId, error.message, true); }
    finally {
      busy = false;
      button.textContent = label;
      [$('focus-start'), $('focus-reset'), $('send-button'), $('refresh')].forEach((element) => { if (element) element.disabled = false; });
      if ($('note-save')) $('note-save').disabled = !noteDirty;
      renderCountdown();
    }
  }
  async function claim() {
    if (!pendingPair) return;
    feedback('pair-status', 'Pairing this phone…');
    if ($('pair-retry')) $('pair-retry').hidden = true;
    try {
      const result = await request('/v1/pair/claim', { code: pendingPair }, true);
      if (!result || typeof result.phoneKey !== 'string' || !result.phoneKey) throw new Error('Pairing returned no access key. Create a new pairing code on your PC.');
      phoneKey = result.phoneKey;
      generation += 1;
      pendingPair = null;
      let saved = true;
      try { localStorage.setItem(storageKey, phoneKey); } catch { saved = false; }
      pairedView(true);
      await refresh();
      if (!saved && $('global-error')) { $('global-error').textContent = 'This browser could not save the key. Access lasts for this page session; allow local storage before pairing again.'; $('global-error').hidden = false; }
    } catch (error) {
      pairedView(false);
      feedback('pair-status', `${error.message} Pairing codes are single use and expire after 10 minutes; create a fresh one in Sudo if needed.`, true);
      if ($('pair-retry')) $('pair-retry').hidden = false;
    }
  }
  async function loadGuide() {
    if (!phoneKey) return;
    feedback('guide-status', 'Checking your stored alarm feed…');
    try {
      const manifest = await request('/v1/alarms');
      if (manifest?.schema !== 'sudo.iphone-alarms.v1' || typeof manifest.ownerPrefix !== 'string' || !manifest.ownerPrefix || manifest.ackUrl !== values.ackUrl) throw new Error('The alarm feed did not pass validation. Open Sudo on your PC and refresh the connection.');
      values.ownerPrefix = manifest.ownerPrefix;
      fillValues();
      feedback('guide-status', `Stored PC schedule updated ${dateText(manifest.sourceUpdatedAt)}. Coverage until ${dateText(manifest.coverageUntil)}. This check does not import alarms into Clock.`);
    } catch (error) { feedback('guide-status', error.message, true); }
  }
  document.querySelectorAll('[data-copy]').forEach((button) => {
    button.addEventListener('click', async () => {
      const value = values[button.dataset.copy];
      if (!value) return;
      try { await navigator.clipboard.writeText(value); feedback('copy-feedback', `${button.dataset.copy === 'phoneKey' ? 'Phone key' : button.dataset.copy === 'ownerPrefix' ? 'Exact owner prefix (including its final space)' : 'URL'} copied.`); }
      catch { feedback('copy-feedback', 'Clipboard access was unavailable. Show the value and select it to copy manually.', true); }
    });
  });
  $('key-reveal')?.addEventListener('click', () => {
    const show = $('phone-key').type === 'password';
    $('phone-key').type = show ? 'text' : 'password';
    $('key-reveal').textContent = show ? 'Hide' : 'Show';
    $('key-reveal').setAttribute('aria-pressed', String(show));
  });
  document.addEventListener('visibilitychange', () => {
    if (document.hidden && $('phone-key')) {
      $('phone-key').type = 'password';
      $('key-reveal').textContent = 'Show';
      $('key-reveal').setAttribute('aria-pressed', 'false');
    }
    if (!document.hidden) { if (isGuide) loadGuide(); else { refresh(); renderCountdown(); } }
  });
  if (!isGuide) {
    $('pair-retry').addEventListener('click', claim);
    $('refresh').addEventListener('click', refresh);
    $('focus-minutes').addEventListener('input', () => { minutesDirty = true; });
    $('note').addEventListener('input', () => { noteDirty = $('note').value !== (state?.note?.text || ''); $('note-save').disabled = busy || !noteDirty; feedback('note-feedback', noteDirty ? 'Unsaved changes' : 'Shared with Sudo on your PC.'); });
    $('note-save').addEventListener('click', () => {
      const text = $('note').value;
      action({ type: 'note', text }, $('note-save'), 'note-feedback', 'Saving…', () => { noteDirty = $('note').value !== text; });
    });
    $('send-form').addEventListener('submit', (event) => {
      event.preventDefault();
      const text = $('send-text').value;
      if (!text.trim()) { feedback('send-feedback', 'Add a link or a few words first.', true); return; }
      action({ type: 'send', text }, $('send-button'), 'send-feedback', 'Sending…', () => { if ($('send-text').value === text) $('send-text').value = ''; });
    });
    $('focus-start').addEventListener('click', () => {
      const running = state?.focus?.deadline > now();
      const minutes = Number($('focus-minutes').value);
      if (!running && (!Number.isInteger(minutes) || minutes < 1 || minutes > 180)) { feedback('focus-feedback', 'Choose a whole number from 1 to 180 minutes.', true); $('focus-minutes').focus(); return; }
      const resume = !running && state?.focus?.remaining > 0 && state.focus.remaining < state.focus.minutes * 60000 && minutes === state.focus.minutes;
      action({ type: 'focus', action: running ? 'pause' : 'start', ...(!running && !resume ? { minutes } : {}) }, $('focus-start'), 'focus-feedback', running ? 'Pausing…' : 'Starting…');
    });
    $('focus-reset').addEventListener('click', () => action({ type: 'focus', action: 'reset' }, $('focus-reset'), 'focus-feedback', 'Resetting…'));
    $('forget-phone').addEventListener('click', () => clearKey('This browser has been disconnected. Pair again from Sudo on your PC whenever you’re ready.'));
    setInterval(refresh, 15000);
    setInterval(renderCountdown, 1000);
  } else $('guide-refresh')?.addEventListener('click', loadGuide);
  pairedView(Boolean(phoneKey));
  if (pendingPair && !isGuide) { pairedView(false); claim(); }
  else if (phoneKey) { if (isGuide) loadGuide(); else refresh(); }
})();
