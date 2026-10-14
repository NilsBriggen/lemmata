// texlab over Tauri IPC: completion, hover, go to definition, references,
// and rename. The outline comes from lemmata-project, which follows \input.
import {
  LSPClient, LSPPlugin, Workspace, type Transport, type WorkspaceFile,
  hoverTooltips, jumpToDefinitionKeymap, findReferencesKeymap, renameKeymap,
} from "@codemirror/lsp-client";
import { keymap, type EditorView } from "@codemirror/view";
import type { Extension, Text } from "@codemirror/state";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Project-relative path ↔ file:// URI. */
export function toUri(root: string, rel: string): string {
  const abs = `${root.replace(/[\\/]+$/, "")}/${rel}`.replace(/\\/g, "/");
  const withSlash = abs.startsWith("/") ? abs : `/${abs}`; // Windows drive paths
  return `file://${encodeURI(withSlash).replace(/#/g, "%23").replace(/\?/g, "%3F")}`;
}

export function fromUri(root: string, uri: string): string | null {
  if (!uri.startsWith("file://")) return null;
  let path = decodeURIComponent(uri.slice("file://".length));
  if (/^\/[A-Za-z]:\//.test(path)) path = path.slice(1);
  const base = root.replace(/\\/g, "/").replace(/\/+$/, "") + "/";
  const norm = path.replace(/\\/g, "/");
  if (!norm.toLowerCase().startsWith(base.toLowerCase())) return null;
  return norm.slice(base.length);
}

class File implements WorkspaceFile {
  constructor(
    public uri: string,
    public languageId: string,
    public version: number,
    public doc: Text,
    public view: EditorView,
  ) {}
  getView() {
    return this.view;
  }
}

/** One editor view whose state is swapped per file, plus a way to open
 *  other project files for go to definition and references. */
class ProjectWorkspace extends Workspace {
  files: File[] = [];
  private versions: Record<string, number> = Object.create(null);

  constructor(client: LSPClient, private display: (uri: string) => Promise<EditorView | null>) {
    super(client);
  }

  private nextVersion(uri: string) {
    return (this.versions[uri] = (this.versions[uri] ?? -1) + 1);
  }

  syncFiles() {
    const result = [];
    for (const file of this.files) {
      const plugin = LSPPlugin.get(file.view);
      if (!plugin || plugin.uri !== file.uri) continue;
      const changes = plugin.unsyncedChanges;
      if (!changes.empty) {
        result.push({ changes, file, prevDoc: file.doc });
        file.doc = file.view.state.doc;
        file.version = this.nextVersion(file.uri);
        plugin.clear();
      }
    }
    return result;
  }

  openFile(uri: string, languageId: string, view: EditorView) {
    this.closeFile(uri);
    const file = new File(uri, languageId, this.nextVersion(uri), view.state.doc, view);
    this.files.push(file);
    this.client.didOpen(file);
  }

  closeFile(uri: string) {
    if (!this.getFile(uri)) return;
    this.files = this.files.filter((f) => f.uri !== uri);
    this.client.didClose(uri);
  }

  async displayFile(uri: string) {
    return this.display(uri);
  }
}

export interface LanguageServer {
  client: LSPClient;
  /** Editor extensions for one file. */
  extensions(rel: string): Extension;
  stop(): void;
}

/** Starts texlab for the open project, or returns null if it isn't available. */
export async function startLanguageServer(
  root: string,
  display: (rel: string) => Promise<EditorView | null>,
  /** Called once if the server stops accepting messages, so the app can say so. */
  onProblem?: (message: string) => void,
): Promise<LanguageServer | null> {
  const handlers = new Set<(m: string) => void>();
  let generation = -1;
  const pending: { generation: number; body: string }[] = [];
  let reported = false;

  const unlisten: UnlistenFn = await listen<{ generation: number; body: string }>("lsp-message", (e) => {
    if (generation < 0) pending.push(e.payload);
    else if (e.payload.generation === generation) handlers.forEach((h) => h(e.payload.body));
  });

  try {
    generation = await invoke<number>("lsp_start");
  } catch (e) {
    unlisten();
    console.info("language server unavailable:", e);
    return null;
  }

  const transport: Transport = {
    send(message) {
      invoke("lsp_send", { generation, body: message }).catch((e) => {
        if (!reported) {
          reported = true;
          console.warn("language server stopped answering:", e);
          onProblem?.("The language server (texlab) stopped answering, so completion and hover may be missing. Reopen the project to restart it.");
        }
      });
    },
    subscribe(h) {
      handlers.add(h);
      for (const m of pending.splice(0)) if (m.generation === generation) h(m.body);
    },
    unsubscribe(h) {
      handlers.delete(h);
    },
  };

  const client = new LSPClient({
    rootUri: toUri(root, ""),
    timeout: 5000,
    workspace: (c) =>
      new ProjectWorkspace(c, async (uri) => {
        const rel = fromUri(root, uri);
        return rel ? display(rel) : null;
      }),
  }).connect(transport);

  const shared: Extension = [
    hoverTooltips(),
    keymap.of([...jumpToDefinitionKeymap, ...findReferencesKeymap, ...renameKeymap]),
  ];

  return {
    client,
    extensions: (rel) => [client.plugin(toUri(root, rel), "latex"), shared],
    stop() {
      client.disconnect();
      unlisten();
    },
  };
}
