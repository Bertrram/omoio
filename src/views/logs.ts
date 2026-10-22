import {
  EMULATOR_OF,
  listSessions,
  readSessionLog,
  sessionPrompt,
  type PlaySession,
} from "../api";
import { emptyState, type View } from "./view";

function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
}

function formatWhen(unixSeconds: string): string {
  const at = new Date(Number(unixSeconds) * 1000);
  if (Number.isNaN(at.getTime())) return "";
  return at.toLocaleString(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

const ENDING: Record<PlaySession["ending"], { label: string; tone: string }> = {
  stopped: { label: "Stopped from Omoio", tone: "" },
  closed: { label: "Closed normally", tone: "go" },
  crashed: { label: "Ended unexpectedly", tone: "warn" },
  refused: { label: "Didn't start", tone: "warn" },
};

async function copy(text: string, button: HTMLButtonElement, done: string): Promise<void> {
  await navigator.clipboard.writeText(text);
  const was = button.textContent;
  button.textContent = done;
  setTimeout(() => (button.textContent = was), 1600);
}

function sessionCard(session: PlaySession): HTMLElement {
  const ending = ENDING[session.ending];
  const card = document.createElement("div");
  card.className = "session";
  card.innerHTML = `
    <div class="session-head">
      <div>
        <div class="session-title"></div>
        <div class="session-meta">
          <span class="id">${session.title_id}</span>
          <span>· ${formatWhen(session.started)}</span>
          <span>· played ${formatDuration(session.seconds)}</span>
        </div>
      </div>
      <span class="session-ending ${ending.tone}">${ending.label}</span>
    </div>
    <div class="session-problems"></div>
    <div class="row-actions">
      <button class="small-btn" data-act="prompt">Copy prompt for an AI</button>
      <button class="small-btn" data-act="log">Copy full log</button>
      <button class="small-btn" data-act="show">Show errors</button>
    </div>
  `;
  card.querySelector<HTMLElement>(".session-title")!.textContent = session.title;

  // Sessions kept before the console was recorded were all PS3 (api.ts).
  const emulator = EMULATOR_OF[session.console ?? "ps3"];
  const problems = card.querySelector<HTMLElement>(".session-problems")!;
  const summary = document.createElement("div");
  summary.className = "session-summary";
  // Only RPCS3's log is read for errors (read_problems in core/playlog.rs
  // looks for its own marks), so for another emulator a problem is Omoio's
  // own words for why the game didn't start (keep_session_log in
  // session.rs), and is said in full.
  const ourWords = session.console !== undefined && session.console !== "ps3";
  summary.textContent =
    session.problems.length === 0
      ? `${emulator} logged no errors.`
      : ourWords
        ? session.problems.join(" ")
        : `${session.problems.length} error${session.problems.length === 1 ? "" : "s"} logged.`;
  problems.appendChild(summary);

  const list = document.createElement("pre");
  list.className = "session-list";
  list.hidden = true;
  list.textContent = session.problems.join("\n");
  problems.appendChild(list);

  const show = card.querySelector<HTMLButtonElement>('[data-act="show"]')!;
  show.disabled = session.problems.length === 0;
  // Nothing more to show than the summary says.
  if (ourWords) show.classList.add("gone");
  show.onclick = () => {
    list.hidden = !list.hidden;
    show.textContent = list.hidden ? "Show errors" : "Hide errors";
  };

  const promptButton = card.querySelector<HTMLButtonElement>('[data-act="prompt"]')!;
  // The prompt describes how Omoio drives RPCS3, so it is offered for a PS3
  // session only. Another emulator's session would get advice for the wrong one.
  if (session.console && session.console !== "ps3") promptButton.classList.add("gone");
  promptButton.onclick = async () => {
    // Built in the backend from the log itself, so it says only what was recorded.
    await copy(await sessionPrompt(session.log_file), promptButton, "Copied");
  };

  const logButton = card.querySelector<HTMLButtonElement>('[data-act="log"]')!;
  logButton.onclick = async () => {
    await copy(await readSessionLog(session.log_file), logButton, "Copied");
  };

  return card;
}

export async function renderLogs(): Promise<View> {
  const sessions = await listSessions();

  const content = document.createElement("div");
  if (sessions.length === 0) {
    content.appendChild(
      emptyState(
        "Nothing recorded yet",
        "Play a game and Omoio keeps what the emulator said about it, so you can look into it afterwards."
      )
    );
  } else {
    // The list can hold sessions from every emulator, so the note names none.
    // RPCS3 and Cemu each start their log afresh when they start (RPCS3
    // opens it with fs::rewrite, logs::file_writer in rpcs3/util/logs.cpp,
    // v0.0.43-20267, commit ff91404b; Cemu 2.6 opens log.txt with
    // std::ios::out, cemuLog_createLogFile in
    // src/Cemu/Logging/CemuLogging.cpp), and Dolphin only adds to its log, so
    // Omoio empties it before each game (backends/dolphin/log.rs).
    const note = document.createElement("div");
    note.className = "notice plain";
    note.textContent =
      "Each emulator's log starts afresh with every game. Omoio keeps a copy of every session so a crash is still there afterwards.";
    content.appendChild(note);
    sessions.forEach((session) => content.appendChild(sessionCard(session)));
  }

  const crashes = sessions.filter((s) => s.ending === "crashed").length;
  const subtitle =
    sessions.length === 0
      ? "No sessions yet"
      : `${sessions.length} session${sessions.length === 1 ? "" : "s"}${crashes > 0 ? ` · ${crashes} ended unexpectedly` : ""}`;

  return { title: "Logs", subtitle, content };
}
