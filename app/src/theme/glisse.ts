import './glisse.css';

/** Publish each range input's fill fraction as --lg-progress for the Glisse slider rail.
 * Values change both from user input and from code (Svelte sets `.value`), so a
 * light periodic pass complements the input listener. Returns a stop function. */
export function startGlisseProgress(root: Document = document): () => void {
  const update = (input: HTMLInputElement) => {
    const min = Number(input.min || 0), max = Number(input.max || 100), value = Number(input.value);
    const fraction = max > min && Number.isFinite(value) ? Math.max(0, Math.min(1, (value - min) / (max - min))) : 0;
    const next = fraction.toFixed(4);
    if (input.style.getPropertyValue('--lg-progress') !== next) input.style.setProperty('--lg-progress', next);
  };
  const all = () => root.querySelectorAll<HTMLInputElement>('input[type="range"]').forEach(update);
  const onInput = (event: Event) => { if (event.target instanceof HTMLInputElement && event.target.type === 'range') update(event.target); };
  root.addEventListener('input', onInput, true);
  all();
  const timer = setInterval(all, 300);
  return () => { root.removeEventListener('input', onInput, true); clearInterval(timer); };
}

/** Switch the whole island between Sudo's standard look and the Glisse design. */
export function applyDesign(design: 'standard' | 'glisse', root: HTMLElement = document.documentElement): void {
  if (design === 'glisse') root.dataset.design = 'glisse';
  else delete root.dataset.design;
}
