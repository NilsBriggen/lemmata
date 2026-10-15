// Invoke wrappers and small pure helpers for the Git panel. The commands live
// in src-tauri/src/git.rs (crate lemmata-git).
import { invoke } from "@tauri-apps/api/core";

export interface FileChange {
  path: string;
  /** M modified, A added, D deleted, R renamed, ? untracked, U conflicted. */
  status: string;
  from: string | null;
}

export interface GitStatus {
  /** null while HEAD is detached. */
  branch: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  hasCommits: boolean;
  staged: FileChange[];
  unstaged: FileChange[];
  untracked: FileChange[];
  conflicted: FileChange[];
}

export type Overview = ({ state: "repo" } & GitStatus) | { state: "not-a-repo" } | { state: "no-git"; message: string };

export interface Commit {
  hash: string;
  subject: string;
  author: string;
  date: string;
}

export const gitApi = {
  overview: () => invoke<Overview>("git_overview"),
  /** git init plus a first commit of everything. */
  init: () => invoke<void>("git_init"),
  stage: (paths: string[]) => invoke<void>("git_stage", { paths }),
  unstage: (paths: string[]) => invoke<void>("git_unstage", { paths }),
  stageAll: () => invoke<void>("git_stage_all"),
  commit: (message: string) => invoke<void>("git_commit", { message }),
  fetch: () => invoke<string>("git_fetch"),
  /** Fast-forward only. */
  pull: () => invoke<string>("git_pull"),
  push: () => invoke<string>("git_push"),
  branches: () => invoke<string[]>("git_branches"),
  /** Refused while there are uncommitted changes. */
  switchBranch: (name: string) => invoke<void>("git_switch", { name }),
  createBranch: (name: string) => invoke<void>("git_create_branch", { name }),
  log: () => invoke<Commit[]>("git_log"),
  /** Clones into parent/name (empty name: taken from the URL) and returns the new folder. Needs no open project. */
  clone: (url: string, parent: string, name: string) => invoke<string>("git_clone", { url, parent, name }),
};

/** The folder git would pick for a URL: https://git.overleaf.com/abc123 gives abc123. */
export function repoName(url: string): string {
  const u = url.trim().replace(/\/+$/, "").replace(/\.git$/, "");
  return u.split(/[/:\\]/).pop() || "";
}

/** "↑2 ↓1", or "" when the branch is level with its upstream. */
export function aheadBehind(s: Pick<GitStatus, "ahead" | "behind">): string {
  return [s.ahead ? `↑${s.ahead}` : "", s.behind ? `↓${s.behind}` : ""].filter(Boolean).join(" ");
}

/** What the commit button needs: something staged and a message. */
export function commitBlocker(s: Pick<GitStatus, "staged">, message: string): string | null {
  if (!s.staged.length) return "Tick the files to include in the commit.";
  if (!message.trim()) return "Write a commit message.";
  return null;
}

/** The cloned URL looks like an Overleaf project, whose git access needs a token. */
export const isOverleaf = (url: string) => /(^|[/@.])git\.overleaf\.com\b/i.test(url);
