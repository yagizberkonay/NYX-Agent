import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowUpRight,
  Check,
  ChevronDown,
  Command,
  FolderOpen,
  Info,
  Mic,
  MoreHorizontal,
  PanelRight,
  Pause,
  Plus,
  Settings2,
  ShieldCheck,
  Sparkles,
  Square,
  Volume2,
} from "lucide-react";

type OrbState = "idle" | "working" | "completed" | "error";
type ActivityStatus = "started" | "running" | "success" | "error" | "waiting_for_permission" | "cancelled";

type ActivityEvent = {
  event_id: string;
  task_id: string;
  timestamp: string;
  status: ActivityStatus;
  message: string;
  tool?: string;
  target?: string;
  duration_ms?: number;
  error_code?: string;
};

type TaskResult = {
  task_id: string;
  status: string;
  verified: boolean;
  summary: string;
};

type ConnectorDescriptor = {
  server: string;
  display_name: string;
  enabled: boolean;
  purpose: string;
};

type ToolDescriptor = {
  name: string;
  description: string;
  permission: "allow" | "ask" | "deny";
  target: string;
};

const demoEvents: ActivityEvent[] = [
  {
    event_id: "demo-1",
    task_id: "demo",
    timestamp: new Date().toISOString(),
    status: "success",
    message: "Runtime hazır · yerel mod",
    target: "host",
  },
];

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function formatTime(timestamp: string) {
  return new Intl.DateTimeFormat("tr-TR", { hour: "2-digit", minute: "2-digit" }).format(new Date(timestamp));
}

function App() {
  const [orbState, setOrbState] = useState<OrbState>("idle");
  const [request, setRequest] = useState("");
  const [workspace, setWorkspace] = useState(".");
  const [activity, setActivity] = useState<ActivityEvent[]>(demoEvents);
  const [tools, setTools] = useState<ToolDescriptor[]>([]);
  const [connectors, setConnectors] = useState<ConnectorDescriptor[]>([]);
  const [lastResult, setLastResult] = useState<TaskResult | null>(null);
  const [showActivity, setShowActivity] = useState(true);
  const [notice, setNotice] = useState("Hazır");

  const latestActivity = activity[activity.length - 1];
  const isWorking = orbState === "working";

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        const [availableTools, availableConnectors] = await Promise.all([
          invoke<ToolDescriptor[]>("list_tools"),
          invoke<ConnectorDescriptor[]>("list_connectors"),
        ]);
        setTools(availableTools);
        setConnectors(availableConnectors);
        await invoke("runtime_health");
        unlisten = await listen<ActivityEvent>("nyx://activity", (event) => {
          setActivity((current) => [...current.slice(-19), event.payload]);
          if (["started", "running"].includes(event.payload.status)) setOrbState("working");
          if (event.payload.status === "success") setOrbState("completed");
          if (event.payload.status === "error") setOrbState("error");
          setNotice(event.payload.message);
        });
      } catch (error) {
        setNotice(`Runtime bağlantı hatası: ${String(error)}`);
      }
    })();
    return () => unlisten?.();
  }, []);

  const activeTools = useMemo(() => tools.filter((tool) => tool.permission !== "deny"), [tools]);

  async function runTask() {
    const cleanRequest = request.trim() || "Çalışma alanımı analiz et";
    setOrbState("working");
    setNotice("İstek analiz ediliyor");
    const started: ActivityEvent = {
      event_id: crypto.randomUUID(),
      task_id: "local",
      timestamp: new Date().toISOString(),
      status: "started",
      message: "İstek analiz ediliyor",
      target: "host",
    };
    setActivity((current) => [...current.slice(-19), started]);

    if (!isTauriRuntime()) {
      setOrbState("error");
      setNotice("NYX masaüstü runtime’ı gerekli · Tauri uygulamasını açın");
      setActivity((current) => [...current.slice(-19), {
        event_id: crypto.randomUUID(),
        task_id: "browser",
        timestamp: new Date().toISOString(),
        status: "error",
        message: "Web demo görev çalıştırmaz; yerel Tauri runtime gerekli",
        target: "browser",
      }]);
      return;
    }

    try {
      const result = await invoke<TaskResult>("start_task", { request: cleanRequest, workspaceRoot: workspace });
      setLastResult(result);
      setOrbState(result.verified ? "completed" : "error");
      setNotice(result.verified ? "Doğrulama başarılı" : "İnceleme gerekiyor");
    } catch (error) {
      const failure: ActivityEvent = {
        event_id: crypto.randomUUID(),
        task_id: "local",
        timestamp: new Date().toISOString(),
        status: "error",
        message: String(error),
        target: "host",
      };
      setActivity((current) => [...current.slice(-19), failure]);
      setOrbState("error");
      setNotice("Görev tamamlanamadı");
    }
  }

  async function stopTask() {
    if (isTauriRuntime()) {
      try {
        await invoke("stop_task");
      } catch (error) {
        setNotice(`Durdurma hatası: ${String(error)}`);
      }
    }
    setOrbState("idle");
    setNotice("Görev durduruldu");
    setActivity((current) => [
      ...current.slice(-19),
      {
        event_id: crypto.randomUUID(),
        task_id: "local",
        timestamp: new Date().toISOString(),
        status: "cancelled",
        message: "Görev kullanıcı tarafından durduruldu",
        target: "host",
      },
    ]);
  }

  const orbLabel = {
    idle: "Dinliyor",
    working: "Çalışıyor",
    completed: "Tamamlandı",
    error: "İnceleme gerekiyor",
  }[orbState];

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand-lockup">
          <div className="brand-mark" aria-hidden="true"><Sparkles size={15} strokeWidth={1.8} /></div>
          <div>
            <div className="brand-name">NYX</div>
            <div className="brand-caption">local-first computer agent</div>
          </div>
        </div>
        <div className="topbar-actions">
          <div className="privacy-pill"><ShieldCheck size={14} /><span>Yerel mod</span><span className="status-dot" /></div>
          <button className="icon-button" aria-label="Ayarlar"><Settings2 size={18} /></button>
          <button className="avatar-button" aria-label="Profil">Y</button>
        </div>
      </header>

      <section className="workspace-grid">
        <section className="hero-panel" aria-label="NYX ana alanı">
          <div className="hero-context">
            <div className="eyebrow"><span className="eyebrow-line" /> Bugün, 28 Ağustos</div>
            <button className="workspace-selector" onClick={() => setWorkspace(workspace === "." ? "./workspace" : ".")}>
              <FolderOpen size={15} /> <span>{workspace === "." ? "Aktif çalışma alanı" : workspace}</span> <ChevronDown size={14} />
            </button>
          </div>

          <div className={`orb-stage orb-${orbState}`}>
            <div className="orb-halo halo-one" />
            <div className="orb-halo halo-two" />
            <div className="orb-shell">
              <div className="orb-glow" />
              <div className="orb-core"><span>NYX</span></div>
              <div className="orb-ring ring-one" />
              <div className="orb-ring ring-two" />
            </div>
            <div className="orb-state-label"><span className="state-dot" /> {orbLabel}</div>
          </div>

          <div className="hero-footer">
            <div className="status-copy"><span className="status-kicker">NYX STATUS</span><strong>{notice}</strong></div>
            <div className="hero-actions">
              <button className="round-action" aria-label="Mikrofonu aç"><Mic size={18} /></button>
              <button className="round-action" aria-label="Sesi aç"><Volume2 size={18} /></button>
              <button className="round-action" aria-label="Daha fazla seçenek"><MoreHorizontal size={18} /></button>
            </div>
          </div>
        </section>

        <aside className={`activity-panel ${showActivity ? "visible" : "collapsed"}`}>
          <div className="panel-heading">
            <div><span className="panel-kicker">LIVE ACTIVITY</span><h2>Çalışma akışı</h2></div>
            <button className="icon-button" onClick={() => setShowActivity(false)} aria-label="Aktiviteyi kapat"><PanelRight size={17} /></button>
          </div>
          <div className="activity-list">
            {activity.slice().reverse().map((event) => (
              <div className="activity-item" key={event.event_id}>
                <div className={`activity-icon activity-${event.status}`}>
                  {event.status === "success" ? <Check size={13} /> : event.status === "cancelled" ? <Square size={11} /> : <span />}
                </div>
                <div className="activity-body"><div className="activity-message">{event.message}</div><div className="activity-meta">{event.tool ?? "runtime"} · {formatTime(event.timestamp)}{event.duration_ms ? ` · ${event.duration_ms} ms` : ""}</div></div>
              </div>
            ))}
          </div>
          <div className="activity-footer"><ShieldCheck size={14} /><span>İşlemler audit akışına kaydedilir</span></div>
        </aside>
      </section>

      <section className="command-dock" aria-label="NYX komut girişi">
        <div className="dock-context"><Command size={16} /><span>NYX'e ne yaptırmak istersiniz?</span></div>
        <textarea value={request} onChange={(event) => setRequest(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); void runTask(); } }} placeholder="Örn. Projeyi incele, testleri çalıştır ve sonucu özetle…" rows={2} />
        <div className="dock-bottom"><div className="dock-hints"><span><kbd>↵</kbd> gönder</span><span><kbd>⇧ ↵</kbd> yeni satır</span><button className="add-button"><Plus size={14} /> ekle</button></div><div className="dock-actions"><button className="secondary-button" onClick={() => setShowActivity(true)}><PanelRight size={15} /> Akış</button>{isWorking ? <button className="stop-button" onClick={stopTask}><Pause size={15} /> Durdur</button> : <button className="send-button" onClick={() => void runTask()}>Başlat <ArrowUpRight size={16} /></button>}</div></div>
      </section>

      <footer className="bottom-meta"><div><span className="meta-label">MODEL</span><span>NYX Local / BYOK ready</span></div><div><span className="meta-label">VOICE</span><span>Whisper STT · Qwen3-TTS Türkçe deneysel</span></div><div><span className="meta-label">TOOLS</span><span>{activeTools.length} native · {connectors.length} MCP</span></div><div className="footer-help"><Info size={14} /> Tauri runtime required</div></footer>
      {lastResult?.verified && <div className="verified-toast"><Check size={15} /> {lastResult.summary}</div>}
    </main>
  );
}

export default App;
