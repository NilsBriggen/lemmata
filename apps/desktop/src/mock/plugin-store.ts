// plugin-store backed by localStorage, one key per store file.
export interface Store {
  get<T>(key: string): Promise<T | undefined>;
  set(key: string, value: unknown): Promise<void>;
}
export async function load(file: string): Promise<Store> {
  const read = () => JSON.parse(localStorage.getItem(`mock-store:${file}`) ?? "{}");
  return {
    async get(key) {
      return read()[key];
    },
    async set(key, value) {
      const data = read();
      data[key] = value;
      localStorage.setItem(`mock-store:${file}`, JSON.stringify(data));
    },
  };
}
