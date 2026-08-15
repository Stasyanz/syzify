/** Copy text to the clipboard. The async Clipboard API needs a secure
 * context and can be missing in webviews, so fall back to a hidden
 * textarea + execCommand("copy"). Returns false when both paths fail. */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // fall through to execCommand
  }
  const previousFocus = document.activeElement;
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.setAttribute("readonly", "");
  ta.style.position = "fixed";
  ta.style.top = "0";
  ta.style.left = "-9999px";
  ta.style.opacity = "0";
  let ok = false;
  try {
    document.body.appendChild(ta);
    ta.select();
    ok = document.execCommand("copy");
  } catch {
    ok = false;
  } finally {
    ta.remove();
    if (previousFocus instanceof HTMLElement) {
      previousFocus.focus();
    }
  }
  return ok;
}
