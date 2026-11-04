/// The few helpers every menu over a game builds its page with.

export function node<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text?: string): HTMLElementTagNameMap[K] {
  const made = document.createElement(tag);
  made.className = className;
  if (text !== undefined) made.textContent = text;
  return made;
}

export function svg(shape: string): string {
  return `<svg viewBox="0 0 24 24" aria-hidden="true">${shape}</svg>`;
}

export function image(source: string): HTMLImageElement {
  const made = node("img", "");
  made.src = source;
  made.alt = "";
  made.decoding = "async";
  return made;
}
