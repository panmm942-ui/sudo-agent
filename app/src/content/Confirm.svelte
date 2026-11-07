<script lang="ts">
  import type { ConfirmPayload } from "./index";

  /** Confirm (420×150): a question sudo needs answered. Enter confirms, Esc cancels. */
  let {
    confirm,
    onconfirm,
    oncancel,
  }: {
    confirm?: ConfirmPayload;
    onconfirm?: (id: string) => void;
    oncancel?: (id: string) => void;
  } = $props();

  const accept = () => confirm && onconfirm?.(confirm.id);
  const reject = () => confirm && oncancel?.(confirm.id);

  function onkeydown(e: KeyboardEvent) {
    if (e.repeat || e.isComposing || e.defaultPrevented) return;
    // A focused button handles its own Enter/Space.
    if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) {
      e.preventDefault();
      accept();
    } else if (e.key === "Escape") {
      e.preventDefault();
      reject();
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="confirm" role="alertdialog" aria-labelledby="confirm-title" aria-describedby="confirm-body">
  <div class="copy">
    <h2 id="confirm-title" class="title">{confirm?.title ?? ""}</h2>
    {#if confirm?.body}<p id="confirm-body" class="body">{confirm.body}</p>{/if}
  </div>
  <div class="actions">
    <button type="button" class="pill-btn" onclick={reject}>{confirm?.reject ?? "Cancel"}</button>
    <button type="button" class="pill-btn accent" onclick={accept}>{confirm?.accept ?? "Confirm"}</button>
  </div>
</div>

<style>
  .confirm {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    height: 100%;
    padding: 20px 28px 16px;
  }
  .copy {
    min-width: 0;
  }
  .title {
    margin: 0;
    font: var(--type-title);
    letter-spacing: var(--tracking-tight);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .body {
    margin: var(--space-1) 0 0;
    font: var(--type-body);
    color: var(--text-secondary);
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
  .actions .pill-btn {
    min-width: 88px;
  }
</style>
