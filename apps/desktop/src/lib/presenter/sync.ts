// The audience window and the presenter window follow each other through
// app-wide events: whoever changes slide says so, the other one follows.
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";

const PAGE = "present-page";
const END = "present-end";

export const sendPage = (page: number) => emit(PAGE, { page }).catch(() => {});
export const sendEnd = () => emit(END, {}).catch(() => {});

export const onPage = (cb: (page: number) => void): Promise<UnlistenFn> => listen<{ page: number }>(PAGE, (e) => cb(e.payload.page));
export const onEnd = (cb: () => void): Promise<UnlistenFn> => listen(END, () => cb());
