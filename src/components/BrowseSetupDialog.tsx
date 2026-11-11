import { useEffect, useState } from "react";
import { Icon } from "@iconify/react";
import { api } from "../lib/ipc";

const SETUP_EVENT = "sinew:browse-setup";
const DISMISSED_KEY = "sinew.browseSetupDismissed";
const SETUP_PAGE = "https://opencode.ai/product/browse";
const INSTALL_COMMAND = "npx @opencode/browse install";

type SetupRequest = { onContinue?: () => void };

function dismissed(): boolean {
  try {
    return localStorage.getItem(DISMISSED_KEY) === "1";
  } catch {
    return false;
  }
}

/// Runs `onContinue` (open the browser panel, turn browser mode on), first
/// offering to set up OpenCode Browse while it is off and not dismissed.
export function withBrowseSetup(onContinue: () => void) {
  if (dismissed()) {
    onContinue();
    return;
  }
  void api
    .browseSetupStatus()
    .then((status) => {
      if (status.enabled) onContinue();
      else window.dispatchEvent(new CustomEvent<SetupRequest>(SETUP_EVENT, { detail: { onContinue } }));
    })
    .catch(() => onContinue());
}

/// Explains the two browsers and walks through OpenCode Browse: the
/// built-in one is the agent's own (no logins, blocked by anti-bot checks);
/// Browse lets the agent use the user's real Chrome.
export function BrowseSetupDialog() {
  const [request, setRequest] = useState<SetupRequest | null>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const open = (event: Event) => {
      setCopied(false);
      setError(null);
      setRequest((event as CustomEvent<SetupRequest>).detail ?? {});
    };
    window.addEventListener(SETUP_EVENT, open);
    return () => window.removeEventListener(SETUP_EVENT, open);
  }, []);

  useEffect(() => {
    if (!request) return undefined;
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setRequest(null);
    };
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, [request]);

  if (!request) return null;

  const finish = () => {
    const next = request.onContinue;
    setRequest(null);
    next?.();
  };

  const notNow = () => {
    try {
      localStorage.setItem(DISMISSED_KEY, "1");
    } catch {
      // The dialog simply comes back next time.
    }
    finish();
  };

  const enable = () => {
    void api
      .enableBrowseMcp()
      .then(finish)
      .catch((err) => setError(err instanceof Error ? err.message : String(err)));
  };

  return (
    <div className="browse-setup" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && setRequest(null)}>
      <div className="browse-setup__panel" role="dialog" aria-modal="true" aria-label="Use your own Chrome">
        <div className="browse-setup__head">
          <Icon icon="solar:global-linear" width={18} height={18} />
          <h2>Use your own Chrome</h2>
        </div>
        <p className="browse-setup__lead">
          The built-in browser is the agent's own: it shows you what the agent does, but it has none of your
          logins and anti-bot checks block it. To let the agent use your Chrome, with your accounts, open tabs
          and extensions, set up OpenCode Browse.
        </p>
        <ol className="browse-setup__steps">
          <li>
            <span>Install the Browse extension in Chrome.</span>
            <button type="button" className="browse-setup__btn" onClick={() => void api.openExternalUrl(SETUP_PAGE)}>
              <Icon icon="solar:square-top-down-linear" width={13} height={13} />
              Open the setup page
            </button>
          </li>
          <li>
            <span>Connect it: run this in a terminal (Windows side).</span>
            <div className="browse-setup__command">
              <code>{INSTALL_COMMAND}</code>
              <button
                type="button"
                className="browse-setup__btn"
                onClick={() => {
                  void navigator.clipboard.writeText(INSTALL_COMMAND).then(() => setCopied(true));
                }}
              >
                <Icon icon={copied ? "solar:check-read-linear" : "solar:copy-linear"} width={13} height={13} />
                {copied ? "Copied" : "Copy"}
              </button>
            </div>
          </li>
          <li>
            <span>Turn it on in Sinew.</span>
            <button type="button" className="browse-setup__btn browse-setup__btn--primary" onClick={enable}>
              Enable Browse
            </button>
          </li>
        </ol>
        {error && <p className="browse-setup__error">{error}</p>}
        <div className="browse-setup__foot">
          <span>You can turn it on later in Settings → MCP.</span>
          <button type="button" className="browse-setup__btn" onClick={notNow}>
            Not now
          </button>
        </div>
      </div>
    </div>
  );
}
