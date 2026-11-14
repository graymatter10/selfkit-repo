// Pick an image, shrink it to ≤ 2400 px on the long edge (document quality,
// small print stays readable) and store it as WebP through Rust. Falls back
// to JPEG only when the webview cannot encode WebP.
import { api } from "./api";

const MAX = 2400;
const urls = new Map<string, string>();

export function pickImage(): Promise<File | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "image/*";
    input.onchange = () => resolve(input.files?.[0] ?? null);
    input.oncancel = () => resolve(null);
    input.click();
  });
}

function toBlob(c: HTMLCanvasElement, type: string, q: number): Promise<Blob | null> {
  return new Promise((r) => c.toBlob(r, type, q));
}

export async function encode(file: Blob): Promise<Uint8Array> {
  const bmp = await createImageBitmap(file);
  const scale = Math.min(1, MAX / Math.max(bmp.width, bmp.height));
  const w = Math.round(bmp.width * scale), h = Math.round(bmp.height * scale);
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d")!;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(bmp, 0, 0, w, h);
  bmp.close?.();
  let b = await toBlob(c, "image/webp", 0.9);
  if (!b || b.type !== "image/webp") b = await toBlob(c, "image/jpeg", 0.92);
  if (!b) throw new Error("Could not read this image");
  return new Uint8Array(await b.arrayBuffer());
}

/** Pick → encode → store. Returns the photo id or null if cancelled. */
export async function addPhoto(): Promise<string | null> {
  const f = await pickImage();
  if (!f) return null;
  return api.photoSave(await encode(f));
}

export async function photoUrl(id: string): Promise<string> {
  const u = urls.get(id);
  if (u) return u;
  const buf = await api.photoGet(id);
  const type = id.endsWith(".webp") ? "image/webp" : id.endsWith(".png") ? "image/png" : "image/jpeg";
  const url = URL.createObjectURL(new Blob([buf], { type }));
  urls.set(id, url);
  return url;
}
