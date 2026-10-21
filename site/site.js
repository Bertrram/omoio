// The API answers cross-origin requests and the download links don't, so the
// newest installer is found here and every download button points straight
// at it. Until then, or if GitHub doesn't answer, they open the releases page.
async function findInstaller() {
  const buttons = document.querySelectorAll("[data-download]");
  const meta = document.getElementById("download-meta");
  const timeout = new AbortController();
  const timer = setTimeout(() => timeout.abort(), 6000);

  try {
    const response = await fetch("https://api.github.com/repos/Bertrram/omoio/releases/latest", { signal: timeout.signal });
    if (!response.ok) throw new Error(`GitHub answered ${response.status}`);
    const release = await response.json();
    const installer = release.assets.find((asset) => asset.name.endsWith("_x64-setup.exe"));
    if (!installer) throw new Error("No installer in the latest release");

    for (const button of buttons) button.href = installer.browser_download_url;
    const version = release.tag_name.replace(/^v/, "");
    // Mebibytes, because that is what Windows shows once the file is saved.
    const size = (installer.size / 1048576).toFixed(1);
    const notes = document.createElement("a");
    notes.href = release.html_url;
    notes.textContent = "Release notes";
    meta.replaceChildren(`Version ${version} · ${size} MB · 64-bit Windows · `, notes);
  } catch {
    meta.textContent = "Opens the releases page on GitHub";
  } finally {
    clearTimeout(timer);
  }
}

// Nothing from YouTube loads until a video is pressed. Each video's link
// carries its YouTube id and the title its player gets.
function playVideoInPlace(event) {
  event.preventDefault();
  const frame = event.currentTarget;
  const player = document.createElement("iframe");
  player.src = `https://www.youtube-nocookie.com/embed/${frame.dataset.video}?autoplay=1&rel=0`;
  player.title = frame.dataset.title;
  player.allow = "autoplay; encrypted-media; picture-in-picture; fullscreen";
  player.allowFullscreen = true;

  const box = document.createElement("div");
  box.className = "video-frame";
  box.append(player);
  frame.replaceWith(box);
  player.focus();
}

// A thank-you while the installer downloads, with the SmartScreen steps and
// the Ko-fi link. Only for the real file: the fallback opens GitHub instead.
function thankForDownload(event) {
  if (!event.currentTarget.href.endsWith(".exe")) return;
  const thanks = document.getElementById("thanks");
  if (typeof thanks.showModal === "function" && !thanks.open) thanks.showModal();
}

// The screens under the hero take turns. The current dot fills while its
// screen is up and the next one comes when it is full, so the timing lives in
// the CSS. Picking a dot or pressing pause stops the turns; with reduced
// motion they never start.
function startTour() {
  const tour = document.querySelector(".tour");
  if (!tour) return;
  const slides = [...tour.querySelectorAll(".tour-slide")];
  const dots = [...tour.querySelectorAll(".tour-dot")];
  const pause = tour.querySelector(".tour-pause");
  let current = 0;

  function show(index) {
    current = (index + slides.length) % slides.length;
    slides.forEach((slide, n) => slide.classList.toggle("on", n === current));
    dots.forEach((dot, n) => {
      dot.classList.toggle("on", n === current);
      if (n === current) dot.setAttribute("aria-current", "true");
      else dot.removeAttribute("aria-current");
    });
  }

  function setPaused(paused) {
    tour.classList.toggle("paused", paused);
    pause.setAttribute("aria-label", paused ? "Play" : "Pause");
  }

  dots.forEach((dot, n) => dot.addEventListener("click", () => {
    setPaused(true);
    show(n);
  }));
  pause.addEventListener("click", () => setPaused(!tour.classList.contains("paused")));
  tour.addEventListener("animationend", (event) => {
    if (event.animationName === "tour-fill" && !tour.classList.contains("paused")) show(current + 1);
  });
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) setPaused(true);
}

// The header gets its own backing once the page scrolls under it.
function watchScroll() {
  const update = () => document.documentElement.classList.toggle("scrolled", window.scrollY > 4);
  addEventListener("scroll", update, { passive: true });
  update();
}

const thanks = document.getElementById("thanks");
thanks.querySelector(".thanks-close").addEventListener("click", () => thanks.close());
thanks.addEventListener("click", (event) => { if (event.target === thanks) thanks.close(); });
for (const button of document.querySelectorAll("[data-download]")) {
  button.addEventListener("click", thankForDownload);
}

if (!/Windows/.test(navigator.userAgent)) {
  document.getElementById("elsewhere").hidden = false;
}

for (const frame of document.querySelectorAll("a[data-video]")) {
  frame.addEventListener("click", playVideoInPlace);
}
watchScroll();
startTour();
findInstaller();
