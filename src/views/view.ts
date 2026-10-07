export interface View {
  title: string;
  subtitle: string;
  content: HTMLElement;
}

export function emptyState(title: string, subtitle: string): HTMLElement {
  const el = document.createElement("div");
  el.className = "empty";
  el.innerHTML = `<div class="empty-t"></div><div class="empty-s"></div>`;
  el.querySelector(".empty-t")!.textContent = title;
  el.querySelector(".empty-s")!.textContent = subtitle;
  return el;
}

/// A labelled row of choices, one of them on, as the catalogue's filters and
/// the library's console filter are.
export function chips<T>(
  label: string,
  options: [T, string][],
  current: T,
  choose: (value: T) => void
): HTMLElement {
  const group = document.createElement("div");
  group.className = "filter-group";
  const name = document.createElement("span");
  name.className = "filter-label";
  name.textContent = label;
  group.appendChild(name);
  for (const [value, text] of options) {
    const button = document.createElement("button");
    button.className = value === current ? "chip on" : "chip";
    button.textContent = text;
    button.onclick = () => choose(value);
    group.appendChild(button);
  }
  return group;
}
