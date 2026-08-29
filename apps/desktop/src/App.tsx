import { useState } from "react";
import { DecodeView } from "./components/DecodeView";
import { EncodeView } from "./components/EncodeView";
import { InfoView } from "./components/InfoView";
import { PlanExplorerView } from "./components/PlanExplorerView";

type Tab = "encode" | "decode" | "info" | "plan";

const TABS: { id: Tab; label: string }[] = [
  { id: "encode", label: "Encode" },
  { id: "decode", label: "Decode" },
  { id: "info", label: "Info" },
  { id: "plan", label: "Plan explorer" },
];

export default function App() {
  const [tab, setTab] = useState<Tab>("encode");

  return (
    <div className="flex h-screen flex-col">
      <header className="flex items-center gap-1 border-b border-neutral-300 bg-white/80 px-4 py-2 dark:border-neutral-800 dark:bg-black/80">
        <span className="mr-4 text-sm font-semibold tracking-wide text-teal-600 dark:text-teal-400">
          stego-flac
        </span>
        {TABS.map((item) => (
          <button
            key={item.id}
            onClick={() => setTab(item.id)}
            className={`rounded-md px-3 py-1.5 text-sm font-medium transition-colors ${
              tab === item.id
                ? "bg-neutral-200 text-black dark:bg-neutral-800 dark:text-white"
                : "text-neutral-500 hover:bg-neutral-100 hover:text-black dark:text-neutral-400 dark:hover:bg-neutral-900 dark:hover:text-neutral-200"
            }`}
          >
            {item.label}
          </button>
        ))}
      </header>
      <main className="flex-1 overflow-y-auto px-6 py-6">
        {tab === "encode" && <EncodeView />}
        {tab === "decode" && <DecodeView />}
        {tab === "info" && <InfoView />}
        {tab === "plan" && <PlanExplorerView />}
      </main>
    </div>
  );
}
