// A second browser tab stands in for the presenter window.
export class WebviewWindow {
  constructor(
    public label: string,
    options: { url?: string } = {},
  ) {
    window.open(`/${options.url?.replace(/^index\.html/, "") ?? ""}`, label, "popup,width=1180,height=740");
  }
  static async getByLabel() {
    return null;
  }
  async once(_event: string, cb: () => void) {
    queueMicrotask(cb);
    return () => {};
  }
  async close() {}
}
