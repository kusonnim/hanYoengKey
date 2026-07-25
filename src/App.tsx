import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type ProviderPreference =
  | "automatic"
  | "uiAutomationOnly"
  | "clipboardOnly";

type Settings = {
  launchAtStartup: boolean;
  enableConversion: boolean;
  hotkeyMode: "hangulEnglishKey";
  selectionProvider: ProviderPreference;
  replacementProvider: ProviderPreference;
  debugLogging: boolean;
};

const providerOptions: Array<[ProviderPreference, string]> = [
  ["automatic", "Automatic"],
  ["uiAutomationOnly", "UI Automation only"],
  ["clipboardOnly", "Clipboard only"],
];

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [status, setStatus] = useState("Loading settings…");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    invoke<Settings>("load_settings")
      .then((loaded) => {
        setSettings(loaded);
        setStatus("Changes save automatically");
      })
      .catch((error) => setStatus(`Could not load settings: ${String(error)}`));
  }, []);

  async function update(next: Settings) {
    const previous = settings;
    setSettings(next);
    setBusy(true);
    setStatus("Saving…");
    try {
      const saved = await invoke<Settings>("save_settings", { settings: next });
      setSettings(saved);
      setStatus("Saved");
    } catch (error) {
      setSettings(previous);
      setStatus(`Could not save: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  async function resetDefaults() {
    setBusy(true);
    setStatus("Resetting…");
    try {
      const defaults = await invoke<Settings>("reset_defaults");
      setSettings(defaults);
      setStatus("Defaults restored");
    } catch (error) {
      setStatus(`Could not reset: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  if (!settings) {
    return (
      <main className="settings-shell">
        <p className="status">{status}</p>
      </main>
    );
  }

  return (
    <main className="settings-shell">
      <header>
        <div>
          <h1>HanYeongKey</h1>
          <p>Conversion settings</p>
        </div>
        <span className="status" aria-live="polite">
          {status}
        </span>
      </header>

      <section aria-label="General settings">
        <ToggleRow
          label="Enable conversion"
          description="Convert selected text with the Hangul/English key."
          checked={settings.enableConversion}
          disabled={busy}
          onChange={(checked) =>
            update({ ...settings, enableConversion: checked })
          }
        />
        <ToggleRow
          label="Launch at Windows startup"
          description="Start HanYeongKey when you sign in."
          checked={settings.launchAtStartup}
          disabled={busy}
          onChange={(checked) =>
            update({ ...settings, launchAtStartup: checked })
          }
        />
        <ToggleRow
          label="Debug logging"
          description="Write additional diagnostic categories without text contents."
          checked={settings.debugLogging}
          disabled={busy}
          onChange={(checked) => update({ ...settings, debugLogging: checked })}
        />
      </section>

      <section className="select-grid" aria-label="Input and provider settings">
        <SelectRow
          label="Hotkey mode"
          value={settings.hotkeyMode}
          disabled={busy}
          onChange={() => undefined}
          options={[["hangulEnglishKey", "Hangul/English key"]]}
        />
        <SelectRow
          label="Selection provider"
          value={settings.selectionProvider}
          disabled={busy}
          onChange={(selectionProvider) =>
            update({
              ...settings,
              selectionProvider: selectionProvider as ProviderPreference,
            })
          }
          options={providerOptions}
        />
        <SelectRow
          label="Replacement provider"
          value={settings.replacementProvider}
          disabled={busy}
          onChange={(replacementProvider) =>
            update({
              ...settings,
              replacementProvider: replacementProvider as ProviderPreference,
            })
          }
          options={providerOptions}
        />
      </section>

      <footer>
        <button type="button" onClick={resetDefaults} disabled={busy}>
          Reset defaults
        </button>
      </footer>
    </main>
  );
}

type ToggleRowProps = {
  label: string;
  description: string;
  checked: boolean;
  disabled: boolean;
  onChange: (checked: boolean) => void;
};

function ToggleRow(props: ToggleRowProps) {
  return (
    <label className="toggle-row">
      <span>
        <strong>{props.label}</strong>
        <small>{props.description}</small>
      </span>
      <input
        type="checkbox"
        checked={props.checked}
        disabled={props.disabled}
        onChange={(event) => props.onChange(event.target.checked)}
      />
    </label>
  );
}

type SelectRowProps = {
  label: string;
  value: string;
  disabled: boolean;
  onChange: (value: string) => void;
  options: Array<[string, string]>;
};

function SelectRow(props: SelectRowProps) {
  return (
    <label>
      <span>{props.label}</span>
      <select
        value={props.value}
        disabled={props.disabled}
        onChange={(event) => props.onChange(event.target.value)}
      >
        {props.options.map(([value, label]) => (
          <option key={value} value={value}>
            {label}
          </option>
        ))}
      </select>
    </label>
  );
}
