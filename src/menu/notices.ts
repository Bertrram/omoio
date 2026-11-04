import { image, node } from "./dom";

/// What the menu is doing or has done, shown as a card in the corner of the
/// screen like a Windows notification, large enough to read from a sofa.
/// Work under way and hints stay until something replaces them; a finished
/// job, a problem and a word about why nothing happened go by themselves.
export interface Notice {
  kind: "working" | "done" | "problem" | "hint" | "info";
  title: string;
  detail?: string;
  /// The figure it is about, its picture's layers bottom first: a swapper
  /// is two. Left out when Omoio has no picture of a layer.
  picture?: (string | null)[];
  /// A job done that earns a cheer, with the page's own shape for its mark,
  /// as a vehicle SuperCharged has the lightning.
  cheer?: string;
}

const NOTICE_LASTS: Partial<Record<Notice["kind"], number>> = { done: 3500, problem: 8000, info: 5000 };

/// Each kind's mark: a tick for done and an arrow for a hint, the shapes
/// the portal menu's tiles use for "On the portal" and "Top picked" (BADGES
/// in src/portal.ts).
export const NOTICE_SHAPES: Record<Exclude<Notice["kind"], "working">, string> = {
  done: `<path d="m5 12.5 4.5 4.5L19 7.5" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>`,
  problem: `<path d="M12 6v7.5M12 18h.01" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"/>`,
  hint: `<path d="M12 3.5 20 13h-5v7.5H9V13H4z" fill="currentColor"/>`,
  info: `<path d="M12 11v6.5M12 6.5h.01" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"/>`,
};

const card = node("div", "portal-notice");
card.setAttribute("role", "status");
document.body.appendChild(card);
/// The notice up now, or `null`.
export let notice: Notice | null = null;
let noticeTimer = 0;

/// The kind's mark: a spinner while working, a shape otherwise.
function noticeMark(kind: Notice["kind"], cheer?: string): HTMLElement {
  if (kind === "working") return node("span", "portal-spinner");
  const mark = node("span", "portal-notice-shape");
  mark.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true">${cheer ?? NOTICE_SHAPES[kind]}</svg>`;
  return mark;
}

/// Shows a notice in place of the one up now, or with `null` takes it away.
export function notify(next: Notice | null) {
  window.clearTimeout(noticeTimer);
  notice = next;
  if (!next) {
    card.classList.remove("shown");
    return;
  }
  const art = node("span", "portal-notice-art");
  const layers = next.picture ?? [];
  if (layers.length > 0 && layers.every(Boolean)) {
    for (const source of layers) art.appendChild(image(source!));
    art.appendChild(node("span", "portal-notice-corner")).appendChild(noticeMark(next.kind, next.cheer));
  } else {
    art.appendChild(noticeMark(next.kind, next.cheer));
  }
  const words = node("span", "portal-notice-words");
  words.append(node("strong", "", next.title));
  if (next.detail) words.append(node("span", "", next.detail));
  card.replaceChildren(art, words);
  card.className = `portal-notice ${next.kind}${next.cheer ? " cheer" : ""} shown`;
  const lasts = NOTICE_LASTS[next.kind];
  if (lasts) noticeTimer = window.setTimeout(() => notify(null), lasts);
}

/// A problem in the backend's own words, which are written for the player,
/// or `otherwise` when it gave none.
export function problem(err: unknown, otherwise: string): Notice {
  return { kind: "problem", title: typeof err === "string" ? err : otherwise };
}
